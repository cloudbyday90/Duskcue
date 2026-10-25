// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct Signals {
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl Signals {
    pub fn listen(stop: CancellationToken) -> io::Result<Self> {
        #[cfg(unix)]
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        #[cfg(windows)]
        let mut console_break = tokio::signal::windows::ctrl_break()?;
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            loop {
                let terminate = async {
                    #[cfg(unix)]
                    {
                        terminate.recv().await;
                    }
                    #[cfg(windows)]
                    {
                        console_break.recv().await;
                    }
                    #[cfg(not(any(unix, windows)))]
                    std::future::pending::<()>().await;
                };
                tokio::select! {
                    _=token.cancelled()=>return,
                    result=tokio::signal::ctrl_c()=>{if result.is_err(){std::process::exit(1);}},
                    _=terminate=>{},
                }
                if stop.is_cancelled() {
                    tracing::error!(event = "forced_shutdown");
                    std::process::exit(1);
                }
                stop.cancel();
            }
        });
        Ok(Self {
            cancel,
            task: Some(task),
        })
    }
    pub async fn finish(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
