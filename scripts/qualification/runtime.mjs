// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { runBounded } from './commands.mjs';
import { verifyArtifacts, RUNTIME_ARTIFACTS } from './attest.mjs';
import { REQUIRED_MANAGED_CASES, REQUIRED_UNIT_PREFIXES, PRODUCER_CASE } from './cases.mjs';
import { traceQualifiedCase, traceSpecification } from './diagnostic.mjs';

export const STOP_CASE = 'stop_is_idempotent_serializes_heartbeat_seek_and_retains_original_profile';

export function exactTestListed(output, selector) {
    return output.split(/\r?\n/).includes(`${selector}: test`) && /^1 test, 0 benchmarks\s*$/m.test(output);
}

export function exactPassed(output) {
    return /test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;/.test(output);
}

async function main() {
    const [mode, selector, ...extra] = process.argv.slice(2);
    if (extra.length || !['case', 'inventory', 'sql', 'case-trace'].includes(mode) || mode !== 'inventory' && !/^[A-Za-z0-9_:]+$/.test(selector || '')) throw new Error('Choose one exact qualification test or inventory.');
    const manifest = await verifyArtifacts();
    if (manifest.source.commit !== process.env.DUSKCUE_SOURCE_COMMIT || !/^[a-f0-9-]{36}$/.test(process.env.DUSKCUE_TEST_RESOURCE_ID || '')) throw new Error('The runtime is not owned by this exact source workflow.');
    const executable = mode === 'sql' ? RUNTIME_ARTIFACTS.stopContract : RUNTIME_ARTIFACTS.libTest;
    if (mode === 'case-trace') traceSpecification(selector);
    if (mode === 'inventory') {
        const inventory = await runBounded(executable, ['--list'], { bytes: 1048576, captureBytes: 1048576, timeoutMs: 30000 });
        if (inventory.code !== 0 || inventory.signal) throw new Error('Current test inventory could not be read.');
        const selected = inventory.stdout.split(/\r?\n/).filter((line) => line.endsWith(': test')).map((line) => line.slice(0, -6))
            .filter((test) => REQUIRED_UNIT_PREFIXES.some((prefix) => test.startsWith(prefix)) || REQUIRED_MANAGED_CASES.includes(test) || test === PRODUCER_CASE);
        console.log(`DUSKCUE_TEST_INVENTORY=${JSON.stringify(selected)}`);
        return;
    }
    const allowed = mode === 'sql' ? selector === STOP_CASE : REQUIRED_MANAGED_CASES.includes(selector) || selector === PRODUCER_CASE || REQUIRED_UNIT_PREFIXES.some((prefix) => selector.startsWith(prefix));
    if (!allowed) throw new Error('The requested test is outside this qualification.');
    const listed = await runBounded(executable, [selector, '--exact', '--list'], { bytes: 1048576, timeoutMs: 30000 });
    if (listed.code !== 0 || listed.signal || !exactTestListed(listed.stdout, selector)) throw new Error('The exact current test is absent; zero tests cannot pass.');
    if (mode === 'case-trace') {
        const diagnostic = await traceQualifiedCase(executable, selector);
        console.log(`DUSKCUE_CASE_DIAGNOSTIC=${JSON.stringify({ ...diagnostic, test: selector, sourceCommit: manifest.source.commit, sourceSha256: manifest.source.sha256, artifacts: manifest.artifacts })}`);
        if (!diagnostic.completed) process.exitCode = 1;
        return;
    }
    const result = await runBounded(executable, [selector, '--exact', '--include-ignored', '--nocapture'], { bytes: 16777216, timeoutMs: 300000 });
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
    const passed = result.code === 0 && !result.signal && exactPassed(result.stdout);
    console.log(`DUSKCUE_RUNTIME_RESULT=${JSON.stringify({ version: 1, test: selector, passed, exactTests: 1, exitCode: result.code, sourceCommit: manifest.source.commit, sourceSha256: manifest.source.sha256, artifacts: manifest.artifacts })}`);
    if (!passed) process.exitCode = 1;
}

if (process.argv[1]?.endsWith('/runtime.mjs')) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
