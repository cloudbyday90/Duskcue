// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{Case, Inventory, ProgressiveFixture};

impl Inventory {
    pub(in super::super) async fn prepare(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            super::enabled()
                && std::env::var("DUSKCUE_TEST_NATIVE_RECOVERY").as_deref() == Ok("live")
        );
        let bytes = read(Path::new("/qualification/format-corpus.json"), 1_048_576).await?;
        let manifest: Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(manifest["version"] == 1);
        let cases = manifest["cases"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("format inventory missing"))?;
        anyhow::ensure!(cases.len() == 14);
        tokio::fs::create_dir(&self.root).await?;
        sqlx::query("INSERT INTO libraries(id,name,slug,media_type,root_path) VALUES($1,'Format corpus',$2,'movies',$3)")
            .bind(self.library).bind(format!("formats-{}",self.library)).bind(self.root.to_string_lossy().as_ref()).execute(fixture.pool).await?;
        sqlx::query("INSERT INTO library_paths(library_id,path,is_default) VALUES($1,$2,true)")
            .bind(self.library)
            .bind(self.root.to_string_lossy().as_ref())
            .execute(fixture.pool)
            .await?;
        let mut names = HashSet::new();
        let mut identities = HashSet::new();
        for entry in cases {
            validate(entry)?;
            let name = entry["filename"].as_str().unwrap();
            anyhow::ensure!(
                names.insert(name.to_string())
                    && identities.insert(entry["id"].as_str().unwrap().to_string())
            );
            let source = Path::new("/qualification/format-sources").join(name);
            let bytes = read(&source, 8 * 1024 * 1024).await?;
            anyhow::ensure!(digest(&bytes) == entry["sha256"]);
            let path = self.root.join(name);
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true).mode(0o600);
            use tokio::io::AsyncWriteExt;
            let mut file = options.open(&path).await?;
            file.write_all(&bytes).await?;
            file.flush().await?;
            drop(file);
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).await?;
        }
        let scan =
            duskcue::workers::library_scanner::scan_library(fixture.pool, self.library, true, None)
                .await?;
        let ingested: Vec<(String, bool)> = sqlx::query_as("SELECT f.file_path,f.is_healthy FROM media_files f JOIN media_items i ON i.id=f.media_item_id WHERE i.library_id=$1 ORDER BY f.file_path LIMIT 15")
            .bind(self.library).fetch_all(fixture.pool).await?;
        let mut diagnostic = scan_diagnostic(&scan, &ingested);
        diagnostic["pass"] = json!("initial");
        eprintln!("DUSKCUE_FORMAT_SCAN={diagnostic}");
        anyhow::ensure!(
            scan.errors.is_empty()
                && scan.files_new == 14
                && scan.items_created == 14
                && ingested.len() == 14,
            "actual format scanner did not ingest the complete inventory: {diagnostic}"
        );
        self.verify_rescan(fixture).await?;
        for (index, entry) in cases.iter().enumerate() {
            let path = self.root.join(entry["filename"].as_str().unwrap());
            let row: (Uuid,Uuid,String,String,String,Value) = sqlx::query_as("SELECT f.media_item_id,f.id,f.container_format,f.video_codec,f.video_dynamic_range,f.additional_streams FROM media_files f JOIN media_items i ON i.id=f.media_item_id WHERE i.library_id=$1 AND f.file_path=$2 AND f.is_healthy=true")
                .bind(self.library).bind(path.to_string_lossy().as_ref()).fetch_one(fixture.pool).await?;
            let video = &row.5["video"];
            let audio = row.5["audio"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("format audio inventory missing"))?;
            anyhow::ensure!(
                entry["container"].as_str() == Some(row.2.as_str())
                    && entry["codec"].as_str() == Some(row.3.as_str())
                    && entry["dynamicRange"].as_str() == Some(row.4.as_str())
                    && video["index"] == 0
                    && video["bit_depth"] == entry["bitDepth"]
                    && video["width"] == 320
                    && video["height"] == 180
                    && video["frame_rate"] == 24.0
                    && video["pixel_format"]
                        == if entry["bitDepth"] == 10 {
                            "yuv420p10le"
                        } else {
                            "yuv420p"
                        }
                    && video["color_transfer"]
                        == match row.4.as_str() {
                            "hdr10" => "smpte2084",
                            "hlg" => "arib-std-b67",
                            _ => "bt709",
                        }
                    && video["color_primaries"] == if row.4 == "sdr" { "bt709" } else { "bt2020" }
                    && audio.len() == 2
                    && audio[0]["index"] == 1
                    && audio[0]["codec"] == "opus"
                    && audio[0]["is_default"] == false
                    && audio[1]["index"] == 2
                    && audio[1]["codec"] == "opus"
                    && audio[1]["is_default"] == true
                    && audio[1]["sample_rate"] == 48000
                    && row.5["probe"]["status"] == "probed",
                "scanner format metadata differs from the generated source"
            );
            sqlx::query("UPDATE media_items SET title=$2,sort_title=$2,content_rating='R',runtime_seconds=8 WHERE id=$1").bind(row.0).bind(format!("Format {}",entry["id"].as_str().unwrap())).execute(fixture.pool).await?;
            self.cases.push(Case {
                index,
                source: entry.clone(),
                path,
                item: row.0,
                file: row.1,
            });
        }
        sqlx::query("INSERT INTO users(id,username,display_name,has_all_library_access) VALUES($1,$2,'Format denied peer',false)").bind(self.peer).bind(format!("format-peer-{}",self.peer)).execute(fixture.pool).await?;
        let peer_profile = Uuid::now_v7();
        sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,is_default) VALUES($1,$2,'Format peer',true)").bind(peer_profile).bind(self.peer).execute(fixture.pool).await?;
        token(
            fixture,
            Uuid::now_v7(),
            self.peer,
            peer_profile,
            &self.peer_token,
        )
        .await?;
        token(
            fixture,
            self.kids_session,
            fixture.user,
            fixture.kids_profile,
            &self.kids_token,
        )
        .await?;
        Ok(())
    }

    async fn verify_rescan(&self, fixture: &ProgressiveFixture<'_>) -> anyhow::Result<()> {
        let before = file_inventory(fixture, self.library).await?;
        anyhow::ensure!(before.len() == 14);
        let scan =
            duskcue::workers::library_scanner::scan_library(fixture.pool, self.library, true, None)
                .await?;
        let after = file_inventory(fixture, self.library).await?;
        let rows = after
            .iter()
            .map(|(path, metadata)| (path.clone(), metadata["healthy"] == true))
            .collect::<Vec<_>>();
        let mut diagnostic = scan_diagnostic(&scan, &rows);
        diagnostic["pass"] = json!("update");
        diagnostic["identities_and_metadata_unchanged"] = json!(before == after);
        eprintln!("DUSKCUE_FORMAT_SCAN={diagnostic}");
        let items: i64 = sqlx::query_scalar("SELECT count(*) FROM media_items WHERE library_id=$1")
            .bind(self.library)
            .fetch_one(fixture.pool)
            .await?;
        anyhow::ensure!(
            scan.errors.is_empty()
                && scan.files_discovered == 14
                && scan.files_new == 0
                && scan.files_modified == 14
                && scan.items_created == 14
                && scan.files_deleted == 0
                && after.len() == 14
                && before == after
                && items == 14,
            "actual format scanner did not preserve its existing inventory on a full update pass: {diagnostic}"
        );
        Ok(())
    }
}

