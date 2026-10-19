// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { assertFormatCases } from './qualification-format-policy.mjs';
import { createFormatPacing } from './qualification-format-pacing.mjs';
import { releaseNativeAudio } from './qualification-live-media.mjs';
import { observeSourceRequests } from './qualification-source-observation.mjs';
import { assertSourceStart } from './qualification-source-policy.mjs';
import { qualifySourcePlayback } from './qualification-source-case.mjs';
import { queryFormatCapability } from '../fixtures/format-capabilities.mjs';

export async function runSourceMatrix({ page, ready, origin, client, directory, openCase, closeCase, verifySource, abortSignal }) {
    if (ready.sourceCapabilities !== true || !['browser', 'native'].includes(client) || typeof verifySource !== 'function') throw new Error('A negotiated real source matrix and verifier are required.');
    const candidates = assertFormatCases(ready.formats);
    const pacing = createFormatPacing({ client, signal: abortSignal });
    const result = { resourceId: ready.resourceId, client, passed: false, cases: [], pacing: pacing.evidence,
        limitations: ['MediaCapabilities predictions are separate from actual presented frames and independently decoded output.',
            'Physical audibility, HDR display fidelity and high resolution performance are not established.'] };
    let failure;
    let optimized = 0;
    try {
        await pacing.begin();
        for (const candidate of candidates) {
            if (abortSignal?.aborted) throw new Error('The source matrix was canceled.');
            const detail = { id: candidate.id, index: candidate.index, item: candidate.item, file: candidate.file, sha256: candidate.sha256,
                passed: false, pacing: await pacing.next() };
            result.cases.push(detail);
            const observation = observeSourceRequests(page, { origin, token: ready.token });
            let closed = false;
            let caseError;
            try {
                await openCase(candidate);
                const region = page.getByRole('region', { name: 'Media player', exact: true });
                await expect(region).toBeVisible();
                await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/prepare').length, { timeout: 20000 }).toBe(1);
                await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/start').length, { timeout: 20000 }).toBe(1);
                const contract = assertSourceStart(observation.records, candidate, client);
                detail.preparation = contract.preparation;
                detail.observations = contract.observations;
                detail.route = contract.route;
                detail.decision = contract.start.reply?.stream_decision || null;
                const evidence = { resourceId: ready.resourceId, phase: `source-${client}-${String(candidate.index).padStart(2, '0')}`,
                    caseId: candidate.id, itemId: candidate.item, fileId: candidate.file, preparation: contract.preparation,
                    observations: contract.observations, route: contract.route, decision: detail.decision,
                    capabilityProtocol: 1, qualityMode: 'auto', transport: client === 'native' ? 'bearer' : 'cookie' };
                if (candidate.dynamicRange === 'sdr') {
                    detail.playback = await qualifySourcePlayback({ page, region, observation, contract, candidate, client, abortSignal });
                    const { sessionId, transcodeId, previousTranscodeId, directSourceRevision, previousDirectSourceRevision, positionMs, paused, attempts } = detail.playback;
                    detail.backend = await verifySource({ ...evidence, sessionId, transcodeId, previousTranscodeId, directSourceRevision,
                        previousDirectSourceRevision, positionMs, paused, attempts });
                    if (detail.backend?.passed !== true) throw new Error('Actual source routing did not pass independent verification.');
                    if (detail.backend.capability) detail.outputCapability = await page.evaluate(queryFormatCapability, detail.backend.capability);
                    if (contract.optimized) optimized += 1;
                } else {
                    const alert = region.locator('.error-message[role="alert"]');
                    await expect(alert).toBeVisible();
                    await expect(alert).toHaveText('HDR conversion is not supported on this device yet.');
                    if (observation.records.some((record) => record.path.includes('/transcode/'))) throw new Error('Refused HDR source playback allocated media delivery.');
                    detail.refusal = { status: contract.start.status, code: contract.start.error.code, visibleError: await alert.innerText() };
                    detail.backend = await verifySource({ ...evidence, status: detail.refusal.status, code: detail.refusal.code });
                    if (detail.backend?.passed !== true) throw new Error('The source HDR refusal did not pass independent verification.');
                }
                await closeCase(candidate);
                closed = true;
                await observation.drain();
                if (candidate.dynamicRange === 'sdr' && observation.records.filter((record) => record.path === '/api/v1/playback/stop' && record.status === 200
                    && record.request?.session_id === detail.playback.sessionId).length !== 1) throw new Error('Source Close did not release its exact session once.');
                detail.passed = true;
            } catch (error) {
                caseError = error;
                if (error?.sourceFailure) detail.failure = error.sourceFailure;
                detail.error = String(error?.message || 'Source qualification failed.').replaceAll(ready.token, '[redacted]').slice(0, 1200);
                await page.screenshot({ path: join(directory, `source-${client}-${String(candidate.index).padStart(2, '0')}-failed.png`) }).catch(() => {});
                throw error;
            } finally {
                const failures = [];
                for (const [name, cleanup] of [
                    ['presentation', () => page.evaluate(() => window.sourcePresentation?.detach())],
                    ['audio', () => releaseNativeAudio(page)],
                    ['playback', async () => { if (!closed) await closeCase(candidate); }],
                    ['observation', () => observation.drain()],
                ]) { try { await cleanup(); } catch { failures.push(name); } }
                observation.detach();
                detail.requests = observation.records;
                detail.pacing.observedClientRequests = observation.records.length;
                pacing.evidence.maxObservedClientRequests = Math.max(pacing.evidence.maxObservedClientRequests, observation.records.length);
                detail.cleanup = { passed: failures.length === 0, failures };
                if (failures.length && !caseError) throw new Error('Owned source case cleanup failed.');
            }
        }
        if (optimized < 1) throw new Error('The actual client did not establish any optimized source route.');
        result.passed = true;
    } catch (error) { failure = error; result.error = String(error?.message || 'Source qualification failed.').replaceAll(ready.token, '[redacted]').slice(0, 1200); }
    finally { await writeFile(join(directory, `source-${client}.json`), `${JSON.stringify(result, null, 2)}\n`); }
    if (failure) throw failure;
    return { passed: true, cases: 14, played: 10, refused: 4, client, sourceIds: candidates.map((candidate) => candidate.id), optimized, conservative: 10 - optimized };
}
