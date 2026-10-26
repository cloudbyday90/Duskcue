// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, Notify, RwLock},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

type Feeds = Arc<RwLock<(Arc<Vec<u8>>, Arc<Vec<u8>>)>>;

pub struct Server {
    pub address: SocketAddr,
    data: Feeds,
    requests: Arc<AtomicUsize>,
    guide_gate: Arc<Mutex<Option<Arc<Notify>>>>,
    stop: CancellationToken,
    task: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Server {
    pub async fn start(playlist: Vec<u8>, guide: Vec<u8>) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let data = Arc::new(RwLock::new((Arc::new(playlist), Arc::new(guide))));
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = CancellationToken::new();
        let guide_gate = Arc::new(Mutex::new(None));
        let gate = guide_gate.clone();
        let (payload, count, cancel) = (data.clone(), requests.clone(), stop.clone());
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            let mut accepted = 0;
            loop {
                tokio::select! {
                    biased;
                    _=cancel.cancelled()=>break,
                    result=tasks.join_next(),if !tasks.is_empty()=>{if let Some(result)=result {result??;}},
                    result=listener.accept(),if accepted<64=>{
                        let (socket,_)=result?;accepted+=1;
                        let (data,count,stop,gate)=(payload.clone(),count.clone(),cancel.clone(),gate.clone());
                        tasks.spawn(async move {
                            tokio::select! {_=stop.cancelled()=>Ok(()),result=serve(socket,data,count,gate)=>result}
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
            address,
            data,
            requests,
            guide_gate,
            stop,
            task: Some(task),
        })
    }
    pub fn origin(&self) -> String {
        format!("http://duskcue-feed.invalid:{}", self.address.port())
    }
    pub fn count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
    pub async fn hold_guide(&self) -> Arc<Notify> {
        let gate = Arc::new(Notify::new());
        *self.guide_gate.lock().await = Some(gate.clone());
        gate
    }
    pub async fn set(&self, playlist: Vec<u8>, guide: Vec<u8>) -> anyhow::Result<()> {
        anyhow::ensure!(
            playlist.len() <= 8 * 1024 * 1024 && guide.len() <= 64 * 1024 * 1024,
            "catalog feed fixture exceeded input bounds"
        );
        *self.data.write().await = (Arc::new(playlist), Arc::new(guide));
        Ok(())
    }
    pub async fn finish(mut self) -> anyhow::Result<()> {
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            tokio::time::timeout(Duration::from_secs(3), task).await???;
        }
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn serve(
    mut socket: TcpStream,
    data: Feeds,
    count: Arc<AtomicUsize>,
    gate: Arc<Mutex<Option<Arc<Notify>>>>,
) -> anyhow::Result<()> {
    let mut head = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !head.ends_with(b"\r\n\r\n") {
            anyhow::ensure!(
                head.len() < 16384,
                "catalog feed request head exceeded bound"
            );
            head.push(socket.read_u8().await?);
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    count.fetch_add(1, Ordering::SeqCst);
    let (playlist, guide) = data.read().await.clone();
    let body = if head.starts_with(b"GET /playlist?") {
        playlist
    } else if head.starts_with(b"GET /guide?") {
        if let Some(gate) = gate.lock().await.take() {
            gate.notified().await;
        }
        guide
    } else {
        anyhow::bail!("catalog feed requested unexpected resource")
    };
    socket
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    socket.write_all(&body).await?;
    socket.shutdown().await?;
    Ok(())
}
