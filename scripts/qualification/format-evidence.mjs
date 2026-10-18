// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { validateFormatProcessProof, validateFormatSummaries, validateFormatUnitProof } from './format-proof.mjs';

export async function readFormatEvidence({ ownership, runtime, directory, browser }) {
    async function copy(name, source, limit) {
        const path = join(directory, name);
        await ownership.copyOut(runtime, source, path);
        const info = await lstat(path);
        if (!info.isFile() || info.isSymbolicLink() || info.size < 1 || info.size > limit) throw new Error('Format evidence is not a bounded regular file.');
        return readFile(path, 'utf8');
    }
    const formatUnits = validateFormatUnitProof(await copy('format-unit-tests.log', '/qualification/evidence/format-unit-tests.log', 65536));
    const formatProcessUnits = validateFormatProcessProof(await copy('format-process-tests.log', '/qualification/evidence/format-process-tests.log', 65536));
    const formatCorpus = JSON.parse(await copy('format-corpus.json', '/qualification/format-corpus.json', 1048576));
    return { formatUnits, formatProcessUnits, formatCorpus, formats: validateFormatSummaries(browser, formatCorpus) };
}
