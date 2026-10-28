// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{control::Work, output::Output, preparation::PreparedCommand};
use crate::services::{
    playback_admission::{AdmissionError, Request},
    vod_lifecycle::Generation,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Notify;
use uuid::Uuid;

pub(crate) struct Commands {
    viewers: Mutex<BTreeMap<Uuid, Arc<Viewer>>>,
}

pub(super) struct Viewer {
    pub prepared: Arc<PreparedCommand>,
    pub group: Arc<Generation>,
    pub cleanup_uncertain: AtomicBool,
    pub retry: Notify,
    pub changed: Notify,
    slot: Mutex<Slot>,
}

struct Slot {
    current: Arc<Output>,
    outputs: BTreeMap<Uuid, Arc<Output>>,
    busy: bool,
    pending: Option<Work>,
}

pub(super) struct ChangeGuard {
    viewer: Arc<Viewer>,
    transferred: bool,
}

impl Drop for ChangeGuard {
    fn drop(&mut self) {
        if !self.transferred {
            self.viewer.finish();
        }
    }
}

impl Commands {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            viewers: Mutex::new(BTreeMap::new()),
        })
    }
    pub(super) fn register(
        &self,
        prepared: PreparedCommand,
    ) -> Result<Arc<Viewer>, AdmissionError> {
        let mut viewers = self.viewers.lock().map_err(|_| AdmissionError::Closed)?;
        if let Some(saved) = viewers.get(&prepared.request.id) {
            return Err(
                if saved.prepared.binding.intent == prepared.binding.intent
                    && same_actor(saved.prepared.request, prepared.request)
                {
                    AdmissionError::Pending
                } else {
                    AdmissionError::Conflict
                },
            );
        }
        if viewers.len() >= 128 {
            return Err(AdmissionError::Capacity);
        }
        let prepared = Arc::new(prepared);
        let current = Output::new(prepared.clone(), None, 0, None, None)?;
        let viewer = Arc::new(Viewer {
            prepared,
            group: Generation::viewer(),
            cleanup_uncertain: AtomicBool::new(false),
            retry: Notify::new(),
            changed: Notify::new(),
            slot: Mutex::new(Slot {
                outputs: BTreeMap::from([(current.id, current.clone())]),
                current,
                busy: false,
                pending: None,
            }),
        });
        viewers.insert(viewer.prepared.request.id, viewer.clone());
        Ok(viewer)
    }
    pub(super) fn get(&self, id: Uuid) -> Option<Arc<Viewer>> {
        self.viewers.lock().ok()?.get(&id).cloned()
    }
    pub(super) fn remove(&self, viewer: &Arc<Viewer>) -> Result<(), AdmissionError> {
        let mut viewers = self.viewers.lock().map_err(|_| AdmissionError::Closed)?;
        if viewers
            .get(&viewer.prepared.request.id)
            .is_some_and(|v| Arc::ptr_eq(v, viewer))
        {
            viewers.remove(&viewer.prepared.request.id);
        }
        Ok(())
    }
}

pub(super) fn same_actor(a: Request, b: Request) -> bool {
    a.actor.user == b.actor.user
        && a.actor.session == b.actor.session
        && a.actor.profile == b.actor.profile
        && a.actor.version == b.actor.version
        && a.actor.transport == b.actor.transport
        && a.profile_revision == b.profile_revision
        && a.subject == b.subject
}

impl Viewer {
    pub(super) fn current(&self) -> Result<Arc<Output>, AdmissionError> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| AdmissionError::Cleanup)?
            .current
            .clone())
    }
    pub(super) fn outputs(&self) -> Result<Vec<Arc<Output>>, AdmissionError> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| AdmissionError::Cleanup)?
            .outputs
            .values()
            .cloned()
            .collect())
    }
    pub(super) fn begin(self: &Arc<Self>) -> Result<ChangeGuard, AdmissionError> {
        let mut slot = self.slot.lock().map_err(|_| AdmissionError::Cleanup)?;
        if !self.group.active() || self.cleanup_uncertain.load(Ordering::Acquire) {
            return Err(AdmissionError::Retired);
        }
        if slot.busy {
            return Err(AdmissionError::Pending);
        }
        slot.busy = true;
        Ok(ChangeGuard {
            viewer: self.clone(),
            transferred: false,
        })
    }
    pub(super) fn queue(
        &self,
        work: Work,
        target: Arc<Output>,
        mut guard: ChangeGuard,
    ) -> Result<(), AdmissionError> {
        let mut slot = self.slot.lock().map_err(|_| AdmissionError::Cleanup)?;
        if !slot.busy || slot.pending.is_some() || slot.outputs.len() >= 2 {
            return Err(AdmissionError::Pending);
        }
        slot.outputs.insert(target.id, target);
        slot.pending = Some(work);
        guard.transferred = true;
        self.changed.notify_one();
        Ok(())
    }
    pub(super) fn take(&self) -> Result<Option<Work>, AdmissionError> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| AdmissionError::Cleanup)?
            .pending
            .take())
    }
    pub(super) fn finish(&self) {
        if let Ok(mut slot) = self.slot.lock() {
            let current = slot.current.id;
            slot.outputs
                .retain(|id, o| *id == current || !o.cleaned.load(Ordering::Acquire));
            slot.busy = false;
        } else {
            self.group.fail();
        }
        self.changed.notify_one();
    }
    pub(super) fn promote(&self, output: &Arc<Output>) -> Result<(), AdmissionError> {
        let mut slot = self.slot.lock().map_err(|_| AdmissionError::Cleanup)?;
        if !self.group.active() {
            return Err(AdmissionError::Retired);
        }
        slot.current = output.clone();
        output.published.store(true, Ordering::Release);
        Ok(())
    }
}
