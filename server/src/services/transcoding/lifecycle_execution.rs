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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;
use tokio_process_tools::{Consumable, GracefulShutdown, LineParsingOptions, Next, NumBytesExt};

use super::super::FfmpegHandle;
use super::super::failure::{FailureClass, FailureHints};
use super::super::progress::ProgressParser;
use super::super::watchdog::ProgressWatchdog;

pub(super) struct SupervisedExecution {
    pub(super) outcome: Result<ExecutionState, ()>,
    pub(super) failure: Option<FailureClass>,
}

pub(super) async fn supervise(
    process: &mut FfmpegHandle,
    requests: &mut watch::Receiver<u64>,
    shutdown: GracefulShutdown,
    mut inspect: impl FnMut(Cow<'_, str>) -> Next + Send + 'static,
    timeout: Duration,
) -> SupervisedExecution {
    let watchdog = Arc::new(Mutex::new(ProgressWatchdog::new(timeout)));
    let advances = Arc::clone(&watchdog);
    let mut parser = ProgressParser::default();
    let stdout = process
        .stdout()
        .consume(tokio_process_tools::ParseLines::inspect(
            bounded_lines(),
            move |line: Cow<'_, str>| {
                if let Some(progress) = parser.push(&line) {
                    advances
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .observe(progress.out_time_ms);
                }
                inspect(line)
            },
        ));
    let hints = Arc::new(Mutex::new(FailureHints::default()));
    let stderr_hints = Arc::clone(&hints);
    let stderr = process
        .stderr()
        .consume(tokio_process_tools::ParseLines::inspect(
            bounded_lines(),
            move |line: Cow<'_, str>| {
                stderr_hints
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .inspect(&line);
                Next::Continue
            },
        ));
    let mut classify_failure = false;
    let outcome = match stdout {
        Ok(stdout) => {
            let stdout = stdout.wait();
            tokio::pin!(stdout);
            let mut stdout_open = true;
            let outcome = loop {
                tokio::select! {
                    biased;
                    _ = super::wait_for_cancellation(requests) => {
                        break process.terminate(shutdown.clone()).await.map(|_| ExecutionState::Cancelled).map_err(|_| ());
                    },
                    result = process.wait_for_completion(Duration::from_secs(3600)).or_terminate(shutdown.clone()) => {
                        classify_failure = matches!(&result, Ok(tokio_process_tools::WaitForCompletionOrTerminateResult::Completed(status)) if !status.success());
                        break result.map(execution_result).map_err(|_| ());
                    },
                    result = &mut stdout, if stdout_open => {
                        stdout_open = false;
                        if result.is_err() {
                            break process.terminate(shutdown.clone()).await.map(|_| ExecutionState::Failed).map_err(|_| ());
                        }
                    },
                    _ = ProgressWatchdog::expired(Arc::clone(&watchdog)) => {
                        break process.terminate(shutdown.clone()).await.map(|_| ExecutionState::Stalled).map_err(|_| ());
                    },
                }
            };
            if stdout_open {
                let _ = tokio::time::timeout(Duration::from_secs(1), &mut stdout).await;
            }
            outcome
        }
        Err(_) => process
            .terminate(shutdown)
            .await
            .map(|_| ExecutionState::Failed)
            .map_err(|_| ()),
    };
    let stderr_complete = if let Ok(stderr) = stderr {
        tokio::time::timeout(Duration::from_secs(1), stderr.wait())
            .await
            .is_ok_and(|result| result.is_ok())
    } else {
        false
    };
    let failure = match outcome {
        Ok(ExecutionState::Stalled) => Some(
            if !stderr_complete
                || hints
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .blocks_recovery()
            {
                FailureClass::Unknown
            } else {
                FailureClass::Stalled
            },
        ),
        Ok(ExecutionState::Failed) if classify_failure && stderr_complete => Some(
            hints
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .classify(),
        ),
        Ok(ExecutionState::Failed) | Err(()) => Some(FailureClass::Unknown),
        _ => None,
    };
    SupervisedExecution { outcome, failure }
}

fn bounded_lines() -> LineParsingOptions {
    LineParsingOptions {
        max_line_length: 4096.bytes(),
        ..LineParsingOptions::default()
    }
}

fn execution_result(
    result: tokio_process_tools::WaitForCompletionOrTerminateResult,
) -> ExecutionState {
    match result {
        tokio_process_tools::WaitForCompletionOrTerminateResult::Completed(status)
            if status.success() =>
        {
            ExecutionState::Succeeded
        }
        _ => ExecutionState::Failed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionState {
    Running,
    Succeeded,
    Failed,
    Stalled,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerSnapshot {
    pub(crate) state: ExecutionState,
    pub(crate) failure: Option<FailureClass>,
}

#[derive(Clone)]
pub(crate) struct WorkerObservation {
    state: watch::Receiver<ExecutionState>,
    cancellation: watch::Receiver<u64>,
    failure: Arc<Mutex<Option<FailureClass>>>,
}

impl WorkerObservation {
    pub(super) fn new(
        state: watch::Receiver<ExecutionState>,
        cancellation: watch::Receiver<u64>,
        failure: Arc<Mutex<Option<FailureClass>>>,
    ) -> Self {
        Self {
            state,
            cancellation,
            failure,
        }
    }

    pub(crate) fn snapshot(&self) -> WorkerSnapshot {
        let mut state = *self.state.borrow();
        if state == ExecutionState::Running {
            if *self.cancellation.borrow() > 0 {
                state = ExecutionState::Cancelled;
            } else if self.state.has_changed().is_err() || self.cancellation.has_changed().is_err()
            {
                state = ExecutionState::Failed;
            }
        }
        let failure = match state {
            ExecutionState::Failed | ExecutionState::Stalled => Some(
                self.failure
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .unwrap_or(FailureClass::Unknown),
            ),
            _ => None,
        };
        WorkerSnapshot { state, failure }
    }
}

pub(crate) struct ExecutionMonitor {
    state: watch::Receiver<ExecutionState>,
    cancellation: watch::Receiver<u64>,
}

impl ExecutionMonitor {
    pub(crate) fn new(
        state: watch::Receiver<ExecutionState>,
        cancellation: watch::Receiver<u64>,
    ) -> Self {
        Self {
            state,
            cancellation,
        }
    }

    pub(crate) fn state(&self) -> ExecutionState {
        if *self.cancellation.borrow() > 0 {
            return ExecutionState::Cancelled;
        }
        let state = *self.state.borrow();
        if state == ExecutionState::Running
            && (self.state.has_changed().is_err() || self.cancellation.has_changed().is_err())
        {
            ExecutionState::Failed
        } else {
            state
        }
    }

    pub(crate) async fn changed(&mut self) {
        tokio::select! {
            _ = self.state.changed() => {},
            _ = self.cancellation.changed() => {},
        }
    }
}
