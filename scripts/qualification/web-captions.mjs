/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { appendFile, lstat, open, realpath } from 'node:fs/promises';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const execute = promisify(execFile);
const workspace = fileURLToPath(new URL('../../', import.meta.url));
const sha256 = /^[a-f0-9]{64}$/;
const captionTitles = [
    'actual burned captions remain unobscured while transport and each vertical disclosure are visible',
    'short-height disclosures keep the actual burned caption band visible and uncovered',
    'the frozen caption frame remains unobscured beneath Up next and Cancel keeps it visible',
];

function artifactPath(value) {
    if (typeof value !== 'string' || !value || isAbsolute(value) || /[\\\u0000-\u001f\u007f]/u.test(value)
        || value.split('/').some((part) => !part || part === '.' || part === '..')) throw new Error('Qualification artifact path must be a safe relative Unix path.');
    return value;
}

async function boundedFile(root, name, limit, expected = null, retain = false) {
    const parts = artifactPath(name).split('/');
    for (let index = 1; index < parts.length; index += 1) {
        const parent = await lstat(join(root, ...parts.slice(0, index)));
        if (!parent.isDirectory() || parent.isSymbolicLink()) throw new Error(`Qualification artifact parent is not a regular directory: ${name}`);
    }
    const path = join(root, ...parts);
    const before = await lstat(path);
    if (!before.isFile() || before.isSymbolicLink() || !Number.isSafeInteger(before.size) || before.size < 1 || before.size > limit) throw new Error(`Qualification artifact exceeds its regular-file bound: ${name}`);
    if (expected && (!sha256.test(expected.sha256) || expected.bytes !== undefined && expected.bytes !== before.size)) throw new Error(`Qualification artifact identity is invalid: ${name}`);
    const file = await open(path, 'r');
    const hash = createHash('sha256');
    const chunks = [];
    let prefix = null;
    try {
        const metadata = await file.stat();
        if (metadata.dev !== before.dev || metadata.ino !== before.ino || metadata.size !== before.size) throw new Error(`Qualification artifact changed before reading: ${name}`);
        let offset = 0;
        while (offset < before.size) {
            const buffer = Buffer.allocUnsafe(Math.min(64 * 1024, before.size - offset));
            const { bytesRead } = await file.read(buffer, 0, buffer.length, offset);
            if (!bytesRead) throw new Error(`Qualification artifact truncated while reading: ${name}`);
            const bytes = buffer.subarray(0, bytesRead);
            if (!prefix) prefix = Buffer.from(bytes.subarray(0, 64));
            hash.update(bytes);
            if (retain) chunks.push(bytes);
            offset += bytesRead;
        }
        if ((await file.read(Buffer.allocUnsafe(1), 0, 1, before.size)).bytesRead || (await file.stat()).size !== before.size) throw new Error(`Qualification artifact changed while reading: ${name}`);
    } finally { await file.close(); }
    const digest = hash.digest('hex');
    if (expected && digest !== expected.sha256) throw new Error(`Qualification artifact SHA256 mismatch: ${name}`);
    return { path, bytes: before.size, sha256: digest, prefix, content: retain ? Buffer.concat(chunks) : null };
}

function exactPassingCase(record, imageId) {
    if (!record || record.passed !== true || record.exactTests !== 1 || record.exitCode !== 0 || record.imageId !== imageId) throw new Error('Required Linux qualification case did not execute exactly once successfully in the owned image.');
}