async fn file_inventory(
    fixture: &ProgressiveFixture<'_>,
    library: Uuid,
) -> anyhow::Result<Vec<(String, Value)>> {
    Ok(sqlx::query_as("SELECT f.file_path,jsonb_build_object('item',f.media_item_id,'file',f.id,'healthy',f.is_healthy,'size',f.file_size,'hash',f.file_hash,'container',f.container_format,'video_codec',f.video_codec,'video_resolution',f.video_resolution,'video_bitrate',f.video_bitrate,'video_dynamic_range',f.video_dynamic_range,'video_frame_rate',f.video_frame_rate,'audio_codec',f.audio_codec,'audio_channels',f.audio_channels,'audio_language',f.audio_language,'audio_bitrate',f.audio_bitrate,'runtime',f.runtime_seconds,'streams',f.additional_streams) FROM media_files f JOIN media_items i ON i.id=f.media_item_id WHERE i.library_id=$1 ORDER BY f.file_path LIMIT 15")
        .bind(library).fetch_all(fixture.pool).await?)
}

async fn token(
    fixture: &ProgressiveFixture<'_>,
    session: Uuid,
    user: Uuid,
    profile: Uuid,
    value: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '30 minutes')").bind(session).bind(user).bind(profile).bind(digest(value.as_bytes())).execute(fixture.pool).await?;
    Ok(())
}

