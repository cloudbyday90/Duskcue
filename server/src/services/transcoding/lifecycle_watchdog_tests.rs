// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::tests::{child, directory, shutdown};
use super::*;
use tokio::sync::Semaphore;

struct Fixture {
    workers: TranscodeWorkers,
    observation: WorkerObservation,
    semaphore: Arc<Semaphore>,
    id: Uuid,
    path: PathBuf,
}

async fn fixture(code: &str) -> anyhow::Result<Fixture> {
    let path = directory().await?;
    let workers = TranscodeWorkers::default();
    let semaphore = Arc::new(Semaphore::new(1));
    let id = Uuid::now_v7();
    workers.start_with_watchdog(
        WorkerLaunch {
            session_id: id,
            handle: child(code)?,
            permit: Arc::clone(&semaphore).acquire_owned().await?,
            directory: path.clone(),
            shutdown: shutdown(),
        },
        |_| Next::Continue,
        || {},
        Duration::from_millis(750),
    );
    let observation = workers.observation(id).unwrap();
    Ok(Fixture {
        workers,
        observation,
        semaphore,
        id,
        path,
    })
}

async fn terminal(observation: &WorkerObservation) -> anyhow::Result<ExecutionState> {
    Ok(tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = observation.snapshot().state;
            if state != ExecutionState::Running {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?)
}

async fn clean(fixture: &Fixture) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), fixture.workers.stop(fixture.id)).await??;
    assert_eq!(fixture.semaphore.available_permits(), 1);
    assert!(!fixture.path.exists());
    tokio::time::timeout(Duration::from_secs(1), async {
        while fixture.workers.observation(fixture.id).is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_media_time_and_log_spam_do_not_prevent_stall() -> anyhow::Result<()> {
    let fixture = fixture("setInterval(()=>{process.stdout.write('frame=42\\nout_time_us=1000000\\nprogress=continue\\n');process.stderr.write('still alive\\n');},20);setTimeout(()=>process.exit(0),15000);").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Stalled
    );
    assert_eq!(
        fixture.workers.failure(fixture.id),
        Some(FailureClass::Stalled)
    );
    clean(&fixture).await?;
    assert_eq!(
        fixture.observation.snapshot().state,
        ExecutionState::Stalled
    );
    assert_eq!(
        fixture.observation.snapshot().failure,
        Some(FailureClass::Stalled)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closed_stdout_with_live_child_is_stalled_and_physically_cleaned() -> anyhow::Result<()> {
    let fixture = fixture(
        "process.stdout.end();setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);",
    )
    .await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Stalled
    );
    clean(&fixture).await?;
    assert_eq!(
        fixture.observation.snapshot().failure,
        Some(FailureClass::Stalled)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_end_without_child_exit_still_expires() -> anyhow::Result<()> {
    let fixture = fixture("process.stdout.write('out_time_us=4000000\\nprogress=end\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Stalled
    );
    clean(&fixture).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successful_exit_is_terminal_even_with_failure_looking_stderr() -> anyhow::Result<()> {
    let fixture = fixture("process.stderr.write('Cannot load libcuda.so.1\\n');process.stdout.write('out_time_us=4000000\\nprogress=end\\n');").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Succeeded
    );
    assert_eq!(fixture.observation.snapshot().failure, None);
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        fixture.observation.snapshot().state,
        ExecutionState::Succeeded
    );
    clean(&fixture).await?;
    assert_eq!(
        fixture.observation.snapshot().state,
        ExecutionState::Succeeded
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_hardware_failure_survives_cleanup_for_safe_startup_retry() -> anyhow::Result<()> {
    let fixture =
        fixture("process.stderr.write('Cannot load libcuda.so.1\\n',()=>process.exit(2));").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Failed
    );
    assert_eq!(
        fixture.observation.snapshot().failure,
        Some(FailureClass::Hardware)
    );
    clean(&fixture).await?;
    assert_eq!(fixture.observation.snapshot().state, ExecutionState::Failed);
    assert_eq!(
        fixture.observation.snapshot().failure,
        Some(FailureClass::Hardware)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn security_denial_cannot_be_reclassified_as_hardware_recovery() -> anyhow::Result<()> {
    let fixture = fixture("process.stderr.write('Cannot load libcuda.so.1\\nPermission denied\\n',()=>process.exit(2));").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Failed
    );
    assert_eq!(
        fixture.observation.snapshot().failure,
        Some(FailureClass::Unknown)
    );
    clean(&fixture).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_of_running_child_dominates_failure_hints_and_stall() -> anyhow::Result<()> {
    let fixture = fixture("process.stderr.write('Cannot load libcuda.so.1\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);").await?;
    assert_eq!(
        fixture.observation.snapshot().state,
        ExecutionState::Running
    );
    clean(&fixture).await?;
    assert_eq!(
        fixture.observation.snapshot().state,
        ExecutionState::Cancelled
    );
    assert_eq!(fixture.observation.snapshot().failure, None);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn advancing_complete_records_extend_liveness_until_successful_exit() -> anyhow::Result<()> {
    let fixture = fixture("let time=0;setInterval(()=>process.stdout.write('out_time_us='+(time+=250000)+'\\nprogress=continue\\n'),200);setTimeout(()=>process.exit(0),1800);").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Succeeded
    );
    assert_eq!(fixture.observation.snapshot().failure, None);
    clean(&fixture).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_process_with_security_denial_has_no_recovery_class() -> anyhow::Result<()> {
    let fixture = fixture("process.stderr.write('Permission denied\\n');setInterval(()=>{},1000);setTimeout(()=>process.exit(0),15000);").await?;
    assert_eq!(
        terminal(&fixture.observation).await?,
        ExecutionState::Stalled
    );
    assert_eq!(
        fixture.observation.snapshot().failure,
        Some(FailureClass::Unknown)
    );
    clean(&fixture).await
}
