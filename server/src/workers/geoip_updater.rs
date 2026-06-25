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

//! Background GeoLite2-City MMDB updater — the scheduled task that keeps the
//! geolocation database current.
//!
//! Implements the `geoip_database_update` scheduled task described in
//! [ANALYTICS_SECURITY.md](../../docs/security/ANALYTICS_SECURITY.md)
//! §GeoIP Database Updates:
//!
//! - **Task type**: `geoip_database_update`
//! - **Schedule**: Weekly, Monday 03:00 (cron `0 3 * * 1`)
//! - **What it does**: Downloads the latest GeoLite2-City `.tar.gz` from
//!   MaxMind, extracts the `.mmdb` file, validates it, and atomically replaces
//!   the existing database. The [`GeoIpService`](crate::services::geoip::GeoIpService)
//!   is then hot-reloaded via `reload()` without a server restart.
//! - **License key**: Read from [`BootstrapConfig::geoip_license_key`]
//!   (bootstrap config / `DUSKCUE_GEOIP_LICENSE_KEY` env var), not from the
//!   database — it is a secret needed before the DB is available.
//! - **Fallback**: If the download fails, the existing MMDB continues to work.
//!   The task logs a warning and retries next week.
//!
//! ## Download flow
//!
//! 1. Build the MaxMind download URL with the license key as a query parameter.
//! 2. Download the `.tar.gz` archive (MaxMind redirects to a Cloudflare R2
//!    presigned URL; `reqwest` follows redirects automatically).
//! 3. Decompress (gzip via `flate2`) and extract (tar via `tar::Archive`) the
//!    `.mmdb` file from the archive — it is nested in a dated directory
//!    (e.g., `GeoLite2-City_20260617/GeoLite2-City.mmdb`).
//! 4. Write the extracted bytes to a `.tmp` file in the geoip directory.
//! 5. Validate by opening with `maxminddb::Reader::open_readfile` — if the
//!    file is not a valid MMDB, delete the temp file and abort.
//! 6. Atomically rename the `.tmp` file over the target path (on Unix this is
//!    an atomic overwrite; on Windows the destination is removed first).
//! 7. Call [`GeoIpService::reload`](crate::services::geoip::GeoIpService::reload)
//!    to swap the in-memory reader — concurrent lookups keep using the old
//!    `Arc` until the swap completes.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use flate2::read::GzDecoder;
use maxminddb::Reader;
use thiserror::Error;
use uuid::Uuid;

use crate::services::geoip::GEOIP_DB_FILENAME;
use crate::state::AppState;

const MAXMIND_DOWNLOAD_BASE: &str = "https://download.maxmind.com/app/geoip_download";
const EDITION_ID: &str = "GeoLite2-City";
const DOWNLOAD_TIMEOUT_SECS: u64 = 300;
const TMP_SUFFIX: &str = ".tmp";

#[derive(Debug, Error)]
enum UpdateError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("MaxMind returned HTTP {status} — check license key validity")]
    HttpStatus { status: reqwest::StatusCode },
    #[error("failed to read download body: {0}")]
    Body(String),
    #[error("failed to decompress or extract tar.gz: {0}")]
    Extract(String),
    #[error("no .mmdb file found inside the downloaded archive")]
    NoMmdb,
    #[error("failed to write temp file at {path}: {source}")]
    WriteTemp {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("downloaded file failed MMDB validation at {path}: {source}")]
    Validation {
        path: String,
        #[source]
        source: maxminddb::MaxMindDbError,
    },
    #[error("failed to replace database file: {0}")]
    Replace(#[from] std::io::Error),
}

