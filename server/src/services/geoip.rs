// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! MaxMind GeoLite2 City MMDB reader and IP geolocation lookup.
//!
//! Implements the IP geolocation enrichment layer described in
//! [ANALYTICS_SECURITY.md](../../docs/security/ANALYTICS_SECURITY.md):
//!
//! - Opens the GeoLite2-City MMDB file from `{data_dir}/geoip/GeoLite2-City.mmdb`
//!   via [`maxminddb::Reader::open_readfile`], holding the reader behind an
//!   [`ArcSwap`] for lock-free hot-reload.
//! - Gracefully degrades when the MMDB is absent: the service initializes with
//!   `None` stored, [`GeoIpService::lookup`] returns `None`, and the rest of
//!   the server runs without geolocation enrichment.
//! - [`GeoIpService::reload`] atomically swaps in a freshly-downloaded MMDB
//!   (called by the `geoip_updater` worker in Phase 11 Task 9) without
//!   blocking concurrent lookups — in-flight lookups keep using the old reader
//!   until the swap completes.
//! - [`classify_location`] classifies an IP as LAN/WAN/Relay using RFC 1918,
//!   CGNAT (Tailscale/WireGuard mesh), link-local, and loopback ranges, plus
//!   any operator-configured server subnets.
//!
//! The `geoip2` record types are provided by the `maxminddb` crate itself
//! (`use maxminddb::geoip2`), not a separate dependency. The 0.28 API returns
//! a lightweight [`maxminddb::LookupResult`] handle from `lookup()`; decoded
//! records borrow the reader's internal buffer, so [`GeoIpService::lookup`]
//! extracts owned data into [`GeoLocation`] within the ArcSwap-guard scope and
//! returns the owned struct — the borrow is released before the function
//! returns.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use maxminddb::{geoip2, Reader};
use thiserror::Error;

/// Subdirectory under `data_dir` holding the MMDB file.
pub const GEOIP_SUBDIR: &str = "geoip";

/// Canonical GeoLite2-City database filename.
pub const GEOIP_DB_FILENAME: &str = "GeoLite2-City.mmdb";

/// Owned geolocation result extracted from a MaxMind City record.
///
/// All fields are owned (`String`/`f64`/`u16`) so the struct can be returned
/// from [`GeoIpService::lookup`] without lifetime coupling to the MMDB reader's
/// internal buffer. Every field is `Option` — MaxMind records are sparse; a
/// given IP may resolve to only a country without city-level detail.
///
/// `country_iso` is the ISO 3166-1 alpha-2 code (e.g., `"US"`, `"GB"`) and is
/// the field used for impossible-travel same-country suppression and
/// `user_location_history` tracking. `country_name` is the English display
/// name for the admin dashboard.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeoLocation {
    pub city: Option<String>,
    pub region: Option<String>,
    pub region_code: Option<String>,
    pub country_iso: Option<String>,
    pub country_name: Option<String>,
    pub continent_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub accuracy_radius_km: Option<u16>,
    pub time_zone: Option<String>,
}

/// How a client IP reaches the server — used by the impossible-travel
/// suppression engine and play-session enrichment.
///
/// Per ANALYTICS_SECURITY.md §Location Type Classification: LAN and VPN
/// connections suppress impossible-travel detection entirely (both sides of
/// the "travel" are on the same trusted network).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationType {
    Lan,
    Wan,
    Relay,
}

impl LocationType {
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            LocationType::Lan => "lan",
            LocationType::Wan => "wan",
            LocationType::Relay => "relay",
        }
    }
}

/// Operational status of the GeoIP database, surfaced by the
/// `GET /api/v1/analytics/geoip/status` admin endpoint.
#[derive(Debug, Clone)]
pub struct GeoIpStatus {
    pub loaded: bool,
    pub path: PathBuf,
    pub present_on_disk: bool,
    pub size_bytes: Option<u64>,
    pub age_days: Option<i64>,
}

/// Errors that can occur while loading or reloading the MMDB.
#[derive(Debug, Error)]
pub enum GeoIpError {
    #[error("failed to open MMDB at {}: {source}", .path.display())]
    OpenFailed {
        path: PathBuf,
        #[source]
        source: maxminddb::MaxMindDbError,
    },
}

