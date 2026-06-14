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

import { get, post, del } from './core.js';

export async function setup(data) {
    return post('/setup', data);
}

export async function loginWithInvite(data) {
    return post('/auth/invite', data);
}

export async function loginWithPassword(data) {
    return post('/auth/login', data);
}

export async function logout() {
    return post('/auth/logout');
}

export async function logoutAll() {
    return post('/auth/logout-all');
}

export async function startWebauthnAuth(data) {
    return post('/auth/webauthn/start', data);
}

export async function finishWebauthnAuth(data, challengeId) {
    return post('/auth/webauthn/finish', data, {
        headers: { 'X-Challenge-Id': challengeId },
    });
}

export async function verifyTotp(data) {
    return post('/auth/totp', data);
}

export async function authenticateWithReauthCode(data) {
    return post('/auth/reauth', data);
}

export async function requestReauthCode() {
    return post('/auth/reauth/request');
}

export async function createDeviceCode(data) {
    return post('/device/code', data);
}

export async function pollDeviceToken(data) {
    return post('/device/token', data);
}

export async function verifyDeviceCode(data) {
    return post('/device/verify', data);
}

export async function listSessions(params = {}) {
    return get('/user/sessions', params);
}

export async function deleteSession(sessionId) {
    return del(`/user/sessions/${sessionId}`);
}

export async function signOutEverywhere() {
    return post('/user/sign-out-everywhere');
}

export async function requestUserReauth() {
    return post('/user/request-reauth');
}

export async function listPasskeys() {
    return get('/user/passkeys');
}

export async function startPasskeyRegistration(data) {
    return post('/user/passkeys/register/start', data);
}

export async function finishPasskeyRegistration(data, challengeId) {
    return post('/user/passkeys/register/finish', data, {
        headers: { 'X-Challenge-Id': challengeId },
    });
}

export async function deletePasskey(passkeyId) {
    return del(`/user/passkeys/${passkeyId}`);
}

export async function listInvitations(params = {}) {
    return get('/invitations', params);
}

export async function createInvitation(data) {
    return post('/invitations', data);
}

export async function revokeInvitation(invitationId) {
    return del(`/invitations/${invitationId}`);
}

export async function resendInvitation(invitationId) {
    return post(`/invitations/${invitationId}/resend`);
}

export async function listCapabilities() {
    return get('/auth/capabilities');
}