pub async fn run_geoip_update(state: &AppState, task_id: Uuid, _config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting GeoIP database update task");

    let license_key = match state.bootstrap.geoip_license_key.as_deref() {
        Some(k) if !k.is_empty() => k,
        _ => {
            tracing::info!(
                task_id = %task_id,
                "No GeoIP license key configured — skipping database update \
                 (set DUSKCUE_GEOIP_LICENSE_KEY or geoip_license_key in config.toml to enable)"
            );
            return;
        }
    };

    let db_path = state.geoip.db_path().to_path_buf();

    match download_and_replace(license_key, &db_path).await {
        Ok(()) => match state.geoip.reload() {
            Ok(()) => {
                tracing::info!(
                    task_id = %task_id,
                    path = %db_path.display(),
                    "GeoIP database downloaded and reloaded successfully"
                );
            }
            Err(e) => {
                tracing::error!(
                    task_id = %task_id,
                    error = %e,
                    "GeoIP file replaced on disk but in-memory reload failed — \
                     the new database will be loaded on next server restart"
                );
            }
        },
        Err(e) => {
            tracing::warn!(
                task_id = %task_id,
                error = %e,
                "GeoIP database download failed — existing database (if any) continues to work; \
                 will retry next week"
            );
        }
    }

    tracing::info!(task_id = %task_id, "GeoIP database update task completed");
}

async fn download_and_replace(license_key: &str, db_path: &Path) -> Result<(), UpdateError> {
    let url = format!(
        "{MAXMIND_DOWNLOAD_BASE}?edition_id={EDITION_ID}&license_key={}&suffix=tar.gz",
        urlencoding::encode(license_key)
    );

    tracing::info!("Downloading GeoLite2-City database from MaxMind");

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()?;

    let response = http.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(UpdateError::HttpStatus {
            status: response.status(),
        });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpdateError::Body(e.to_string()))?;

    tracing::info!(
        bytes = bytes.len(),
        "Downloaded GeoLite2-City tar.gz archive, extracting MMDB"
    );

    let mmdb_bytes = extract_mmdb(&bytes)?;

    let geoip_dir = db_path.parent().ok_or_else(|| UpdateError::WriteTemp {
        path: db_path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir"),
    })?;

    tokio::fs::create_dir_all(geoip_dir)
        .await
        .map_err(|e| UpdateError::WriteTemp {
            path: geoip_dir.display().to_string(),
            source: e,
        })?;

    let tmp_path = geoip_dir.join(format!("{GEOIP_DB_FILENAME}{TMP_SUFFIX}"));

    tokio::fs::write(&tmp_path, &mmdb_bytes)
        .await
        .map_err(|e| UpdateError::WriteTemp {
            path: tmp_path.display().to_string(),
            source: e,
        })?;

    if let Err(e) = validate_mmdb(&tmp_path) {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e);
    }

    atomic_replace(&tmp_path, db_path)?;

    tracing::info!("GeoIP database file replaced atomically");
    Ok(())
}

fn extract_mmdb(tar_gz_bytes: &[u8]) -> Result<Vec<u8>, UpdateError> {
    let gz = GzDecoder::new(tar_gz_bytes);
    let mut archive = tar::Archive::new(gz);

    for entry in archive
        .entries()
        .map_err(|e| UpdateError::Extract(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| UpdateError::Extract(e.to_string()))?;
        let path_str = entry
            .path()
            .map_err(|e| UpdateError::Extract(e.to_string()))?
            .to_string_lossy()
            .into_owned();

        if path_str.ends_with(".mmdb") {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| UpdateError::Extract(e.to_string()))?;
            tracing::info!(
                size = buf.len(),
                entry = %path_str,
                "Extracted MMDB from tar.gz archive"
            );
            return Ok(buf);
        }
    }

    Err(UpdateError::NoMmdb)
}

fn validate_mmdb(path: &Path) -> Result<(), UpdateError> {
    Reader::<Vec<u8>>::open_readfile(path)
        .map(|_| ())
        .map_err(|source| UpdateError::Validation {
            path: path.display().to_string(),
            source,
        })
}