/// In-memory GeoLite2-City reader with atomic hot-reload.
///
/// The reader is stored as `ArcSwap<Option<Reader<Vec<u8>>>>`:
/// - `Some(reader)` — MMDB loaded; lookups proceed.
/// - `None` — MMDB absent or corrupt at startup; lookups return `None`
///   (graceful degradation).
///
/// `ArcSwap` provides lock-free reads via `load_full()` (returns an owned
/// `Arc`, valid for the caller's scope). A `reload()` atomically swaps in a new
/// reader; concurrent lookups keep using the previous `Arc` until it drops.
pub struct GeoIpService {
    reader: ArcSwap<Option<Reader<Vec<u8>>>>,
    db_path: PathBuf,
}

impl GeoIpService {
    /// Construct the service, attempting to open the MMDB at
    /// `{data_dir}/geoip/GeoLite2-City.mmdb`.
    ///
    /// If the file is missing or unreadable, the service starts in degraded
    /// mode (`None` reader) — the server runs normally without geolocation.
    /// The startup caller logs the outcome so the operator knows whether
    /// GeoIP is active.
    #[must_use]
    pub fn new(data_dir: &std::path::Path) -> Self {
        let db_path = data_dir.join(GEOIP_SUBDIR).join(GEOIP_DB_FILENAME);
        let reader = match Reader::open_readfile(&db_path) {
            Ok(r) => {
                tracing::info!(
                    path = %db_path.display(),
                    "GeoIP database loaded — geolocation enrichment enabled"
                );
                Some(r)
            }
            Err(e) => {
                tracing::warn!(
                    path = %db_path.display(),
                    error = %e,
                    "GeoIP database not available — geolocation enrichment disabled \
                     (place GeoLite2-City.mmdb here or configure the updater)"
                );
                None
            }
        };
        Self {
            reader: ArcSwap::new(Arc::new(reader)),
            db_path,
        }
    }

    /// Construct a service with no database loaded (for tests / default
    /// `AppState::new()` where no real data dir exists).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            reader: ArcSwap::new(Arc::new(None)),
            db_path: PathBuf::new(),
        }
    }

    /// Look up the approximate geographic location of a public IP address.
    ///
    /// Returns `None` when:
    /// - The MMDB is not loaded (degraded mode).
    /// - The IP is not in the database.
    /// - The record decodes but has no useful fields.
    ///
    /// Private/loopback IPs still return `None` — callers should check
    /// [`classify_location`] first and skip enrichment for LAN connections.
    #[must_use]
    pub fn lookup(&self, ip: IpAddr) -> Option<GeoLocation> {
        let arc = self.reader.load_full();
        let reader = arc.as_ref().as_ref()?;
        let result = reader.lookup(ip).ok()?;
        if !result.has_data() {
            return None;
        }
        let city = result.decode::<geoip2::City>().ok()??;
        let loc = &city.location;
        let region = city.subdivisions.first();
        Some(GeoLocation {
            city: city.city.names.english.map(String::from),
            region: region.and_then(|s| s.names.english).map(String::from),
            region_code: region.and_then(|s| s.iso_code).map(String::from),
            country_iso: city.country.iso_code.map(String::from),
            country_name: city.country.names.english.map(String::from),
            continent_code: city.continent.code.map(String::from),
            latitude: loc.latitude,
            longitude: loc.longitude,
            accuracy_radius_km: loc.accuracy_radius,
            time_zone: loc.time_zone.map(String::from),
        })
    }

    /// Whether a usable MMDB reader is currently loaded.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.reader.load_full().as_ref().is_some()
    }

    /// Atomically swap in a freshly-downloaded MMDB from `db_path`.
    ///
    /// Called by the `geoip_updater` scheduled task (Phase 11 Task 9) after a
    /// successful download. If the open fails, the existing reader is left
    /// untouched — a failed update does not take the service offline.
    ///
    /// The updater writes the new file to a temp path, validates it, then
    /// atomically renames over `db_path` before calling this method.
    pub fn reload(&self) -> Result<(), GeoIpError> {
        let new_reader =
            Reader::open_readfile(&self.db_path).map_err(|source| GeoIpError::OpenFailed {
                path: self.db_path.clone(),
                source,
            })?;
        self.reader.store(Arc::new(Some(new_reader)));
        tracing::info!(
            path = %self.db_path.display(),
            "GeoIP database reloaded"
        );
        Ok(())
    }

    /// The on-disk path where the MMDB is expected to live.
    #[must_use]
    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    /// Current database status for the admin `geoip/status` endpoint.
    ///
    /// `loaded` reflects whether the in-memory reader is active; the disk
    /// fields (`present_on_disk`, `size_bytes`, `age_days`) are read from the
    /// filesystem so they report the truth even if a reload hasn't run yet
    /// after a manual file replacement.
    #[must_use]
    pub fn status(&self) -> GeoIpStatus {
        let loaded = self.is_available();
        let present_on_disk = self.db_path.exists();
        let meta = std::fs::metadata(&self.db_path).ok();
        let size_bytes = meta.as_ref().map(|m| m.len());
        let age_days = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|modified| {
                let modified_secs = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .ok()?;
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .ok()?;
                Some((now_secs.saturating_sub(modified_secs) / 86_400) as i64)
            });
        GeoIpStatus {
            loaded,
            path: self.db_path.clone(),
            present_on_disk,
            size_bytes,
            age_days,
        }
    }
}

