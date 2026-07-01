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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use sqlx::Row;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::domains::tv::service as tv_service;
use crate::domains::tv::types::TvSurfaceSectionType;
use crate::services::event_bus::EventBus;

const MEDIA_VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "ts", "m2ts", "wmv", "flv", "webm", "mov", "mpg", "mpeg", "m4v", "3gp",
    "ogv", "iso", "img",
];

const MEDIA_SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];

const DEBOUNCE_TIMEOUT_SECS: u64 = 3;

const BULK_IMPORT_THRESHOLD: usize = 10;

const LIBRARY_COOLDOWN_SECS: u64 = 10;

struct WatchedLibrary {
    paths: Vec<PathBuf>,
}

struct WatchEvent {}

struct PendingDirectory {
    count: usize,
}

pub struct LibraryWatcherManager {
    pool: sqlx::PgPool,
    enrichment: Arc<crate::services::metadata::EnrichmentOrchestrator>,
    event_bus: Arc<EventBus>,
    watched: Arc<std::sync::Mutex<HashMap<Uuid, WatchedLibrary>>>,
    debouncer: Arc<
        std::sync::Mutex<
            Option<
                notify_debouncer_full::Debouncer<
                    notify::RecommendedWatcher,
                    notify_debouncer_full::RecommendedCache,
                >,
            >,
        >,
    >,
    pending: Arc<std::sync::Mutex<HashMap<PathBuf, PendingDirectory>>>,
    cooldowns: Arc<std::sync::Mutex<HashMap<Uuid, std::time::Instant>>>,
}

