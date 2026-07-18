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

import { del, get, patch, post, put } from './core.js';

export async function listProfiles() {
    return get('/profiles', {}, { profileScoped: false });
}

export async function createProfile(data) {
    return post('/profiles', data);
}

export async function updateProfile(profileId, data) {
    return patch(`/profiles/${profileId}`, data);
}

export async function deleteProfile(profileId) {
    return del(`/profiles/${profileId}`);
}

export async function switchProfile(profileId, data = {}) {
    return post(`/profiles/${profileId}/switch`, data, { profileScoped: false });
}

export async function unlockParentProfile(data) {
    return post('/profiles/parent-unlock', data, { profileScoped: false });
}

export async function listAmbientChannels() {
    return get('/ambient-channels');
}

export async function createAmbientChannel(data) {
    return post('/ambient-channels', data);
}

export async function nextAmbientChannelItem(channelId, afterMediaItemId = null) {
    return post(`/ambient-channels/${channelId}/next`, { after_media_item_id: afterMediaItemId });
}

export async function replaceAmbientChannelItems(channelId, mediaItemIds) {
    return put(`/ambient-channels/${channelId}/items`, { media_item_ids: mediaItemIds });
}
