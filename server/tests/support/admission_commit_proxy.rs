// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

pub struct Proxy {
    pub url: String,
    dropped: Arc<AtomicUsize>,
    enabled: Arc<AtomicBool>,
    cancel: CancellationToken,
    task: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Proxy {
    pub async fn start(
        raw: &str,
        needle: &'static [u8],
        before_commit: bool,
        begin_ack: bool,
    ) -> anyhow::Result<Self> {
        let mut url = url::Url::parse(raw)?;
        anyhow::ensure!(
            url.host_str() == Some("127.0.0.1"),
            "commit fault fixture requires owned loopback PostgreSQL"
        );
        let target = format!(
            "127.0.0.1:{}",
            url.port()
                .ok_or_else(|| anyhow::anyhow!("fixture port missing"))?
        );
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        url.set_port(Some(listener.local_addr()?.port()))
            .map_err(|_| anyhow::anyhow!("fixture port invalid"))?;
        url.query_pairs_mut().append_pair("sslmode", "disable");
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let dropped = Arc::new(AtomicUsize::new(0));
        let count = dropped.clone();
        let enabled = Arc::new(AtomicBool::new(true));
        let active = enabled.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            let mut admitted = 0;
            loop {
                tokio::select! {
                    biased;
                    _=token.cancelled()=>break,
                    result=tasks.join_next(),if !tasks.is_empty()=>{if let Some(result)=result {result??;}},
                    accepted=listener.accept(),if admitted<64=>{
                        let (client,_)=accepted?;admitted+=1;
                        let target=target.clone();let token=token.clone();let count=count.clone();let active=active.clone();
                        tasks.spawn(async move {
                            tokio::select! {
                                _=token.cancelled()=>Ok(()),
                                result=relay(client,&target,count,active,needle,before_commit,begin_ack)=>result,
                            }
                        });
                    }
                }
            }
            drop(listener);
            while let Some(result) = tasks.join_next().await {
                result??;
            }
            Ok(())
        });
        Ok(Self {
            url: url.into(),
            dropped,
            enabled,
            cancel,
            task: Some(task),
        })
    }
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::SeqCst)
    }
    pub fn resume(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }
    #[allow(dead_code)]
    pub fn arm(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }
    pub async fn finish(mut self) -> anyhow::Result<()> {
        self.resume();
        self.cancel.cancel();
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.task
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("fixture task missing"))?,
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

async fn relay(
    client: TcpStream,
    target: &str,
    dropped: Arc<AtomicUsize>,
    enabled: Arc<AtomicBool>,
    needle: &'static [u8],
    before_commit: bool,
    begin_ack: bool,
) -> anyhow::Result<()> {
    let server = TcpStream::connect(target).await?;
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let armed = AtomicBool::new(false);
    let upstream = async {
        let mut buffer = [0; 8192];
        let mut tail = Vec::new();
        loop {
            let n = match client_read.read(&mut buffer).await {
                Ok(n) => n,
                Err(_) => return Ok::<_, anyhow::Error>(()),
            };
            if n == 0 {
                return Ok(());
            }
            tail.extend_from_slice(&buffer[..n]);
            if tail.windows(needle.len()).any(|v| v == needle) {
                armed.store(true, Ordering::SeqCst);
            }
            if before_commit
                && enabled.load(Ordering::SeqCst)
                && armed.load(Ordering::SeqCst)
                && tail.windows(7).any(|v| v == b"COMMIT\0")
            {
                dropped.fetch_add(1, Ordering::SeqCst);
                return Ok(());
            }
            if tail.len() > 128 {
                tail.drain(..tail.len() - 128);
            }
            if server_write.write_all(&buffer[..n]).await.is_err() {
                return Ok(());
            }
        }
    };
    let downstream = async {
        loop {
            let mut head = [0; 5];
            if server_read.read_exact(&mut head).await.is_err() {
                return Ok::<_, anyhow::Error>(());
            }
            let length = u32::from_be_bytes(head[1..].try_into()?);
            anyhow::ensure!(
                (4..=1024 * 1024).contains(&length),
                "fixture protocol frame exceeded bound"
            );
            let mut payload = vec![0; (length - 4) as usize];
            server_read.read_exact(&mut payload).await?;
            if head[0] == b'C'
                && enabled.load(Ordering::SeqCst)
                && ((payload == b"COMMIT\0" && armed.load(Ordering::SeqCst))
                    || (begin_ack && payload == b"BEGIN\0"))
            {
                dropped.fetch_add(1, Ordering::SeqCst);
                return Ok(());
            }
            if client_write.write_all(&head).await.is_err()
                || client_write.write_all(&payload).await.is_err()
            {
                return Ok(());
            }
        }
    };
    tokio::select! {result=upstream=>result,result=downstream=>result}
}