fn validate(entry: &Value) -> anyhow::Result<()> {
    let name = entry["filename"].as_str().unwrap_or("");
    let id = entry["id"].as_str().unwrap_or("");
    let container = entry["container"].as_str().unwrap_or("");
    let master = entry["master"].as_str().unwrap_or("");
    let (codec, depth, range) = match master {
        "hevc-sdr-8" => ("hevc", 8, "sdr"),
        "hevc-sdr-10" => ("hevc", 10, "sdr"),
        "av1-sdr-8" => ("av1", 8, "sdr"),
        "av1-sdr-10" => ("av1", 10, "sdr"),
        "hevc-pq-10" => ("hevc", 10, "hdr10"),
        "hevc-hlg-10" => ("hevc", 10, "hlg"),
        _ => anyhow::bail!("format master is outside the six-master inventory"),
    };
    anyhow::ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
            && matches!(container, "mp4" | "mkv" | "webm")
            && (container != "webm" || codec == "av1")
            && id == format!("{master}-{container}")
            && name == format!("{id}.{container}")
            && entry["codec"] == codec
            && entry["bitDepth"] == depth
            && entry["dynamicRange"] == range
            && entry["videoIndex"] == 0
            && entry["audioIndex"] == 2
            && entry["audioHz"] == 880
            && entry["capability"].is_object()
            && entry["sha256"]
                .as_str()
                .is_some_and(|value| value.len() == 64
                    && value
                        .bytes()
                        .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())),
        "format manifest case is outside its fixed scope"
    );
    Ok(())
}

pub(super) async fn hash(path: &Path) -> anyhow::Result<String> {
    Ok(digest(&read(path, 8 * 1024 * 1024).await?))
}

async fn read(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= limit
    );
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .await?;
    let mut bytes = Vec::new();
    (&mut file).take(limit + 1).read_to_end(&mut bytes).await?;
    anyhow::ensure!(bytes.len() as u64 <= limit);
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}

fn scan_diagnostic(
    scan: &duskcue::workers::library_scanner::ScanResult,
    rows: &[(String, bool)],
) -> Value {
    let filename = |path: &str| {
        Path::new(path)
            .file_name()
            .map(|value| value.to_string_lossy().chars().take(96).collect::<String>())
            .unwrap_or_else(|| "unavailable".into())
    };
    json!({"discovered":scan.files_discovered,"new":scan.files_new,"modified":scan.files_modified,"created":scan.items_created,"errors_total":scan.errors.len(),
        "errors":scan.errors.iter().take(14).map(|error|json!({"file":filename(&error.path),"phase":error.phase.chars().take(32).collect::<String>(),"message":error.message.chars().filter(|value|!value.is_control()).take(512).collect::<String>()})).collect::<Vec<_>>(),
        "ingested":rows.iter().take(15).map(|(path,healthy)|json!({"file":filename(path),"healthy":healthy})).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_rejects_paths_and_unsupported_shapes() {
        let valid = json!({"id":"hevc-sdr-8-mp4","master":"hevc-sdr-8","filename":"hevc-sdr-8-mp4.mp4","container":"mp4","codec":"hevc","bitDepth":8,"dynamicRange":"sdr","videoIndex":0,"audioIndex":2,"audioHz":880,"capability":{},"sha256":"a".repeat(64)});
        assert!(validate(&valid).is_ok());
        for (key, value) in [
            ("filename", json!("../source.mp4")),
            ("codec", json!("h264")),
            ("bitDepth", json!(12)),
            ("dynamicRange", json!("unknown")),
            ("audioIndex", json!(1)),
            ("sha256", json!("x".repeat(64))),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert!(validate(&invalid).is_err());
        }
    }
}