fn atomic_replace(tmp: &Path, dest: &Path) -> Result<(), UpdateError> {
    #[cfg(unix)]
    {
        std::fs::rename(tmp, dest)?;
    }
    #[cfg(not(unix))]
    {
        if dest.exists() {
            let _ = std::fs::remove_file(dest);
        }
        std::fs::rename(tmp, dest)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extract_mmdb_finds_the_mmdb_in_a_nested_targz() {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut dir_header = tar::Header::new_gnu();
            dir_header.set_path("GeoLite2-City_20260617/").unwrap();
            dir_header.set_entry_type(tar::EntryType::Directory);
            dir_header.set_size(0);
            dir_header.set_mode(0o755);
            dir_header.set_cksum();
            builder.append(&dir_header, std::io::empty()).unwrap();

            let mmdb_content = b"fake mmdb payload for testing";
            let mut file_header = tar::Header::new_gnu();
            file_header
                .set_path("GeoLite2-City_20260617/GeoLite2-City.mmdb")
                .unwrap();
            file_header.set_size(mmdb_content.len() as u64);
            file_header.set_mode(0o644);
            file_header.set_cksum();
            builder.append(&file_header, &mmdb_content[..]).unwrap();

            let mut readme_header = tar::Header::new_gnu();
            file_header
                .set_path("GeoLite2-City_20260617/README.txt")
                .unwrap();
            readme_header
                .set_path("GeoLite2-City_20260617/README.txt")
                .unwrap();
            readme_header.set_size(5);
            readme_header.set_mode(0o644);
            readme_header.set_cksum();
            builder.append(&readme_header, &b"hello"[..]).unwrap();

            builder.finish().unwrap();
        }

        let mut gz_bytes = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gz_bytes, flate2::Compression::default());
            encoder.write_all(&tar_bytes).unwrap();
            encoder.finish().unwrap();
        }

        let result = extract_mmdb(&gz_bytes).unwrap();
        assert_eq!(result, b"fake mmdb payload for testing");
    }

    #[test]
    fn extract_mmdb_returns_error_when_no_mmdb_present() {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_gnu();
            header.set_path("readme.txt").unwrap();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &b"hello"[..]).unwrap();
            builder.finish().unwrap();
        }

        let mut gz_bytes = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gz_bytes, flate2::Compression::default());
            encoder.write_all(&tar_bytes).unwrap();
            encoder.finish().unwrap();
        }

        let err = extract_mmdb(&gz_bytes).unwrap_err();
        assert!(matches!(err, UpdateError::NoMmdb));
    }

    #[test]
    fn extract_mmdb_returns_error_on_garbage_input() {
        let err = extract_mmdb(b"not a gzip file at all").unwrap_err();
        assert!(matches!(err, UpdateError::Extract(_)));
    }

    #[test]
    fn validate_mmdb_rejects_non_mmdb_file() {
        let tmp = std::env::temp_dir().join("duskcue_geoip_validate_test.bin");
        std::fs::write(&tmp, b"this is definitely not an mmdb file").unwrap();

        let result = validate_mmdb(&tmp);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UpdateError::Validation { .. }
        ));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn atomic_replace_overwrites_destination() {
        let dir = std::env::temp_dir().join("duskcue_geoip_atomic_test");
        std::fs::create_dir_all(&dir).unwrap();

        let tmp = dir.join("new.mmdb.tmp");
        let dest = dir.join("new.mmdb");

        std::fs::write(&tmp, b"new content").unwrap();
        std::fs::write(&dest, b"old content").unwrap();

        atomic_replace(&tmp, &dest).unwrap();

        let result = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(result, "new content");
        assert!(!tmp.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_replace_works_when_destination_absent() {
        let dir = std::env::temp_dir().join("duskcue_geoip_atomic_absent_test");
        std::fs::create_dir_all(&dir).unwrap();

        let tmp = dir.join("fresh.mmdb.tmp");
        let dest = dir.join("fresh.mmdb");

        std::fs::write(&tmp, b"content").unwrap();

        atomic_replace(&tmp, &dest).unwrap();

        let result = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(result, "content");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn geoip_db_filename_constant_is_stable() {
        assert_eq!(GEOIP_DB_FILENAME, "GeoLite2-City.mmdb");
    }
}