/// Classify how a client IP reaches the server.
///
/// - [`LocationType::Lan`] — private range (RFC 1918, CGNAT/Tailscale mesh,
///   loopback, link-local) or matches one of the operator-configured server
///   subnets. LAN/VPN connections suppress impossible-travel detection.
/// - [`LocationType::Relay`] — reserved for future known-relay/proxy IP list
///   matching; currently always falls through to `Wan`.
/// - [`LocationType::Wan`] — any other public IP.
///
/// `server_subnets` is the list of CIDRs the server itself listens on (so
/// traffic from the local subnet is classified as LAN even if the source IP
/// is technically a public address on a point-to-point link).
#[must_use]
pub fn classify_location(ip: &IpAddr, server_subnets: &[ipnet::IpNet]) -> LocationType {
    if is_private_ip(ip) || server_subnets.iter().any(|s| s.contains(ip)) {
        LocationType::Lan
    } else {
        LocationType::Wan
    }
}

/// Whether an IP is in a private/reserved range that should never trigger
/// geolocation or impossible-travel logic.
///
/// Covers: RFC 1918 (`10/8`, `172.16/12`, `192.168/16`), loopback
/// (`127/8`, `::1`), link-local (`169.254/16`, `fe80::/10`), unspecified
/// (`0.0.0.0`, `::`), IPv6 ULA (`fc00::/7`), and CGNAT (`100.64/10` —
/// Tailscale/WireGuard mesh).
#[must_use]
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || is_cgnat(v4)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_unique_local_v6(v6)
                || is_link_local_v6(v6)
        }
    }
}

/// Carrier-grade NAT range `100.64.0.0/10` (RFC 6598) — used by Tailscale
/// and some WireGuard meshes.
fn is_cgnat(v4: &Ipv4Addr) -> bool {
    let o = v4.octets();
    o[0] == 100 && (o[1] & 0xC0) == 0x40
}

/// IPv6 Unique Local Address `fc00::/7`.
fn is_unique_local_v6(v6: &Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xFE00) == 0xFC00
}

