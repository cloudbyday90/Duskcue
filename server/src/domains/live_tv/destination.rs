// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod addresses;
mod codec;

use super::assignment::error::AssignmentError;
use std::net::IpAddr;
use url::{Host, Url};

pub(super) struct Approval {
    pub origin: String,
    pub addresses: Vec<IpAddr>,
}

pub(super) fn source(raw: &str) -> Result<Url, AssignmentError> {
    super::service::validate_feed_url(raw).map_err(|_| AssignmentError::InvalidPolicy)?;
    let url = Url::parse(raw).map_err(|_| AssignmentError::InvalidPolicy)?;
    if url.port_or_known_default().is_none_or(|p| p == 0) {
        return Err(AssignmentError::InvalidPolicy);
    }
    Ok(url)
}

fn origin(raw: &str) -> Result<String, AssignmentError> {
    let url = source(raw)?;
    if url.path() != "/" || url.query().is_some() {
        return Err(AssignmentError::InvalidPolicy);
    }
    Ok(url.origin().ascii_serialization())
}

impl Approval {
    pub fn new(expected: &str, pins: &[String], url: &Url) -> Result<Self, AssignmentError> {
        let origin = origin(expected)?;
        if origin != url.origin().ascii_serialization() || !(1..=8).contains(&pins.len()) {
            return Err(AssignmentError::InvalidPolicy);
        }
        let literal = match url.host() {
            Some(Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
            Some(Host::Ipv6(ip)) => Some(IpAddr::V6(ip).to_canonical()),
            _ => None,
        };
        let mut addresses = Vec::with_capacity(pins.len());
        for raw in pins {
            let ip = raw
                .parse::<IpAddr>()
                .map_err(|_| AssignmentError::InvalidPolicy)?
                .to_canonical();
            if !addresses::allowed(ip)
                || literal.is_some_and(|host| host != ip)
                || addresses.contains(&ip)
            {
                return Err(AssignmentError::InvalidPolicy);
            }
            addresses.push(ip);
        }
        Ok(Self { origin, addresses })
    }
}

#[cfg(test)]
mod tests;
