// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, watch};
use tokio_process_tools::{GracefulShutdown, Next};
use uuid::Uuid;

use super::FfmpegHandle;
use super::failure::FailureClass;
use super::watchdog::NO_PROGRESS_TIMEOUT;
use crate::domains::playback::PlaybackError;
use crate::services::vod_lifecycle::Generation;
use tokio_util::task::TaskTracker;

#[path = "lifecycle_execution.rs"]
mod execution;
pub(super) use execution::{ExecutionMonitor, ExecutionState, WorkerObservation};

struct WorkerControl {
    generation: Arc<Generation>,
    encoder: Mutex<Option<OwnedSemaphorePermit>>,
    #[cfg(test)]
    task: Mutex<Option<tokio::task::AbortHandle>>,
    cancellation: watch::Sender<u64>,
    result: watch::Receiver<Option<Completion>>,
    execution: watch::Receiver<ExecutionState>,
    failure: Arc<Mutex<Option<FailureClass>>>,
}

struct WorkerCompletion {
    generation: Arc<Generation>,
    done: bool,
}

impl WorkerCompletion {
    fn complete(&mut self) {
        self.done = true;
    }
}

impl Drop for WorkerCompletion {
    fn drop(&mut self) {
        if !self.done {
            self.generation.fail();
        }
    }
}

#[derive(Clone)]
struct Completion {
    request: u64,
    outcome: Result<(), String>,
}

fn report(
    completion: &watch::Sender<Option<Completion>>,
    requests: &mut watch::Receiver<u64>,
    outcome: Result<(), String>,
) {
    completion.send_replace(Some(Completion {
        request: *requests.borrow_and_update(),
        outcome,
    }));
}

#[derive(Default, Clone)]
pub(super) struct TranscodeWorkers {
    workers: Arc<DashMap<Uuid, Arc<WorkerControl>>>,
    tracker: TaskTracker,
    lifetime: Arc<()>,
}

pub(super) struct WorkerLaunch {
    pub generation: Arc<Generation>,
    pub session_id: Uuid,
    pub handle: FfmpegHandle,
    pub permit: OwnedSemaphorePermit,
    pub directory: PathBuf,
    pub shutdown: GracefulShutdown,
}

pub(super) struct WorkerCancellation {
    sender: watch::Sender<u64>,
    generation: Arc<Generation>,
    armed: bool,
}

impl WorkerCancellation {
    pub(super) fn acknowledge(&mut self) {
        self.armed = false;
    }
}

impl Drop for WorkerCancellation {
    fn drop(&mut self) {
        if self.armed {
            self.generation.retire();
            self.sender
                .send_modify(|request| *request = request.saturating_add(1));
        }
    }
}

