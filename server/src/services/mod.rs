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

pub mod artwork_delivery;
pub mod artwork_downloader;
pub mod conditions;
pub mod decision_engine;
pub mod encryption;
pub mod enrichment_persistence;
pub mod event_bus;
pub mod events_handler;
pub mod fanart_client;
pub mod fs_watcher;
pub mod geoip;
pub mod hw_accel;
pub mod image_pipeline;
pub mod media_matching;
pub mod metadata;
pub mod nfo_parser;
pub mod omdb_client;
pub mod opensubtitles_client;
pub mod overlays;
pub mod sandbox;
pub mod scheduler;
pub mod segments;
pub mod storyboards;
pub mod subtitle_discovery;
pub mod subtitles;
pub mod subdl_client;
pub mod tmdb_client;
pub mod transcoding;
pub mod trakt_client;
pub mod tvdb_client;
