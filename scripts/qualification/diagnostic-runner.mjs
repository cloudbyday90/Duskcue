// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { lstat } from 'node:fs/promises';
import { join } from 'node:path';
import { hashFile } from './source.mjs';
import { traceSpecification } from './diagnostic.mjs';

export async function retainKnownDiagnostic(ownership, imageId, environment, directory, sourceSha256, selector, sourceClip) {
    const specification = traceSpecification(selector);
    const container = await ownership.create(imageId, ['case-trace', selector], { environment });
    const log = `logs/${specification.name}-trace-driver.log`;
    let diagnostic;
    try {
        if (specification.sourceRequired) await ownership.copyIn(container, sourceClip, '/fixtures/source.mkv');
        const execution = await ownership.start(container, { log: join(directory, log), timeoutMs: 120000, bytes: 1048576 });
        const markers = execution.stdout.split(/\r?\n/).filter((line) => line.startsWith('DUSKCUE_CASE_DIAGNOSTIC='));
        if (markers.length !== 1) throw new Error('Expected exactly one qualified case diagnostic.');
        const outcome = JSON.parse(markers[0].slice('DUSKCUE_CASE_DIAGNOSTIC='.length));
        if (outcome.test !== selector || outcome.countsTowardQualification !== false || outcome.sourceCommit !== environment.DUSKCUE_SOURCE_COMMIT || outcome.sourceSha256 !== sourceSha256) throw new Error('Case diagnostic provenance mismatched.');
        diagnostic = { ...outcome, containerExitCode: execution.code, imageId, log };
    } catch (error) { diagnostic = { completed: false, countsTowardQualification: false, imageId, log, test: selector, error: error.message }; }
    finally {
        try {
            const path = `logs/${specification.name}-strace.log`;
            const retained = join(directory, path);
            await ownership.copyOut(container, specification.trace, retained, { requireSuccess: false });
            const meta = await lstat(retained);
            if (!meta.isFile() || meta.isSymbolicLink() || meta.size > 16777216) throw new Error('Case trace is not a bounded regular log.');
            diagnostic.trace = path;
            diagnostic.traceRetained = true;
            diagnostic.traceBytes = meta.size;
            diagnostic.traceSha256 = await hashFile(retained);
        } catch { diagnostic.traceRetained = false; }
        await ownership.remove(container);
    }
    return diagnostic;
}
