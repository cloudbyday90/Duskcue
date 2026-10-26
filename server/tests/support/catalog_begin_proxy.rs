// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

struct Gate {
    armed: AtomicBool,
    held: AtomicBool,
    pid: AtomicI32,
    wake: Notify,
    release: Notify,
}
pub struct Proxy {
    pub url: String,
    gate: Arc<Gate>,
    cancel: CancellationToken,
    task: Option<JoinHandle<anyhow::Result<()>>>,
}
impl Proxy {
    pub async fn start(raw: &str) -> anyhow::Result<Self> {
        let mut url = url::Url::parse(raw)?;
        anyhow::ensure!(
            url.host_str() == Some("127.0.0.1"),
            "BEGIN fixture requires owned loopback PostgreSQL"
        );
        let target = format!(
            "127.0.0.1:{}",
            url.port()
                .ok_or_else(|| anyhow::anyhow!("owned port missing"))?
        );
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        url.set_port(Some(listener.local_addr()?.port()))
            .map_err(|_| anyhow::anyhow!("owned port invalid"))?;
        url.query_pairs_mut().append_pair("sslmode", "disable");
        let gate = Arc::new(Gate {
            armed: AtomicBool::new(false),
            held: AtomicBool::new(false),
            pid: AtomicI32::new(0),
            wake: Notify::new(),
            release: Notify::new(),
        });
        let cancel = CancellationToken::new();
        let shared = gate.clone();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            let mut count = 0;
            loop {
                tokio::select! {biased;
                    _=token.cancelled()=>break,
                    joined=tasks.join_next(),if !tasks.is_empty()=>{if let Some(joined)=joined {joined??;}},
                    client=listener.accept(),if count<32=>{let (client,_)=client?;count+=1;let target=target.clone();let shared=shared.clone();let token=token.clone();tasks.spawn(async move {tokio::select!{_ =token.cancelled()=>Ok(()),result=relay(client,&target,shared)=>result}});}
                }
            }
            drop(listener);
            while let Some(joined) = tasks.join_next().await {
                joined??;
            }
            Ok(())
        });
        Ok(Self {
            url: url.into(),
            gate,
            cancel,
            task: Some(task),
        })
    }
    pub fn arm(&self) {
        self.gate.held.store(false, Ordering::SeqCst);
        self.gate.armed.store(true, Ordering::SeqCst);
    }
    pub async fn held(&self) -> anyhow::Result<i32> {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let wake = self.gate.wake.notified();
                if self.gate.held.load(Ordering::SeqCst) {
                    break;
                }
                wake.await;
            }
        })
        .await?;
        Ok(self.gate.pid.load(Ordering::SeqCst))
    }
    pub fn release(&self) {
        self.gate.release.notify_one();
    }
    pub async fn finish(mut self) -> anyhow::Result<()> {
        self.cancel.cancel();
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.task
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("proxy task missing"))?,
        )
        .await???;
        self.task = None;
        Ok(())
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
async fn relay(client: TcpStream, target: &str, gate: Arc<Gate>) -> anyhow::Result<()> {
    let server = TcpStream::connect(target).await?;
    let (mut cr, mut cw) = client.into_split();
    let (mut sr, mut sw) = server.into_split();
    let upstream = async {
        let mut buffer = [0; 8192];
        loop {
            let n = match cr.read(&mut buffer).await {
                Ok(n) => n,
                Err(_) => return Ok::<_, anyhow::Error>(()),
            };
            if n == 0 {
                return Ok(());
            }
            if sw.write_all(&buffer[..n]).await.is_err() {
                return Ok(());
            }
            if buffer[..n].windows(5).any(|v| v == b"X\0\0\0\x04") {
                return Ok(());
            }
        }
    };
    let downstream = async {
        loop {
            let mut head = [0; 5];
            if sr.read_exact(&mut head).await.is_err() {
                return Ok::<_, anyhow::Error>(());
            }
            let length = u32::from_be_bytes(head[1..].try_into()?);
            anyhow::ensure!(
                (4..=1024 * 1024).contains(&length),
                "BEGIN protocol frame exceeded bound"
            );
            let mut payload = vec![0; (length - 4) as usize];
            if sr.read_exact(&mut payload).await.is_err() {
                return Ok(());
            }
            if head[0] == b'K' && payload.len() == 8 {
                gate.pid.store(
                    i32::from_be_bytes(payload[..4].try_into()?),
                    Ordering::SeqCst,
                );
            }
            if head[0] == b'C' && payload == b"BEGIN\0" && gate.armed.swap(false, Ordering::SeqCst)
            {
                gate.held.store(true, Ordering::SeqCst);
                gate.wake.notify_waiters();
                gate.release.notified().await;
            }
            if cw.write_all(&head).await.is_err() || cw.write_all(&payload).await.is_err() {
                return Ok(());
            }
        }
    };
    tokio::select! {result=upstream=>result,result=downstream=>result}
}
