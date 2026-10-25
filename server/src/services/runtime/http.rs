// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::ShutdownError;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub struct HttpServer {
    stop: CancellationToken,
    task: Option<JoinHandle<Result<(), std::io::Error>>>,
}
impl HttpServer {
    pub fn start(listener: TcpListener, app: axum::Router, stop: CancellationToken) -> Self {
        let completion = stop.clone().drop_guard();
        let signal = stop.clone();
        let task = tokio::spawn(async move {
            let _completion = completion;
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(signal.cancelled_owned())
            .await
        });
        Self {
            stop,
            task: Some(task),
        }
    }
    pub fn close(&self) {
        self.stop.cancel();
    }
    pub fn finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub async fn wait(&mut self) -> Result<(), ShutdownError> {
        if let Some(task) = self.task.as_mut() {
            let outcome = task.await;
            self.task = None;
            outcome
                .map_err(|_| ShutdownError)?
                .map_err(|_| ShutdownError)?;
        }
        Ok(())
    }
}
impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unexpected_http_task_exit_notifies_root_and_fails_join() {
        let stop = CancellationToken::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut http = HttpServer::start(listener, axum::Router::new(), stop.clone());
        http.task.as_ref().unwrap().abort();
        tokio::time::timeout(std::time::Duration::from_secs(1), stop.cancelled())
            .await
            .unwrap();
        assert!(http.wait().await.is_err());
    }
}
