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

import { get, post, buildApiUrl } from './core.js';

export async function reportCapabilities(data) {
    return post('/device/capabilities', data);
}

export async function getCapabilities(params = {}) {
    return get('/device/capabilities', params);
}

export async function listCapabilityTests(params = {}) {
    return get('/device/capability-tests', params);
}

export async function startCapabilityWizard(data) {
    return post('/device/capability-tests/start', data);
}

export async function submitCapabilityTestResult(testId, data) {
    return post(`/device/capability-tests/${testId}/result`, data);
}

export function bandwidthProbeUrl() {
    return buildApiUrl('/probe/bandwidth');
}

export async function submitBandwidthProbeResult(data) {
    return post('/probe/bandwidth/result', data);
}

export async function submitTelemetry(data) {
    return post('/playback/telemetry', data);
}

export async function submitQoeReport(data) {
    return post('/playback/qoe', data);
}

export async function getNetworkQualitySummary(params = {}) {
    return get('/admin/quality/network', params);
}

export async function getDeviceCapabilitySummary(params = {}) {
    return get('/admin/quality/devices', params);
}

export async function getQoeSummary(params = {}) {
    return get('/admin/quality/qoe', params);
}

export async function getTranscodeBreakdown(params = {}) {
    return get('/admin/quality/transcodes', params);
}