impl TranscodeWorkers {
    pub(super) fn start(
        &self,
        launch: WorkerLaunch,
        inspect: impl FnMut(Cow<'_, str>) -> Next + Send + 'static,
        cleanup: impl FnOnce() + Send + 'static,
    ) {
        self.start_with_watchdog(launch, inspect, cleanup, NO_PROGRESS_TIMEOUT);
    }

    fn start_with_watchdog(
        &self,
        launch: WorkerLaunch,
        inspect: impl FnMut(Cow<'_, str>) -> Next + Send + 'static,
        cleanup: impl FnOnce() + Send + 'static,
        timeout: Duration,
    ) {
        let WorkerLaunch {
            generation,
            session_id,
            handle,
            permit,
            directory,
            shutdown,
        } = launch;
        let (cancellation, mut requests) = watch::channel(0);
        let supervisor_cancellation = cancellation.clone();
        let (completion, result) = watch::channel(None);
        let (execution, execution_state) = watch::channel(ExecutionState::Running);
        let failure = Arc::new(Mutex::new(None));
        let control = Arc::new(WorkerControl {
            generation: generation.clone(),
            encoder: Mutex::new(Some(permit)),
            #[cfg(test)]
            task: Mutex::new(None),
            cancellation,
            result,
            execution: execution_state,
            failure: Arc::clone(&failure),
        });
        self.workers.insert(session_id, control.clone());
        let registry = Arc::clone(&self.workers);
        let worker_control = control.clone();
        let mut worker_completion = WorkerCompletion {
            generation: generation.clone(),
            done: false,
        };
        let task = self.tracker.spawn(async move {
            let mut process = handle.terminate_on_drop(shutdown.clone());
            let execution::SupervisedExecution { outcome, failure: classified } = {
                let supervised = execution::supervise(
                    &mut process, &mut requests, shutdown.clone(), inspect, timeout,
                );
                tokio::pin!(supervised);
                tokio::select! {
                    result = &mut supervised => result,
                    _ = generation.stop.cancelled() => {
                        supervisor_cancellation.send_modify(|value| *value = value.saturating_add(1));
                        supervised.await
                    }
                }
            };
            let terminal_state = outcome.as_ref().copied().unwrap_or(ExecutionState::Failed);
            let terminal_state = if *requests.borrow() > 0 {
                ExecutionState::Cancelled
            } else {
                terminal_state
            };
            *failure
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = match terminal_state {
                ExecutionState::Failed | ExecutionState::Stalled => classified,
                _ => None,
            };
            execution.send_replace(terminal_state);
            if outcome.is_err() {
                report(
                    &completion,
                    &mut requests,
                    Err("FFmpeg termination could not be confirmed".into()),
                );
                loop {
                    let exited = tokio::select! {
                        result = process.wait_for_completion(Duration::from_secs(3600)) => result.is_ok_and(|result| physically_complete(&result)),
                        _ = requests.changed() => process.terminate(shutdown.clone()).await.is_ok(),
                    };
                    if exited {
                        break;
                    }
                    report(
                        &completion,
                        &mut requests,
                        Err("FFmpeg termination could not be confirmed".into()),
                    );
                }
            }
            drop(process);
            if let Ok(mut permit) = worker_control.encoder.lock() {
                permit.take();
            } else {
                generation.fail();
            }
            if *requests.borrow_and_update() == 0 {
                let _ = requests.changed().await;
            }
            loop {
                let cleanup_result = match generation.drain().await {
                    Ok(()) => crate::services::vod_lifecycle::remove_cache(&directory).await,
                    Err(error) => Err(error),
                };
                match cleanup_result {
                    Ok(()) => {
                        cleanup();
                        report(&completion, &mut requests, Ok(()));
                        registry.remove(&session_id);
                        worker_completion.complete();
                        return;
                    }
                    Err(_) => {
                        report(
                            &completion,
                            &mut requests,
                            Err("transcode cache cleanup failed".into()),
                        );
                        if requests.changed().await.is_err() {
                            return;
                        }
                    }
                }
            }
        });
        #[cfg(test)]
        {
            *control.task.lock().expect("test task registration") = Some(task.abort_handle());
        }
        #[cfg(not(test))]
        drop(task);
    }

    pub(super) async fn stop(&self, session_id: Uuid) -> Result<bool, PlaybackError> {
        let Some(control) = self
            .workers
            .get(&session_id)
            .map(|entry| Arc::clone(entry.value()))
        else {
            return Ok(false);
        };
        let mut result = control.result.clone();
        control.generation.retire();
        if result
            .borrow()
            .as_ref()
            .is_some_and(|completion| completion.outcome.is_ok())
        {
            return Ok(true);
        }
        let mut request = 0;
        control.cancellation.send_modify(|value| {
            *value = value.saturating_add(1);
            request = *value;
        });
        loop {
            if let Some(completion) = result.borrow_and_update().clone()
                && (completion.outcome.is_ok() || completion.request >= request)
            {
                return completion
                    .outcome
                    .map(|_| true)
                    .map_err(PlaybackError::FfmpegFailed);
            }
            if result.changed().await.is_err() {
                return Err(PlaybackError::FfmpegFailed(
                    "transcode worker completion was lost".into(),
                ));
            }
        }
    }

    pub(super) fn pending(&self, session_id: Uuid) -> Option<WorkerCancellation> {
        self.workers
            .get(&session_id)
            .map(|entry| WorkerCancellation {
                sender: entry.cancellation.clone(),
                generation: entry.generation.clone(),
                armed: true,
            })
    }

    pub(super) fn execution(&self, session_id: Uuid) -> Option<ExecutionMonitor> {
        self.workers.get(&session_id).map(|entry| {
            ExecutionMonitor::new(entry.execution.clone(), entry.cancellation.subscribe())
        })
    }

    pub(super) fn observation(&self, session_id: Uuid) -> Option<WorkerObservation> {
        self.workers.get(&session_id).map(|entry| {
            WorkerObservation::new(
                entry.execution.clone(),
                entry.cancellation.subscribe(),
                Arc::clone(&entry.failure),
            )
        })
    }

    pub(super) fn failure(&self, session_id: Uuid) -> Option<FailureClass> {
        self.observation(session_id)?.snapshot().failure
    }

    pub(super) fn generation(&self, session_id: Uuid) -> Option<Arc<Generation>> {
        self.workers
            .get(&session_id)
            .map(|entry| entry.generation.clone())
    }

    pub(super) fn cancel_all(&self) {
        for entry in self.workers.iter() {
            entry.generation.retire();
            entry
                .cancellation
                .send_modify(|value| *value = value.saturating_add(1));
        }
    }

    pub(super) async fn drain(&self) -> Result<(), PlaybackError> {
        self.cancel_all();
        self.tracker.close();
        self.tracker.wait().await;
        if self.workers.is_empty() {
            Ok(())
        } else {
            Err(PlaybackError::LifecycleCleanup)
        }
    }
}

async fn wait_for_cancellation(requests: &mut watch::Receiver<u64>) {
    if *requests.borrow() > 0 {
        return;
    }
    while requests.changed().await.is_ok() {
        if *requests.borrow_and_update() > 0 {
            return;
        }
    }
}

fn physically_complete<T>(result: &tokio_process_tools::WaitForCompletionResult<T>) -> bool {
    matches!(
        result,
        tokio_process_tools::WaitForCompletionResult::Completed(_)
    )
}

impl Drop for TranscodeWorkers {
    fn drop(&mut self) {
        if Arc::strong_count(&self.lifetime) != 1 {
            return;
        }
        for entry in self.workers.iter() {
            entry
                .cancellation
                .send_modify(|value| *value = value.saturating_add(1));
        }
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lifecycle_readiness_tests.rs"]
mod readiness_tests;

#[cfg(test)]
#[path = "lifecycle_watchdog_tests.rs"]
mod watchdog_tests;