export async function validateCaptionArtifact(directory, checkout = workspace, expectedCommit = process.env.GITHUB_SHA) {
    const rootMetadata = await lstat(directory);
    if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) throw new Error('Caption qualification root must be a regular directory.');
    const root = await realpath(directory);
    const bytes = await boundedFile(root, 'result.json', 4 * 1024 * 1024, null, true);
    const result = JSON.parse(bytes.content.toString('utf8'));
    if (result.version !== 1 || result.kind !== 'duskcue-linux-qualification' || result.status !== 'passed' || result.cleanup?.passed !== true) throw new Error('Fresh Linux qualification and owned cleanup must both pass before caption consumption.');
    const { REQUIRED_MANAGED_CASES, REQUIRED_UNIT_CASES, PRODUCER_CASE } = await import('./cases.mjs');
    const { STOP_CASE } = await import('./runtime.mjs');
    const { validateProgressiveHttpEvidence } = await import('./progressive-http.mjs');
    const { qualificationSources } = await import('./source.mjs');
    const commit = (await execute('git', ['rev-parse', 'HEAD'], { cwd: checkout, maxBuffer: 8192, encoding: 'utf8', windowsHide: true })).stdout.trim();
    if (result.source?.commit !== commit || expectedCommit && expectedCommit !== commit) throw new Error('Caption producer belongs to a different source commit.');
    const current = await qualificationSources(checkout);
    const tracked = new Set((await execute('git', ['ls-files', '--cached', '-z'], { cwd: checkout, maxBuffer: 4 * 1024 * 1024, encoding: 'utf8', windowsHide: true })).stdout.split('\u0000').filter(Boolean));
    const status = (await execute('git', ['status', '--porcelain=v1', '--untracked-files=no'], { cwd: checkout, maxBuffer: 4 * 1024 * 1024, encoding: 'utf8', windowsHide: true })).stdout.trim();
    if (status || current.files.some((record) => !tracked.has(record.path))) throw new Error('Caption qualification requires unchanged committed source inputs in the fresh web checkout.');
    assert.deepEqual(result.source.files, current.files, 'Caption qualification source inventory does not match this complete checkout.');
    assert.equal(result.source.sha256, current.sha256, 'Caption qualification source inventory hash differs.');
    if (result.image?.architecture !== 'amd64' || !/^sha256:[a-f0-9]{64}$/.test(result.image?.id || '')) throw new Error('Caption qualification requires the recorded immutable amd64 Linux image.');
    const imageId = result.image.id;
    if (!Array.isArray(result.cases) || result.cases.length > 128 || result.cases.some((record) => record.passed !== true || record.exitCode !== 0 || record.imageId !== imageId)) throw new Error('A Linux case failed or belongs to another image.');
    for (const name of [...REQUIRED_MANAGED_CASES, ...REQUIRED_UNIT_CASES, PRODUCER_CASE]) {
        const records = result.cases.filter((record) => record.test === name);
        if (records.length !== 1) throw new Error(`Required exact Linux qualification case is missing or duplicated: ${name}`);
        exactPassingCase(records[0], imageId);
        await boundedFile(root, records[0].log, 16 * 1024 * 1024);
    }
    exactPassingCase(result.sql, imageId);
    if (result.sql.test !== STOP_CASE) throw new Error('Current exact progressive SQL qualification is required.');
    validateProgressiveHttpEvidence(result.sql.progressiveHttp, result.resourceId);
    await boundedFile(root, result.sql.log, 16 * 1024 * 1024);
    for (const [kind, path, limit] of [['libTest', 'build/duskcue-lib-tests', 2 * 1024 ** 3], ['stopContract', 'build/playback-stop-contract', 2 * 1024 ** 3], ['bootstrap', 'build/duskcue-ffmpeg-bootstrap.so', 16 * 1024 ** 2], ['probe', 'build/duskcue-ffmpeg-probe', 16 * 1024 ** 2]]) {
        const recorded = result.artifacts?.[kind];
        if (recorded?.path !== path) throw new Error(`Missing paired qualification binary: ${kind}`);
        const artifact = await boundedFile(root, path, limit, recorded);
        if (artifact.prefix.length < 20 || artifact.prefix.subarray(0, 4).toString('hex') !== '7f454c46' || artifact.prefix[4] !== 2 || artifact.prefix[5] !== 1 || artifact.prefix.readUInt16LE(18) !== 62) throw new Error(`Qualification binary is not amd64 ELF: ${kind}`);
    }
    const producer = result.producer;
    exactPassingCase(producer, imageId);
    if (producer.test !== PRODUCER_CASE || producer.directory !== 'producer' || producer.captionDirectory !== 'producer/caption' || !Array.isArray(producer.files) || producer.files.length > 32) throw new Error('Fresh exact production-argument caption output is required.');
    const producerCase = result.cases.find((record) => record.test === PRODUCER_CASE);
    if (producer.log !== producerCase.log) throw new Error('Caption producer log does not match its exact passing Linux case.');
    const records = new Map();
    for (const record of producer.files) {
        const path = artifactPath(record.path);
        if (records.has(path) || !Number.isSafeInteger(record.bytes) || record.bytes < 1) throw new Error('Duplicate or invalid caption producer asset identity.');
        records.set(path, record);
    }
    if (!records.has('producer/caption/manifest.m3u8')) throw new Error('Caption producer playlist identity is missing.');
    const manifest = await boundedFile(root, 'producer/caption/manifest.m3u8', 64 * 1024, records.get('producer/caption/manifest.m3u8'), true);
    const text = manifest.content.toString('utf8');
    const segments = text.split(/\r?\n/).filter((line) => /^seg_\d+\.m4s$/.test(line));
    if (segments.length !== 4 || new Set(segments).size !== 4 || !text.includes('#EXT-X-ENDLIST') || !text.includes('#EXT-X-MAP:URI="init.mp4"')) throw new Error('A complete four-segment production-argument caption playlist is required.');
    const expected = new Map([
        ['producer/source.mkv', 16 * 1024 * 1024], ['producer/first.srt', 4 * 1024], ['producer/second.srt', 4 * 1024],
        ['producer/caption/manifest.m3u8', 64 * 1024], ['producer/caption/init.mp4', 1024 * 1024],
        ...segments.map((name) => [`producer/caption/${name}`, 4 * 1024 * 1024]),
    ]);
    for (const [path, limit] of expected) {
        if (!records.has(path)) throw new Error(`Caption producer asset is missing: ${path}`);
        const asset = path === 'producer/caption/manifest.m3u8' ? manifest : await boundedFile(root, path, limit, records.get(path), path.endsWith('.srt'));
        if (path === 'producer/second.srt' && asset.content.toString('utf8').replaceAll('\r', '').trim() !== '1\n00:00:01,000 --> 00:00:07,000\nDUSKCUE SELECTED CAPTION') throw new Error('Selected caption cue differs from the production-argument producer.');
    }
    if (records.size !== expected.size) throw new Error('Caption producer asset manifest contains unexpected files.');
    return { directory: join(root, 'producer/caption'), commit, imageId, producer: PRODUCER_CASE, manifestSha256: manifest.sha256, assets: expected.size };
}

