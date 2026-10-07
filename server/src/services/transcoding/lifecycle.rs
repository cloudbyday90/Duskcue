use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, watch};
use tokio_process_tools::{Consumable, GracefulShutdown, LineParsingOptions, Next};
use uuid::Uuid;

use super::FfmpegHandle;
use crate::domains::playback::PlaybackError;

struct WorkerControl {
    cancellation: watch::Sender<u64>,
    result: watch::Receiver<Option<Completion>>,
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

#[derive(Default)]
pub(super) struct TranscodeWorkers {
    workers: Arc<DashMap<Uuid, Arc<WorkerControl>>>,
}

pub(super) struct WorkerLaunch {
    pub session_id: Uuid,
    pub handle: FfmpegHandle,
    pub permit: OwnedSemaphorePermit,
    pub directory: PathBuf,
    pub shutdown: GracefulShutdown,
}

pub(super) struct WorkerCancellation {
    sender: watch::Sender<u64>,
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
        let WorkerLaunch {
            session_id,
            handle,
            permit,
            directory,
            shutdown,
        } = launch;
        let (cancellation, mut requests) = watch::channel(0);
        let (completion, result) = watch::channel(None);
        self.workers.insert(
            session_id,
            Arc::new(WorkerControl {
                cancellation,
                result,
            }),
        );
        let registry = Arc::clone(&self.workers);
        tokio::spawn(async move {
            let mut process = handle.terminate_on_drop(shutdown.clone());
            let consumer = process
                .stdout()
                .consume(tokio_process_tools::ParseLines::inspect(
                    LineParsingOptions::default(),
                    inspect,
                ));
            let outcome = match consumer {
                Ok(consumer) => {
                    let terminate = tokio::select! {
                        _ = consumer.wait() => false,
                        _ = wait_for_cancellation(&mut requests) => true,
                    };
                    if terminate {
                        process
                            .terminate(shutdown.clone())
                            .await
                            .map(|_| ())
                            .map_err(|_| ())
                    } else {
                        tokio::select! {
                            result = process.wait_for_completion(Duration::from_secs(3600)).or_terminate(shutdown.clone()) => result.map(|_| ()).map_err(|_| ()),
                            _ = wait_for_cancellation(&mut requests) => process.terminate(shutdown.clone()).await.map(|_| ()).map_err(|_| ()),
                        }
                    }
                }
                Err(_) => process
                    .terminate(shutdown.clone())
                    .await
                    .map(|_| ())
                    .map_err(|_| ()),
            };
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
            drop(permit);
            if *requests.borrow_and_update() == 0 {
                let _ = requests.changed().await;
            }
            loop {
                match tokio::fs::remove_dir_all(&directory).await {
                    Ok(()) => {
                        cleanup();
                        report(&completion, &mut requests, Ok(()));
                        registry.remove(&session_id);
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        cleanup();
                        report(&completion, &mut requests, Ok(()));
                        registry.remove(&session_id);
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
                armed: true,
            })
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