/// IPv6 Link-Local `fe80::/10`.
fn is_link_local_v6(v6: &Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xFFC0) == 0xFE80
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn location_type_db_str() {
        assert_eq!(LocationType::Lan.as_db_str(), "lan");
        assert_eq!(LocationType::Wan.as_db_str(), "wan");
        assert_eq!(LocationType::Relay.as_db_str(), "relay");
    }

    #[test]
    fn disabled_service_returns_none() {
        let svc = GeoIpService::disabled();
        assert!(!svc.is_available());
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(svc.lookup(ip).is_none());
    }

    #[test]
    fn disabled_service_status_reports_not_loaded() {
        let svc = GeoIpService::disabled();
        let status = svc.status();
        assert!(!status.loaded);
        assert!(!status.present_on_disk);
        assert!(status.size_bytes.is_none());
        assert!(status.age_days.is_none());
    }

    #[test]
    fn new_from_missing_dir_degrades_gracefully() {
        let tmp = std::env::temp_dir().join("duskcue_geoip_test_missing");
        let svc = GeoIpService::new(&tmp);
        assert!(!svc.is_available());
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(svc.lookup(ip).is_none());
    }

    #[test]
    fn reload_missing_file_returns_error_keeps_state() {
        let svc = GeoIpService::disabled();
        let err = svc.reload().unwrap_err();
        assert!(matches!(err, GeoIpError::OpenFailed { .. }));
        assert!(!svc.is_available());
    }

    #[test]
    fn classify_private_ipv4_is_lan() {
        let subnets = vec![];
        for addr in ["10.0.0.1", "172.16.5.10", "192.168.1.1", "127.0.0.1"] {
            let ip: IpAddr = addr.parse().unwrap();
            assert_eq!(
                classify_location(&ip, &subnets),
                LocationType::Lan,
                "{addr} should be LAN"
            );
        }
    }

    #[test]
    fn classify_cgnat_is_lan() {
        let subnets = vec![];
        for addr in ["100.64.0.1", "100.100.50.20", "100.127.255.254"] {
            let ip: IpAddr = addr.parse().unwrap();
            assert_eq!(
                classify_location(&ip, &subnets),
                LocationType::Lan,
                "{addr} (CGNAT/Tailscale) should be LAN"
            );
        }
    }

    #[test]
    fn classify_link_local_is_lan() {
        let subnets = vec![];
        let ip: IpAddr = "169.254.1.1".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Lan);
    }

    #[test]
    fn classify_public_ipv4_is_wan() {
        let subnets = vec![];
        for addr in ["8.8.8.8", "1.1.1.1", "203.0.113.42"] {
            let ip: IpAddr = addr.parse().unwrap();
            assert_eq!(
                classify_location(&ip, &subnets),
                LocationType::Wan,
                "{addr} should be WAN"
            );
        }
    }

    #[test]
    fn classify_just_below_cgnat_is_wan() {
        let subnets = vec![];
        let ip: IpAddr = "100.63.255.255".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Wan);
    }

    #[test]
    fn classify_just_above_cgnat_is_wan() {
        let subnets = vec![];
        let ip: IpAddr = "100.128.0.1".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Wan);
    }

    #[test]
    fn classify_private_ipv6_is_lan() {
        let subnets = vec![];
        for addr in [
            "::1",
            "fc00::1",
            "fd12:3456:789a::1",
            "fe80::1",
            "febf::ffff",
        ] {
            let ip: IpAddr = addr.parse().unwrap();
            assert_eq!(
                classify_location(&ip, &subnets),
                LocationType::Lan,
                "{addr} should be LAN"
            );
        }
    }

    #[test]
    fn classify_public_ipv6_is_wan() {
        let subnets = vec![];
        let ip: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Wan);
    }

    #[test]
    fn classify_fec0_site_local_is_wan() {
        let subnets = vec![];
        let ip: IpAddr = "fec0::1".parse().unwrap();
        assert_eq!(
            classify_location(&ip, &subnets),
            LocationType::Wan,
            "fec0::/10 (deprecated site-local) is not ULA/link-local"
        );
    }

    #[test]
    fn classify_server_subnet_matches_as_lan() {
        let subnets = vec![ipnet::IpNet::from_str("203.0.113.0/24").unwrap()];
        let ip: IpAddr = "203.0.113.50".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Lan);
    }

    #[test]
    fn classify_server_subnet_non_match_is_wan() {
        let subnets = vec![ipnet::IpNet::from_str("203.0.113.0/24").unwrap()];
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Wan);
    }

    #[test]
    fn classify_server_subnet_ipv6_match() {
        let subnets = vec![ipnet::IpNet::from_str("2001:db8::/32").unwrap()];
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        assert_eq!(classify_location(&ip, &subnets), LocationType::Lan);
    }

    #[test]
    fn is_private_ip_edge_cases() {
        assert!(is_private_ip(&"0.0.0.0".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"::".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip(&"100.63.255.255".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip(&"100.128.0.0".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn geo_location_default_is_all_none() {
        let loc = GeoLocation::default();
        assert!(loc.city.is_none());
        assert!(loc.country_iso.is_none());
        assert!(loc.latitude.is_none());
    }

    #[test]
    fn db_path_uses_canonical_layout() {
        let svc = GeoIpService::new(std::path::Path::new("/data"));
        assert!(svc
            .db_path()
            .ends_with("geoip/GeoLite2-City.mmdb"));
    }
}