export function validateCaptionResults(report) {
    if (!report || !Array.isArray(report.suites)) throw new Error('A completed Playwright JSON report is required.');
    const cases = [];
    const visit = (suites, parentFile = '') => {
        for (const suite of suites) {
            const file = suite.file || parentFile;
            for (const spec of suite.specs || []) {
                const name = (spec.file || file).split(/[/\\]/).at(-1);
                for (const record of spec.tests || []) cases.push({ ...record, title: spec.title, file: name });
            }
            visit(suite.suites || [], file);
        }
    };
    visit(report.suites);
    const captions = cases.filter((record) => record.file === 'playback-captions.spec.ts');
    if (captions.length !== 3 || new Set(captions.map((record) => record.title)).size !== 3 || captionTitles.some((title) => !captions.some((record) => record.title === title))) throw new Error('Exactly the three required actual-caption cases must be present in the complete browser report.');
    const scopes = { 'playback-captions.spec.ts': 3, 'playback-progressive.spec.ts': 3, 'playback-tab-background.spec.ts': 1, 'profile-remembering.spec.ts': 1, 'localization.spec.ts': 6, 'playback-localization.spec.ts': 2 };
    for (const [file, count] of Object.entries(scopes)) {
        if (cases.filter((record) => record.file === file).length !== count) throw new Error(`The complete current ${file} scope must execute (${count} cases).`);
    }
    const required = cases.filter((record) => Object.hasOwn(scopes, record.file));
    for (const record of required) {
        if (record.projectName !== 'chromium' || record.expectedStatus !== 'passed' || !Array.isArray(record.results) || !record.results.length || record.results.some((result) => result.status === 'skipped') || record.results.at(-1).status !== 'passed') throw new Error(`Required browser case was skipped, failed or never executed: ${record.title}`);
    }
    return {
        captionPassed: captions.length, captionSkipped: 0,
        captionRetries: captions.reduce((count, record) => count + record.results.length - 1, 0),
        captionFlaky: captions.filter((record) => record.results.slice(0, -1).some((result) => result.status !== 'passed')).length,
        progressivePassed: scopes['playback-progressive.spec.ts'], backgroundPassed: scopes['playback-tab-background.spec.ts'], rememberingPassed: scopes['profile-remembering.spec.ts'], localePassed: scopes['localization.spec.ts'] + scopes['playback-localization.spec.ts'],
        requiredRetries: required.reduce((count, record) => count + record.results.length - 1, 0),
        requiredFlaky: required.filter((record) => record.results.slice(0, -1).some((result) => result.status !== 'passed')).length,
    };
}

async function main() {
    const [mode, path] = process.argv.slice(2);
    if (mode === 'artifact' && path) {
        const result = await validateCaptionArtifact(path);
        if (/[\r\n]/u.test(result.directory)) throw new Error('Caption environment path contains a newline.');
        if (process.env.GITHUB_ENV) await appendFile(process.env.GITHUB_ENV, `DUSKCUE_TEST_CAPTION_HLS_DIR=${result.directory}\n`, 'utf8');
        process.stdout.write(`${JSON.stringify(result)}\n`);
    } else if (mode === 'results' && path) {
        const artifact = await boundedFile(dirname(resolve(path)), join('.', path.split(/[/\\]/).at(-1)).replaceAll('\\', '/'), 16 * 1024 * 1024, null, true);
        process.stdout.write(`${JSON.stringify(validateCaptionResults(JSON.parse(artifact.content.toString('utf8'))))}\n`);
    } else throw new Error('Use web-captions.mjs artifact <downloaded-artifact-root> or results <Playwright-JSON>.');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
