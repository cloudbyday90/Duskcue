/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { promisify } from 'node:util';
import { validateCaptionArtifact, validateCaptionResults } from './web-captions.mjs';
import { REQUIRED_MANAGED_CASES, PRODUCER_CASE } from './cases.mjs';
import { qualificationSources, SOURCE_FILES, SOURCE_ROOTS } from './source.mjs';

const execute = promisify(execFile);

async function artifactFixture() {
    const directory = await mkdtemp(join(tmpdir(), 'duskcue-caption-validator-'));
    const checkout = join(directory, 'checkout');
    const artifact = join(directory, 'artifact');
    await mkdir(checkout);
    await mkdir(artifact);
    const put = async (root, path, bytes) => { await mkdir(dirname(join(root, path)), { recursive: true }); await writeFile(join(root, path), bytes); };
    for (const path of SOURCE_ROOTS) await put(checkout, `${path}/fixture.txt`, `fixture ${path}\n`);
    for (const path of SOURCE_FILES) await put(checkout, path, `fixture ${path}\n`);
    const git = (args) => execute('git', args, { cwd: checkout, maxBuffer: 8192, windowsHide: true });
    await git(['init', '--quiet']);
    await git(['add', '.']);
    const hooks = join(directory, 'empty-hooks');
    await mkdir(hooks);
    await git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', '-c', `core.hooksPath=${hooks}`, 'commit', '--quiet', '-m', 'Fixture inputs']);
    const commit = (await git(['rev-parse', 'HEAD'])).stdout.trim();
    const source = { commit, ...await qualificationSources(checkout) };
    const imageId = `sha256:${'1'.repeat(64)}`;
    const elf = Buffer.alloc(64);
    elf.set([0x7f, 0x45, 0x4c, 0x46, 2, 1]);
    elf.writeUInt16LE(62, 18);
    const artifacts = {};
    for (const [kind, path] of [['libTest', 'build/duskcue-lib-tests'], ['bootstrap', 'build/duskcue-ffmpeg-bootstrap.so'], ['probe', 'build/duskcue-ffmpeg-probe']]) {
        await put(artifact, path, elf);
        artifacts[kind] = { path, sha256: createHash('sha256').update(elf).digest('hex') };
    }
    const cases = [];
    for (const [index, name] of [...REQUIRED_MANAGED_CASES, PRODUCER_CASE].entries()) {
        const log = `logs/${index}.log`;
        await put(artifact, log, `test ${name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;\n`);
        cases.push({ test: name, passed: true, exactTests: 1, exitCode: 0, log, imageId });
    }
    const manifest = '#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI="init.mp4"\n' + [0, 1, 2, 3].map((index) => `#EXTINF:2.000000,\nseg_${String(index).padStart(4, '0')}.m4s\n`).join('') + '#EXT-X-ENDLIST\n';
    const files = [];
    for (const [path, value] of [
        ['producer/source.mkv', 'synthetic fixture bytes'], ['producer/first.srt', 'unselected cue'],
        ['producer/second.srt', '1\n00:00:01,000 --> 00:00:07,000\nDUSKCUE SELECTED CAPTION\n\n'],
        ['producer/caption/manifest.m3u8', manifest], ['producer/caption/init.mp4', 'fixture initialization'],
        ...[0, 1, 2, 3].map((index) => [`producer/caption/seg_${String(index).padStart(4, '0')}.m4s`, `fixture segment ${index}`]),
    ]) {
        const bytes = Buffer.from(value);
        await put(artifact, path, bytes);
        files.push({ path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') });
    }
    const result = { version: 1, kind: 'duskcue-linux-qualification', status: 'passed', source, image: { id: imageId, architecture: 'amd64' }, artifacts, cases, producer: { ...cases.at(-1), directory: 'producer', captionDirectory: 'producer/caption', files }, cleanup: { passed: true } };
    const save = () => put(artifact, 'result.json', JSON.stringify(result));
    await save();
    return { directory, checkout, artifact, commit, result, save };
}

async function cleanupFixture(directory) {
    const target = await realpath(directory);
    const temporary = await realpath(tmpdir());
    if (dirname(target) !== temporary || !basename(target).startsWith('duskcue-caption-validator-')) throw new Error('Fixture cleanup target is outside its created temporary directory.');
    await rm(target, { recursive: true, force: true });
}

const captions = [
    'actual burned captions remain unobscured while transport and each vertical disclosure are visible',
    'short-height disclosures keep the actual burned caption band visible and uncovered',
    'the frozen caption frame remains unobscured beneath Up next and Cancel keeps it visible',
];

function report() {
    const suite = (file, names) => ({ file: `tests/e2e/${file}`, specs: names.map((title) => ({ title, tests: [{ projectName: 'chromium', expectedStatus: 'passed', results: [{ status: 'passed' }] }] })) });
    return { suites: [
        suite('playback-captions.spec.ts', captions),
        suite('playback-progressive.spec.ts', ['zero', 'growth', 'replacement']),
        suite('localization.spec.ts', ['fr home', 'fr search', 'fr preferences', 'ar home', 'ar search', 'ar preferences']),
        suite('playback-localization.spec.ts', ['fr player', 'ar player']),
    ] };
}

test('current complete scopes count executed captions and explicitly report retry qualification', () => {
    const complete = report();
    assert.deepEqual(validateCaptionResults(complete), { captionPassed: 3, captionSkipped: 0, captionRetries: 0, captionFlaky: 0, progressivePassed: 3, localePassed: 8, requiredRetries: 0, requiredFlaky: 0 });
    complete.suites[0].specs[0].tests[0].results.unshift({ status: 'failed' });
    assert.equal(validateCaptionResults(complete).captionRetries, 1);
    assert.equal(validateCaptionResults(complete).captionFlaky, 1);
});

test('default skips, failed results, absent executions and missing required scopes fail closed', () => {
    for (const status of ['skipped', 'failed', 'timedOut', 'interrupted']) {
        const value = report();
        value.suites[0].specs[0].tests[0].results[0].status = status;
        assert.throws(() => validateCaptionResults(value));
    }
    const absent = report();
    absent.suites[0].specs[0].tests[0].results = [];
    assert.throws(() => validateCaptionResults(absent));
    const partial = report();
    partial.suites.splice(1, 1);
    assert.throws(() => validateCaptionResults(partial));
    const duplicated = report();
    duplicated.suites[0].specs[1].title = captions[0];
    assert.throws(() => validateCaptionResults(duplicated));
});

test('nested report suites and Windows report filenames retain the same scope identities', () => {
    const value = report();
    value.suites[0].file = 'tests\\e2e\\playback-captions.spec.ts';
    value.suites[0] = { file: value.suites[0].file, suites: [{ specs: value.suites[0].specs }] };
    assert.equal(validateCaptionResults(value).captionPassed, 3);
    value.suites[0].suites[0].specs[0].tests[0].projectName = 'webkit';
    assert.throws(() => validateCaptionResults(value));
});

test('a fresh provenance envelope relocates validated caption assets without executing its fixture binaries', async () => {
    const fixture = await artifactFixture();
    try {
        const result = await validateCaptionArtifact(fixture.artifact, fixture.checkout, fixture.commit);
        assert.equal(result.assets, 9);
        assert.equal(result.commit, fixture.commit);
        assert.equal(result.directory, join(await realpath(fixture.artifact), 'producer/caption'));
        await assert.rejects(() => validateCaptionArtifact(fixture.artifact, fixture.checkout, '2'.repeat(40)), /different source commit/);
        fixture.result.cases[0].passed = false;
        await fixture.save();
        await assert.rejects(() => validateCaptionArtifact(fixture.artifact, fixture.checkout, fixture.commit), /case failed/);
        fixture.result.cases[0].passed = true;
        fixture.result.cleanup.passed = false;
        await fixture.save();
        await assert.rejects(() => validateCaptionArtifact(fixture.artifact, fixture.checkout, fixture.commit), /cleanup must both pass/);
    } finally { await cleanupFixture(fixture.directory); }
});

test('tampered assets, unsafe paths and changed source cannot inherit successful producer flags', async () => {
    const fixture = await artifactFixture();
    try {
        const sourcePath = join(fixture.checkout, SOURCE_FILES[0]);
        const source = await readFile(sourcePath);
        await writeFile(sourcePath, Buffer.concat([source, Buffer.from('changed\n')]));
        await assert.rejects(() => validateCaptionArtifact(fixture.artifact, fixture.checkout, fixture.commit), /unchanged committed/);
        await writeFile(sourcePath, source);
        await writeFile(join(fixture.artifact, 'producer/caption/init.mp4'), 'tampered');
        await assert.rejects(() => validateCaptionArtifact(fixture.artifact, fixture.checkout, fixture.commit), /identity is invalid|SHA256 mismatch/);
        fixture.result.producer.files[0].path = '../outside.mkv';
        await fixture.save();
        await assert.rejects(() => validateCaptionArtifact(fixture.artifact, fixture.checkout, fixture.commit), /safe relative/);
    } finally { await cleanupFixture(fixture.directory); }
});