impl LibraryWatcherManager {
    pub fn new(
        pool: sqlx::PgPool,
        enrichment: Arc<crate::services::metadata::EnrichmentOrchestrator>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            pool,
            enrichment,
            event_bus,
            watched: Arc::new(std::sync::Mutex::new(HashMap::new())),
            debouncer: Arc::new(std::sync::Mutex::new(None)),
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            cooldowns: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(
        self: &Arc<Self>,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<(), String> {
        let library_paths = self.load_all_library_paths().await?;

        if library_paths.is_empty() {
            tracing::info!("No libraries to watch");
            return Ok(());
        }

        let (tx, rx) = mpsc::channel::<WatchEvent>(256);
        let mut debouncer = self.build_debouncer(tx)?;

        for (library_id, paths) in &library_paths {
            for path in paths {
                if let Err(e) = debouncer.watch(path, RecursiveMode::Recursive) {
                    tracing::warn!(
                        library_id = %library_id,
                        path = %path.display(),
                        error = %e,
                        "Failed to start FS watcher for library path — scheduled scans will still work"
                    );
                    continue;
                }
                tracing::info!(
                    library_id = %library_id,
                    path = %path.display(),
                    "Watching library path"
                );
            }

            self.watched.lock().unwrap().insert(
                *library_id,
                WatchedLibrary {
                    paths: paths.clone(),
                },
            );
        }

        *self.debouncer.lock().unwrap() = Some(debouncer);

        tracing::info!(
            library_count = library_paths.len(),
            "Filesystem watcher started"
        );

        let manager = Arc::clone(self);
        tracker.spawn(async move {
            manager.run_event_processor(rx, shutdown).await;
        });

        Ok(())
    }

    pub fn stop(&self) {
        if let Some(debouncer) = self.debouncer.lock().unwrap().take() {
            drop(debouncer);
            tracing::info!("Filesystem watcher stopped");
        }
        self.watched.lock().unwrap().clear();
    }

    pub fn watch_library(&self, library_id: Uuid, paths: Vec<PathBuf>) -> Result<(), String> {
        let mut debouncer_guard = self.debouncer.lock().unwrap();
        let Some(debouncer) = debouncer_guard.as_mut() else {
            tracing::warn!(
                library_id = %library_id,
                "Cannot watch library — debouncer not initialized"
            );
            return Err("Debouncer not initialized".to_string());
        };

        for path in &paths {
            if let Err(e) = debouncer.watch(path, RecursiveMode::Recursive) {
                tracing::warn!(
                    library_id = %library_id,
                    path = %path.display(),
                    error = %e,
                    "Failed to start FS watcher for new library path"
                );
            } else {
                tracing::info!(
                    library_id = %library_id,
                    path = %path.display(),
                    "Watching new library path"
                );
            }
        }

        drop(debouncer_guard);

        self.watched
            .lock()
            .unwrap()
            .insert(library_id, WatchedLibrary { paths });

        Ok(())
    }

    pub fn unwatch_library(&self, library_id: Uuid) {
        let mut watched = self.watched.lock().unwrap();
        if let Some(lib) = watched.remove(&library_id) {
            let mut debouncer_guard = self.debouncer.lock().unwrap();
            if let Some(debouncer) = debouncer_guard.as_mut() {
                for path in &lib.paths {
                    let _ = debouncer.unwatch(path);
                    tracing::info!(
                        library_id = %library_id,
                        path = %path.display(),
                        "Stopped watching library path"
                    );
                }
            }
        }
    }

    fn build_debouncer(
        &self,
        tx: mpsc::Sender<WatchEvent>,
    ) -> Result<
        notify_debouncer_full::Debouncer<
            notify::RecommendedWatcher,
            notify_debouncer_full::RecommendedCache,
        >,
        String,
    > {
        let pending = Arc::clone(&self.pending);

        notify_debouncer_full::new_debouncer(
            Duration::from_secs(DEBOUNCE_TIMEOUT_SECS),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| match result {
                Ok(events) => {
                    let mut media_paths: Vec<PathBuf> = Vec::new();
                    for event in events {
                        use notify::EventKind;
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
                            _ => continue,
                        }

                        for path in &event.paths {
                            if is_media_file(path) {
                                media_paths.push(path.clone());
                            }
                        }
                    }

                    if media_paths.is_empty() {
                        return;
                    }

                    {
                        let mut guard = pending.lock().unwrap();
                        for path in &media_paths {
                            let dir = path.parent().unwrap_or(path).to_path_buf();
                            let entry = guard.entry(dir).or_insert(PendingDirectory { count: 0 });
                            entry.count += 1;
                        }
                    }

                    let _ = tx.try_send(WatchEvent {});
                }
                Err(errors) => {
                    for error in errors {
                        tracing::debug!(error = %error, "Filesystem watch error");
                    }
                }
            },
        )
        .map_err(|e| format!("Failed to create debouncer: {e}"))
    }

    async fn run_event_processor(
        &self,
        mut rx: mpsc::Receiver<WatchEvent>,
        shutdown: CancellationToken,
    ) {
        tracing::info!("FS watcher event processor started");

        let pending = Arc::clone(&self.pending);
        let cooldowns = Arc::clone(&self.cooldowns);
        let watched = Arc::clone(&self.watched);
        let pool = self.pool.clone();
        let enrichment = self.enrichment.clone();
        let event_bus = self.event_bus.clone();

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(_event) => {
                            let batch = drain_pending(&pending);

                            if batch.is_empty() {
                                continue;
                            }

                            process_batch(
                                &batch,
                                &watched,
                                &cooldowns,
                                &pool,
                                &enrichment,
                                &event_bus,
                            )
                            .await;
                        }
                        None => {
                            tracing::info!("FS watcher channel closed");
                            break;
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    tracing::info!("FS watcher event processor shutting down");
                    break;
                }
            }
        }
    }

    async fn load_all_library_paths(&self) -> Result<HashMap<Uuid, Vec<PathBuf>>, String> {
        let rows = sqlx::query(
            r#"SELECT lp.library_id, lp.path
               FROM library_paths lp
               JOIN libraries l ON l.id = lp.library_id
               WHERE l.deleted_at IS NULL
               AND lp.scan_enabled = true"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to load library paths: {e}"))?;

        let mut result: HashMap<Uuid, Vec<PathBuf>> = HashMap::new();
        for row in &rows {
            let library_id: Uuid = row.get("library_id");
            let path: String = row.get("path");
            result
                .entry(library_id)
                .or_default()
                .push(PathBuf::from(path));
        }

        Ok(result)
    }
}

