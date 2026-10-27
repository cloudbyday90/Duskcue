// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    CHUNK, Content, PLAYLIST, Prepared, VodLifecycle, body::Output, owner::ResponseEntry,
    queue::Queue,
};
use crate::domains::playback::PlaybackError;
use axum::{body::Body, response::Response};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::{fs::File, sync::oneshot};
use tokio_util::task::task_tracker::TaskTrackerToken;
use uuid::Uuid;

type Preparation<'a> = Pin<Box<dyn Future<Output = Result<Prepared, PlaybackError>> + Send + 'a>>;
type Ready =
    oneshot::Receiver<Result<(axum::http::StatusCode, axum::http::HeaderMap), PlaybackError>>;
type ResponseAdmission = Result<(Ready, Output), PlaybackError>;

pub(crate) struct ReadContext {
    pub(super) owner: Arc<VodLifecycle>,
    pub(super) entry: Arc<ResponseEntry>,
    pub(super) files: Vec<Option<File>>,
}
impl ReadContext {
    pub fn check(&self) -> Result<(), PlaybackError> {
        if self.entry.queue.cancelled() {
            Err(PlaybackError::LifecycleUnavailable)
        } else {
            Ok(())
        }
    }
    async fn stream(&mut self, content: Content) -> Result<(), PlaybackError> {
        match content {
            Content::File { id, offset, length } => {
                self.seek(id, offset).await?;
                let mut remaining = length;
                while remaining > 0 {
                    let part = self.chunk(id, remaining.min(CHUNK as u64) as usize).await?;
                    if part.is_empty() {
                        return Err(PlaybackError::FileNotFound);
                    }
                    remaining -= part.len() as u64;
                    if !self.entry.queue.push(part.into()).await {
                        return Err(PlaybackError::LifecycleUnavailable);
                    }
                }
            }
            Content::Bytes(bytes) => {
                if bytes.len() > PLAYLIST {
                    return Err(PlaybackError::SessionNotFound);
                }
                for offset in (0..bytes.len()).step_by(CHUNK) {
                    if !self
                        .entry
                        .queue
                        .push(bytes.slice(offset..(offset + CHUNK).min(bytes.len())))
                        .await
                    {
                        return Err(PlaybackError::LifecycleUnavailable);
                    }
                }
            }
            Content::Empty => {}
        }
        Ok(())
    }
}

struct Completion {
    owner: Arc<VodLifecycle>,
    id: Uuid,
    entry: Arc<ResponseEntry>,
    _generation_task: Option<TaskTrackerToken>,
    _viewer_task: Option<TaskTrackerToken>,
    done: bool,
}
impl Completion {
    fn finish(mut self) {
        if let Some(generation) = &self.entry.generation {
            generation.remove(self.id);
        }
        if let Some(viewer) = &self.entry.viewer {
            viewer.remove(self.id);
        }
        if let Ok(mut registry) = self.owner.registry.lock() {
            registry.responses.remove(&self.id);
        } else {
            self.owner.fail();
        }
        self.done = true;
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.done {
            self.entry.queue.cancel();
            if let Some(generation) = &self.entry.generation {
                generation.fail();
            }
            if let Some(viewer) = &self.entry.viewer {
                viewer.fail();
            }
            self.owner.fail();
        }
    }
}

impl VodLifecycle {
    #[cfg(test)]
    pub async fn response<F>(
        self: &Arc<Self>,
        generation: Option<Uuid>,
        prepare: F,
    ) -> Result<Response, PlaybackError>
    where
        F: for<'a> FnOnce(&'a mut ReadContext) -> Preparation<'a> + Send + 'static,
    {
        self.response_scoped(generation, None, prepare).await
    }
    pub(crate) async fn response_scoped<F>(
        self: &Arc<Self>,
        generation: Option<Uuid>,
        viewer: Option<Arc<super::Generation>>,
        prepare: F,
    ) -> Result<Response, PlaybackError>
    where
        F: for<'a> FnOnce(&'a mut ReadContext) -> Preparation<'a> + Send + 'static,
    {
        let (receive, output) = self.enqueue_response(generation, viewer, prepare)?;
        let (status, headers) = receive
            .await
            .map_err(|_| PlaybackError::LifecycleCleanup)??;
        let mut response = Response::new(Body::from_stream(output));
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        Ok(response)
    }
    fn enqueue_response<F>(
        self: &Arc<Self>,
        generation: Option<Uuid>,
        viewer: Option<Arc<super::Generation>>,
        prepare: F,
    ) -> ResponseAdmission
    where
        F: for<'a> FnOnce(&'a mut ReadContext) -> Preparation<'a> + Send + 'static,
    {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| PlaybackError::LifecycleCleanup)?;
        if registry.closed {
            return Err(PlaybackError::LifecycleUnavailable);
        }
        let permit = self
            .responses
            .clone()
            .try_acquire_owned()
            .map_err(|_| PlaybackError::LifecycleUnavailable)?;
        let id = Uuid::now_v7();
        let queue = Arc::new(Queue::new());
        let generation = generation
            .map(|id| {
                registry
                    .generations
                    .get(&id)
                    .cloned()
                    .ok_or(PlaybackError::SessionNotFound)
            })
            .transpose()?;
        let generation_task = generation
            .as_ref()
            .map(|g| g.register(id, &queue))
            .transpose()?;
        let viewer_task = viewer
            .as_ref()
            .map(|viewer| viewer.register(id, &queue))
            .transpose()?;
        let entry = Arc::new(ResponseEntry {
            queue: queue.clone(),
            generation,
            viewer,
            _permit: permit,
        });
        registry.responses.insert(id, entry.clone());
        let completion = Completion {
            owner: self.clone(),
            id,
            entry: entry.clone(),
            _generation_task: generation_task,
            _viewer_task: viewer_task,
            done: false,
        };
        let (send, receive) = oneshot::channel();
        let owner = self.clone();
        self.response_tasks.spawn(async move {
            let mut ctx = ReadContext {
                owner,
                entry,
                files: Vec::new(),
            };
            match prepare(&mut ctx).await {
                Ok(prepared) => {
                    if send.send(Ok((prepared.status, prepared.headers))).is_err() {
                        ctx.entry.queue.cancel();
                    }
                    if ctx.stream(prepared.content).await.is_err() {
                        ctx.entry.queue.cancel();
                    }
                }
                Err(error) => {
                    let _ = send.send(Err(error));
                    ctx.entry.queue.cancel();
                }
            }
            ctx.close_files().await;
            ctx.entry.queue.finish();
            ctx.entry.queue.drained().await;
            completion.finish();
        });
        drop(registry);
        Ok((receive, Output::new(queue)))
    }
}
