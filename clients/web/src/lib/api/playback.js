/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { get, post, patch, put, del, buildApiUrl } from './core.js';

export async function startPlayback(data) {
    return post('/playback/start', data);
}

export async function heartbeat(data) {
    return post('/playback/heartbeat', data);
}

export async function stopPlayback(data) {
    return post('/playback/stop', data);
}

export async function seek(data) {
    return post('/playback/seek', data);
}

export async function getPlaybackInfo(sessionId) {
    return get(`/playback/info/${sessionId}`);
}

export function streamFileUrl(mediaFileId) {
    return buildApiUrl(`/stream/${mediaFileId}`);
}

export function transcodeManifestUrl(sessionId) {
    return buildApiUrl(`/transcode/${sessionId}/manifest.m3u8`);
}

export function transcodePlaylistUrl(sessionId, rendition) {
    return buildApiUrl(`/transcode/${sessionId}/${rendition}/index.m3u8`);
}

export function transcodeSegmentUrl(sessionId, rendition, segment) {
    return buildApiUrl(`/transcode/${sessionId}/${rendition}/${segment}`);
}

export async function getWatchData(itemId) {
    return get(`/items/${itemId}/watch-data`);
}

export async function updateWatchData(itemId, data) {
    return put(`/items/${itemId}/watch-data`, data);
}

export async function listBookmarks(itemId) {
    return get(`/items/${itemId}/bookmarks`);
}

export async function createBookmark(itemId, data) {
    return post(`/items/${itemId}/bookmarks`, data);
}

export async function deleteBookmark(itemId, bookmarkId) {
    return del(`/items/${itemId}/bookmarks/${bookmarkId}`);
}

export async function listPlaylists(params = {}) {
    return get('/playlists', params);
}

export async function getPlaylist(playlistId) {
    return get(`/playlists/${playlistId}`);
}

export async function createPlaylist(data) {
    return post('/playlists', data);
}

export async function updatePlaylist(playlistId, data) {
    return patch(`/playlists/${playlistId}`, data);
}

export async function deletePlaylist(playlistId) {
    return del(`/playlists/${playlistId}`);
}

export async function listPlaylistItems(playlistId, params = {}) {
    return get(`/playlists/${playlistId}/items`, params);
}

export async function addPlaylistItem(playlistId, data) {
    return post(`/playlists/${playlistId}/items`, data);
}

export async function removePlaylistItem(playlistId, itemId) {
    return del(`/playlists/${playlistId}/items/${itemId}`);
}

export async function listStreamingPolicies(params = {}) {
    return get('/streaming-policies', params);
}

export async function getStreamingPolicy(policyId) {
    return get(`/streaming-policies/${policyId}`);
}

export async function createStreamingPolicy(data) {
    return post('/streaming-policies', data);
}

export async function updateStreamingPolicy(policyId, data) {
    return patch(`/streaming-policies/${policyId}`, data);
}

export async function deleteStreamingPolicy(policyId) {
    return del(`/streaming-policies/${policyId}`);
}

export async function getEffectiveStreamingLimits(userId) {
    return get(`/users/${userId}/streaming-limits`);
}

