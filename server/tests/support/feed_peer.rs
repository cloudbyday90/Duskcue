// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

pub enum Security {
    Plain,
    Tls,
    Expired,
    StalledTls,
}
pub enum Behavior {
    Reply(Vec<u8>),
    #[allow(dead_code)]
    GatedReply(Vec<u8>, Arc<tokio::sync::Notify>),
    Close(Vec<u8>),
    HeadersStall,
    BodyStall,
    Drip,
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

struct Observed {
    phase: AtomicUsize,
    closed: AtomicBool,
    request: Mutex<String>,
    sni: Mutex<Option<String>>,
}

pub struct Peer {
    pub address: SocketAddr,
    observed: Arc<Observed>,
    cancel: CancellationToken,
    task: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Peer {
    pub async fn new(ip: IpAddr, security: Security, behavior: Behavior) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::new(ip, 0)).await?;
        let address = listener.local_addr()?;
        let observed = Arc::new(Observed {
            phase: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            request: Mutex::new(String::new()),
            sni: Mutex::new(None),
        });
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let state = observed.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = task_cancel.cancelled() => Ok(()),
                result = serve(listener, security, behavior, &state) => result,
            }
        });
        Ok(Self {
            address,
            observed,
            cancel,
            task: Some(task),
        })
    }

    pub fn origin(&self, scheme: &str, host: &str) -> String {
        format!("{scheme}://{host}:{}", self.address.port())
    }

    pub fn hits(&self) -> usize {
        usize::from(self.observed.phase.load(Ordering::SeqCst) > 0)
    }

    pub async fn wait_phase(&self, phase: usize) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            while self.observed.phase.load(Ordering::SeqCst) < phase {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("fixture peer did not reach expected phase"))
    }

    pub fn check_request(&self, host: &str, tls: bool) -> anyhow::Result<()> {
        let request = self
            .observed
            .request
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        anyhow::ensure!(
            request.starts_with("GET /private-feed?credential=feed-query-private HTTP/1.1\r\n"),
            "saved request target changed"
        );
        let lowercase = request.to_ascii_lowercase();
        anyhow::ensure!(
            lowercase.contains(&format!("\r\nhost: {host}:{}\r\n", self.address.port()))
                && lowercase.contains("\r\naccept-encoding: identity\r\n")
                && !lowercase.contains("authorization:")
                && !lowercase.contains("cookie:")
                && !lowercase.contains("referer:"),
            "request authority or credential boundary changed"
        );
        if tls {
            anyhow::ensure!(
                self.observed
                    .sni
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_deref()
                    == Some(host),
                "TLS SNI did not retain saved hostname"
            );
        }
        Ok(())
    }

    pub async fn finish(mut self, require_closed: bool) -> anyhow::Result<()> {
        let closure = if require_closed {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !self.observed.closed.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| anyhow::anyhow!("peer did not observe upstream closure"))
        } else {
            Ok(())
        };
        self.cancel.cancel();
        let joined = tokio::time::timeout(
            Duration::from_secs(3),
            self.task
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("peer task absent"))?,
        )
        .await;
        self.task = None;
        joined.map_err(|_| anyhow::anyhow!("fixture peer cleanup deadline"))???;
        closure
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn acceptor(expired: bool) -> anyhow::Result<TlsAcceptor> {
    let cert = if expired {
        include_bytes!("../fixtures/iptv-feed/expired.der").as_slice()
    } else {
        include_bytes!("../fixtures/iptv-feed/leaf.der").as_slice()
    };
    let key = if expired {
        include_bytes!("../fixtures/iptv-feed/expired-key.der").as_slice()
    } else {
        include_bytes!("../fixtures/iptv-feed/leaf-key.der").as_slice()
    };
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(cert.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())),
            )?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

async fn closed(stream: &mut dyn Io, state: &Observed) -> anyhow::Result<()> {
    let mut buffer = [0; 1024];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                break;
            }
            Err(_) => anyhow::bail!("fixture peer read failed before closure"),
        }
    }
    state.closed.store(true, Ordering::SeqCst);
    Ok(())
}

async fn serve(
    listener: TcpListener,
    security: Security,
    behavior: Behavior,
    state: &Observed,
) -> anyhow::Result<()> {
    let (mut stream, _) = listener.accept().await?;
    state.phase.store(1, Ordering::SeqCst);
    if matches!(security, Security::StalledTls) {
        let mut hello = [0; 4096];
        anyhow::ensure!(stream.read(&mut hello).await? > 0, "TLS ClientHello absent");
        state.phase.store(2, Ordering::SeqCst);
        return closed(&mut stream, state).await;
    }
    let mut stream: Box<dyn Io> = match security {
        Security::Plain => Box::new(stream),
        Security::Tls | Security::Expired => {
            match acceptor(matches!(security, Security::Expired))?
                .accept(stream)
                .await
            {
                Ok(stream) => {
                    *state.sni.lock().unwrap_or_else(|e| e.into_inner()) =
                        stream.get_ref().1.server_name().map(ToString::to_string);
                    Box::new(stream)
                }
                Err(_) => {
                    state.closed.store(true, Ordering::SeqCst);
                    return Ok(());
                }
            }
        }
        Security::StalledTls => unreachable!(),
    };
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        match stream.read(&mut byte).await {
            Ok(0) | Err(_) => {
                state.closed.store(true, Ordering::SeqCst);
                return Ok(());
            }
            Ok(_) => request.push(byte[0]),
        }
        anyhow::ensure!(request.len() <= 16384, "request fixture budget exceeded");
    }
    *state.request.lock().unwrap_or_else(|e| e.into_inner()) = String::from_utf8(request)?;
    state.phase.store(3, Ordering::SeqCst);
    match behavior {
        Behavior::GatedReply(bytes, gate) => {
            gate.notified().await;
            stream.write_all(&bytes).await?;
            stream.flush().await?;
        }
        Behavior::Reply(bytes) => {
            stream.write_all(&bytes).await?;
            stream.flush().await?;
        }
        Behavior::Close(bytes) => {
            stream.write_all(&bytes).await?;
            stream.shutdown().await?;
            state.closed.store(true, Ordering::SeqCst);
            return Ok(());
        }
        Behavior::HeadersStall => {}
        Behavior::BodyStall => {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 65536\r\n\r\nx")
                .await?;
            stream.flush().await?;
        }
        Behavior::Drip => {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 65536\r\n\r\n")
                .await?;
            loop {
                if stream.write_all(b"x").await.is_err() {
                    state.closed.store(true, Ordering::SeqCst);
                    return Ok(());
                }
                tokio::select! {
                    result = closed(&mut *stream, state) => return result,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                }
            }
        }
    }
    state.phase.store(4, Ordering::SeqCst);
    closed(&mut *stream, state).await
}

pub fn response(body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Fixture-Private: feed-response-private\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