fn drain_pending(
    pending: &Arc<std::sync::Mutex<HashMap<PathBuf, PendingDirectory>>>,
) -> Vec<(PathBuf, usize)> {
    let mut guard = pending.lock().unwrap();
    guard.drain().map(|(dir, info)| (dir, info.count)).collect()
}

async fn process_batch(
    batch: &[(PathBuf, usize)],
    watched: &Arc<std::sync::Mutex<HashMap<Uuid, WatchedLibrary>>>,
    cooldowns: &Arc<std::sync::Mutex<HashMap<Uuid, std::time::Instant>>>,
    pool: &sqlx::PgPool,
    enrichment: &Arc<crate::services::metadata::EnrichmentOrchestrator>,
    event_bus: &Arc<EventBus>,
) {
    for (directory, count) in batch {
        let library_id = {
            let guard = watched.lock().unwrap();
            resolve_library_for_path(&guard, directory)
        };

        let Some(library_id) = library_id else {
            continue;
        };

        {
            let guard = cooldowns.lock().unwrap();
            if let Some(last) = guard.get(&library_id)
                && last.elapsed() < Duration::from_secs(LIBRARY_COOLDOWN_SECS)
            {
                tracing::debug!(
                    library_id = %library_id,
                    "Skipping FS-triggered scan — library in cooldown"
                );
                continue;
            }
        }

        let quick = if *count >= BULK_IMPORT_THRESHOLD {
            tracing::info!(
                library_id = %library_id,
                directory = %directory.display(),
                file_count = count,
                "Bulk import detected, triggering full scan"
            );
            false
        } else {
            tracing::info!(
                library_id = %library_id,
                directory = %directory.display(),
                file_count = count,
                "FS change detected, triggering scan"
            );
            true
        };

        {
            let mut guard = cooldowns.lock().unwrap();
            guard.insert(library_id, std::time::Instant::now());
        }

        let pool = pool.clone();
        let enrichment = enrichment.clone();
        let event_bus = event_bus.clone();
        tokio::spawn(async move {
            match crate::workers::library_scanner::scan_library(
                &pool,
                library_id,
                quick,
                Some(enrichment),
            )
            .await
            {
                Ok(result) => {
                    tracing::info!(
                        library_id = %library_id,
                        scan_type = if quick { "quick" } else { "full" },
                        new = result.files_new,
                        modified = result.files_modified,
                        deleted = result.files_deleted,
                        "FS-triggered scan completed"
                    );
                    if let Err(e) = tv_service::publish_tv_surface_changed_for_library(
                        &pool,
                        &event_bus,
                        library_id,
                        "library_scan_completed",
                        all_tv_sections(),
                    )
                    .await
                    {
                        tracing::warn!(
                            library_id = %library_id,
                            error = %e,
                            "Failed to publish TV surface change after FS-triggered scan"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        library_id = %library_id,
                        error = %e,
                        "FS-triggered scan failed"
                    );
                }
            }
        });
    }
}

fn all_tv_sections() -> Vec<TvSurfaceSectionType> {
    vec![
        TvSurfaceSectionType::Continue,
        TvSurfaceSectionType::NextUp,
        TvSurfaceSectionType::NewEpisodes,
        TvSurfaceSectionType::Recommended,
    ]
}

fn resolve_library_for_path(watched: &HashMap<Uuid, WatchedLibrary>, path: &Path) -> Option<Uuid> {
    for (library_id, lib) in watched {
        for lib_path in &lib.paths {
            if path.starts_with(lib_path) {
                return Some(*library_id);
            }
        }
    }
    None
}

fn is_media_file(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };

    MEDIA_VIDEO_EXTENSIONS.contains(&ext.as_str())
        || MEDIA_SUBTITLE_EXTENSIONS.contains(&ext.as_str())
}
