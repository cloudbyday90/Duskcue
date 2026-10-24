// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::{Host, Url};

use super::TransportError;

pub struct Source {
    url: Url,
    destination: SocketAddr,
}

impl Source {
    pub fn new(input: &str, approved: SocketAddr) -> Result<Self, TransportError> {
        if approved.port() == 0
            || input
                .split_once("://")
                .and_then(|(_, value)| value.split(['/', '?', '#']).next())
                .is_some_and(|authority| authority.contains('@'))
            || input.len() > 4096
            || input
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        {
            return Err(TransportError::Destination);
        }
        let url = Url::parse(input).map_err(|_| TransportError::Destination)?;
        let address = match url.host() {
            Some(Host::Ipv4(ip)) => IpAddr::V4(ip),
            Some(Host::Ipv6(ip)) => IpAddr::V6(ip),
            _ => return Err(TransportError::Destination),
        };
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || canonical(address) != canonical(approved.ip())
            || url.port_or_known_default() != Some(approved.port())
            || forbidden(address)
        {
            return Err(TransportError::Destination);
        }
        Ok(Self {
            url,
            destination: approved,
        })
    }

    pub(super) async fn open(
        self,
        cancel: &CancellationToken,
    ) -> Result<reqwest::Response, TransportError> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .http1_only()
            .pool_max_idle_per_host(0)
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(5))
            .connection_verbose(false)
            .build()
            .map_err(|_| TransportError::Connection)?;
        let request = client
            .get(self.url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send();
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TransportError::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(5), request) => result.map_err(|_| TransportError::Deadline)?.map_err(|_| TransportError::Connection)?,
        };
        let peer = response.remote_addr().ok_or(TransportError::Connection)?;
        if canonical(peer.ip()) != canonical(self.destination.ip())
            || peer.port() != self.destination.port()
            || response.status() != reqwest::StatusCode::OK
        {
            return Err(TransportError::Response);
        }
        let kind = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(str::trim)
            .unwrap_or("");
        let encoding = response.headers().get(reqwest::header::CONTENT_ENCODING);
        if !(kind.eq_ignore_ascii_case("video/mp2t")
            || kind.eq_ignore_ascii_case("application/octet-stream"))
            || encoding.is_some_and(|v| v != "identity")
        {
            return Err(TransportError::Response);
        }
        Ok(response)
    }
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        _ => ip,
    }
}

fn forbidden(ip: IpAddr) -> bool {
    match canonical(ip) {
        IpAddr::V4(ip) => {
            ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_link_local()
                || ip == Ipv4Addr::new(100, 100, 100, 200)
                || ip == Ipv4Addr::new(168, 63, 129, 16)
        }
        IpAddr::V6(ip) => {
            ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unicast_link_local()
                || ip == "fd00:ec2::254".parse::<std::net::Ipv6Addr>().unwrap()
        }
    }
}
