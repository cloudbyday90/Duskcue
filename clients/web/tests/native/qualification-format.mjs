// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { queryFormatCapability } from '../fixtures/format-capabilities.mjs';
import { observeFormatRequests } from '../fixtures/format-observation.mjs';
import { assertFormatCases, assertFormatStart } from './qualification-format-policy.mjs';
import { qualifyFormatPlayback } from './qualification-format-case.mjs';
import { releaseNativeAudio } from './qualification-live-media.mjs';
import { createFormatPacing } from './qualification-format-pacing.mjs';

export async function runFormatMatrix({ page, ready, origin, client, directory, openCase, closeCase, verifyFormat, abortSignal }) {
    if (!['browser', 'native'].includes(client) || typeof verifyFormat !== 'function') throw new Error('A real client and private format verifier are required.');
    const cases = assertFormatCases(ready.formats);
    const pacing = createFormatPacing({ client, signal: abortSignal });
    const result = { resourceId: ready.resourceId, client, passed: false, cases: [], limitations: ['Capability reports are diagnostics, separate from actual presentation.',
        'The current product sends no device profile; SDR cases qualify the real conservative H.264/AAC fallback.',
        'Reference-signal checks are performed by the backend; physical audibility, HDR display fidelity and performance are not established.'], pacing: pacing.evidence };
    let failure;
    try {
        await pacing.begin();
        for (const candidate of cases) {
            if (abortSignal?.aborted) throw new Error('The format matrix was canceled.');
            const detail = { id: candidate.id, index: candidate.index, item: candidate.item, file: candidate.file, sha256: candidate.sha256, passed: false, pacing: await pacing.next() };
            result.cases.push(detail);
            const observation = observeFormatRequests(page, { origin, token: ready.token });
            let closed = false;
            let caseError;
            try {
                await openCase(candidate);
                const region = page.getByRole('region', { name: 'Media player', exact: true });
                await expect(region).toBeVisible();
                await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/start').length, { timeout: 20000 }).toBe(1);
                const start = assertFormatStart(observation.records.find((record) => record.path === '/api/v1/playback/start'), candidate, client);
                const evidence = { resourceId: ready.resourceId, phase: `format-${client}-${String(candidate.index).padStart(2, '0')}`,
                    caseId: candidate.id, itemId: candidate.item, fileId: candidate.file };
                if (candidate.dynamicRange === 'sdr') {
                    detail.playback = await qualifyFormatPlayback({ page, region, observation, start, candidate, client, abortSignal });
                    detail.sourceCapability = await page.evaluate(queryFormatCapability, candidate.capability);
                    const { sessionId, transcodeId, previousTranscodeId, positionMs, paused, attempts } = detail.playback;
                    detail.backend = await verifyFormat({ ...evidence, sessionId, transcodeId, previousTranscodeId, positionMs, paused, attempts });
                    if (detail.backend?.passed !== true || detail.backend.capability?.type !== 'media-source') throw new Error('Actual format output verification or decoding configuration is missing.');
                    detail.outputCapability = await page.evaluate(queryFormatCapability, detail.backend.capability);
                } else {
                    detail.sourceCapability = await page.evaluate(queryFormatCapability, candidate.capability);
                    await expect(region.locator('.error-message[role="alert"]')).toBeVisible();
                    await expect(region.locator('.error-message[role="alert"]')).toHaveText('HDR conversion is not supported on this device yet.');
                    detail.refusal = { status: start.status, code: start.error.code, visibleError: await region.locator('.error-message[role="alert"]').innerText() };
                    if (observation.records.some((record) => record.path.includes('/transcode/'))) throw new Error('Refused HDR playback allocated client media delivery.');
                    detail.backend = await verifyFormat({ ...evidence, status: start.status, code: start.error.code });
                    if (detail.backend?.passed !== true) throw new Error('The explicit HDR refusal did not pass independent verification.');
                }
                await closeCase(candidate);
                closed = true;
                await observation.drain();
                if (candidate.dynamicRange === 'sdr') {
                    const stops = observation.records.filter((record) => record.path === '/api/v1/playback/stop' && record.status === 200
                        && record.request?.session_id === detail.playback.sessionId);
                    if (stops.length !== 1) throw new Error('Format Close did not release its exact session once.');
                }
                detail.passed = true;
            } catch (error) {
                caseError = error;
                if (error?.formatFailure) detail.failure = error.formatFailure;
                detail.error = String(error?.message || 'Format qualification failed.').replaceAll(ready.token, '[redacted]').slice(0, 1200);
                await page.screenshot({ path: join(directory, `format-${client}-${String(candidate.index).padStart(2, '0')}-failed.png`) }).catch(() => {});
                throw error;
            } finally {
                const failures = [];
                for (const [name, cleanup] of [
                    ['presentation', () => page.evaluate(() => window.formatPresentation?.detach())],
                    ['audio', () => releaseNativeAudio(page)],
                    ['playback', async () => { if (!closed) await closeCase(candidate); }],
                    ['observation', () => observation.drain()],
                ]) {
                    try { await cleanup(); } catch { failures.push(name); }
                }
                observation.detach();
                detail.requests = observation.records;
                detail.pacing.observedClientRequests = observation.records.length;
                pacing.evidence.maxObservedClientRequests = Math.max(pacing.evidence.maxObservedClientRequests, observation.records.length);
                detail.cleanup = { passed: failures.length === 0, failures };
                if (failures.length && !caseError) throw new Error('Owned format case cleanup failed.');
            }
        }
        result.passed = true;
    } catch (error) { failure = error; result.error = String(error?.message || 'Format qualification failed.').replaceAll(ready.token, '[redacted]').slice(0, 1200); }
    finally { await writeFile(join(directory, `format-${client}.json`), `${JSON.stringify(result, null, 2)}\n`); }
    if (failure) throw failure;
    return { passed: true, cases: 14, played: 10, refused: 4, client, sourceIds: cases.map((candidate) => candidate.id) };
}
