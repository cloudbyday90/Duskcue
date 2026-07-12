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

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlaybackError {
    #[error("media item not found")]
    MediaNotFound,

    #[error("user lacks library access or play_media capability")]
    AccessDenied,

    #[error("transcode capacity reached")]
    TranscodeCapacityReached,

    #[error("FFmpeg process failed: {0}")]
    FfmpegFailed(String),

    #[error("session already active for this item")]
    SessionAlreadyActive,

    #[error("invalid seek position: {0}")]
    InvalidSeekPosition(String),

    #[error("invalid playback mode: {0}")]
    InvalidPlaybackMode(String),

    #[error("invalid byte range for direct stream: {0}")]
    InvalidByteRange(String),

    #[error("hardware acceleration initialization failed, fell back to software: {0}")]
    HwAccelFallback(String),

    #[error("FFmpeg process crashed during transcode; session terminated")]
    FfmpegCrashed,

    #[error("transcode disk space exhausted")]
    DiskSpaceExhausted,

    #[error("client IP address blocked by streaming policy")]
    IpBlocked,

    #[error("per-user stream limit exceeded")]
    StreamLimitExceeded,

    #[error("resolution requires direct play — transcode restricted by policy")]
    TranscodeRestrictedByPolicy,

    #[error("session not found")]
    SessionNotFound,

    #[error("media file not found")]
    FileNotFound,

    #[error("media file is unhealthy: {0}")]
    FileUnhealthy(String),

    #[error("streaming policy not found")]
    PolicyNotFound,

    #[error("policy name already exists: {0}")]
    PolicyNameExists(String),

    #[error("system policy cannot be deleted")]
    SystemPolicyCannotBeDeleted,

    #[error("cannot remove default policy without assigning a replacement")]
    CannotRemoveDefaultPolicy,

    #[error("invalid transcode resolution: {0}")]
    InvalidResolution(String),

    #[error("invalid IP range: {0}")]
    InvalidIpRange(String),

    #[error("invalid stream decision: {0}")]
    InvalidStreamDecision(String),

    #[error("user item data not found")]
    UserItemDataNotFound,

    #[error("bookmark not found")]
    BookmarkNotFound,

    #[error("playlist not found")]
    PlaylistNotFound,

    #[error("playlist item not found")]
    PlaylistItemNotFound,

    #[error("invalid playlist visibility: {0}")]
    InvalidVisibility(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
