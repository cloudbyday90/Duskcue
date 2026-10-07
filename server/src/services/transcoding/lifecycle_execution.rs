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

use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

pub(crate) struct ExecutionMonitor {
    state: watch::Receiver<ExecutionState>,
    cancellation: watch::Receiver<u64>,
}

impl ExecutionMonitor {
    pub(crate) fn new(
        state: watch::Receiver<ExecutionState>,
        cancellation: watch::Receiver<u64>,
    ) -> Self {
        Self {
            state,
            cancellation,
        }
    }

    pub(crate) fn state(&self) -> ExecutionState {
        if *self.cancellation.borrow() > 0 {
            return ExecutionState::Cancelled;
        }
        let state = *self.state.borrow();
        if state == ExecutionState::Running
            && (self.state.has_changed().is_err() || self.cancellation.has_changed().is_err())
        {
            ExecutionState::Failed
        } else {
            state
        }
    }

    pub(crate) async fn changed(&mut self) {
        tokio::select! {
            _ = self.state.changed() => {},
            _ = self.cancellation.changed() => {},
        }
    }
}
