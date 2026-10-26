// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Capture(Arc<Mutex<(Vec<u8>, bool)>>);

impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut log = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if log.0.len() + bytes.len() <= 1024 * 1024 {
            log.0.extend_from_slice(bytes);
        } else {
            log.1 = true;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    pub fn check(&self, secrets: &[&str]) -> anyhow::Result<()> {
        let log = self.0.lock().unwrap_or_else(|e| e.into_inner());
        anyhow::ensure!(!log.1, "diagnostic capture budget exceeded");
        anyhow::ensure!(
            !String::from_utf8_lossy(&log.0).contains("private-fixture-secret"),
            "secret appeared in diagnostics"
        );
        anyhow::ensure!(!log.0.is_empty(), "diagnostic logging was not exercised");
        anyhow::ensure!(
            secrets
                .iter()
                .all(|secret| !String::from_utf8_lossy(&log.0).contains(secret)),
            "credential appeared in diagnostics"
        );
        Ok(())
    }
}

pub fn capture() -> anyhow::Result<Capture> {
    capture_with_filter("reqwest=trace,hyper_util=trace,hyper=trace,rustls=trace,duskcue=trace")
}

pub fn capture_with_filter(filter: &str) -> anyhow::Result<Capture> {
    let capture = Capture(Arc::new(Mutex::new((Vec::new(), false))));
    let writer = capture.clone();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(filter)
        .with_writer(move || writer.clone())
        .try_init()
        .map_err(|_| anyhow::anyhow!("diagnostic subscriber setup failed"))?;
    Ok(capture)
}
