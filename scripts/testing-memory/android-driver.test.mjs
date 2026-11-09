// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { runBounded } from '../qualification/commands.mjs';
import { androidStageBudget, androidLogFrames } from './android-stage-budget.mjs';
import { androidVmUri, androidPid, androidProcessIdentity, androidPausedIsolate, androidFlutterMessage, androidPackage, androidCommandIdentity } from './android-vm-identity.mjs';
import { androidVmRead } from './android-vm-probe.mjs';
import { ownAndroidAppLogs } from './android-app-logs.mjs';
import { bootstrapAndroidDriver } from './android-driver-bootstrap.mjs';
import { androidDriverInputs } from './android-driver-inputs.mjs';
import { redactAndroidOutput } from './android-evidence.mjs';
import { androidConsolePorts, androidAdbEnvironment } from './android-adb-policy.mjs';

async function temporary(action) {
    const directory = await mkdtemp(join(tmpdir(), 'duskcue-driver-'));
    try { await action(directory); }
    finally {
        const actual = await realpath(directory);
        assert.equal(dirname(actual), await realpath(tmpdir()));
        assert.ok(basename(actual).startsWith('duskcue-driver-'));
        await rm(actual, { recursive: true });
    }
}

const vm = { type: 'VM', isolates: [{ type: '@Isolate', name: 'main', id: 'isolates/123' }] };
const isolate = { type: 'Isolate', id: 'isolates/123', name: 'main', runnable: true, pauseEvent: { kind: 'PauseStart' } };
const stat = (start = '456', state = 'S') => `123 (e.qualification) ${state} ${Array(18).fill('0').join(' ')} ${start} 0 0`;

test('owned discovery avoids default emulator scanning and confines changes to the child environment', () => {
    const inherited = { ADB_SERVER_SOCKET: 'foreign-socket', ANDROID_SERIAL: 'foreign-device', ADB_LOCAL_TRANSPORT_MAX_PORT: '65535' };
    for (const port of androidConsolePorts) {
        assert.ok(port > 5585 && port <= 5680 && port % 2 === 0);
        const owned = androidAdbEnvironment(inherited, 45000, port);
        assert.equal(owned.ADB_LOCAL_TRANSPORT_MAX_PORT, '0');
        assert.equal(owned.ADB_SERVER_SOCKET, 'tcp:127.0.0.1:45000');
        assert.equal(owned.ANDROID_ADB_SERVER_PORT, '45000');
        assert.equal(owned.ANDROID_SERIAL, `emulator-${port}`);
    }
    assert.equal(inherited.ADB_SERVER_SOCKET, 'foreign-socket');
    for (const [adb, console] of [[5037, 5580], [0, 5620], [65536, 5620], [5621, 5620], [45000, 5621]]) assert.throws(() => androidAdbEnvironment(inherited, adb, console));
});

test('PID/start/paused-isolate and capability evidence reject substitution and malformed identities', () => {
    assert.equal(androidPid('123'), 123);
    assert.doesNotThrow(() => androidCommandIdentity(`${androidPackage}\0`));
    assert.doesNotThrow(() => androidCommandIdentity(`${androidPackage}${'\0'.repeat(128)}`));
    for (const command of [androidPackage, `${androidPackage}\0other`, `${androidPackage}-other\0`, `${androidPackage}\0\0argument\0`, `${androidPackage}${'\0'.repeat(4096)}`]) assert.throws(() => androidCommandIdentity(command));
    assert.deepEqual(androidProcessIdentity(stat(), 123), { pid: 123, started: '456' });
    assert.equal(androidPausedIsolate(vm, isolate), 'isolates/123');
    assert.equal(androidPausedIsolate({ ...vm, isolates: [] }), null);
    assert.equal(androidPausedIsolate(vm, { ...isolate, runnable: false, pauseEvent: { kind: 'None' } }), null);
    assert.equal(androidPausedIsolate(vm, { ...isolate, runnable: true, pauseEvent: { kind: 'None' } }), null);
    assert.equal(androidFlutterMessage('I/flutter (  123): ANDROID_PHASE background', 123), 'ANDROID_PHASE background');
    assert.equal(androidVmUri('The Dart VM service is listening on http://127.0.0.1:12345/abcdefghijk=/', 12345), 'http://127.0.0.1:12345/abcdefghijk=/');
    for (const text of ['', '123 124', '0123', '123\n124', '-1', '123;anything']) assert.throws(() => androidPid(text));
    for (const [text, pid] of [[stat(), 124], [stat('0'), 123], [stat('456', 'Z'), 123], ['123 (x) S 0', 123]]) assert.throws(() => androidProcessIdentity(text, pid));
    assert.throws(() => androidFlutterMessage('I/flutter (124): ANDROID_PHASE background', 123));
    assert.throws(() => androidPausedIsolate({ ...vm, isolates: [...vm.isolates, ...vm.isolates] }));
    assert.throws(() => androidPausedIsolate(vm, { ...isolate, pauseEvent: { kind: 'Resume' } }));
    for (const uri of ['http://localhost:12345/abcdefghijk=/', 'http://127.0.0.1:12346/abcdefghijk=/',
        'http://127.0.0.1:12345/abcdefghijk=/?key=secret', 'http://user@127.0.0.1:12345/abcdefghijk=/', 'http://127.0.0.1:12345/']) {
        assert.throws(() => androidVmUri(`The Dart VM service is listening on ${uri}`, 12345));
    }
});

test('evidence redacts truncated capabilities and never requires their closing delimiter', () => {
    for (const suffix of ['', 'a', 'abcdefghijk=', 'abcdefghijk=/ws']) {
        assert.equal(redactAndroidOutput(`prefix http://127.0.0.1:12345/${suffix}`), 'prefix [redacted-local-capability-url]');
    }
});

test('framing handles split Unicode and rejects oversized, invalid and incomplete app logs', () => {
    const lines = [], frames = androidLogFrames((line) => lines.push(line), 16);
    const bytes = Buffer.from('a€\r\nb\n');
    frames.write(bytes.subarray(0, 2)); frames.write(bytes.subarray(2)); frames.finish();
    assert.deepEqual(lines, ['a€', 'b']);
    assert.throws(() => androidLogFrames(() => {}, 2).write(Buffer.from('abc')));
    assert.throws(() => androidLogFrames(() => {}).write(Buffer.from([255, 10])));
    const partial = androidLogFrames(() => {}); partial.write(Buffer.from('partial')); assert.throws(() => partial.finish());
});

test('shared output overflow cancels an actual blocked child and observes process/stream/file completion', async () => temporary(async (directory) => {
    const budget = androidStageBudget(Date.now() + 10000, { bytes: 24 });
    try {
        await assert.rejects(runBounded(process.execPath, ['-e', "process.stdout.write('x'.repeat(32)); setInterval(()=>{},1000)"], {
            timeoutMs: 5000, log: join(directory, 'overflow.log'), outputBudget: budget, abortSignal: budget.signal,
        }));
        assert.equal(budget.signal.aborted, true);
        assert.equal(budget.bytes, 32);
        assert.match(budget.failure.message, /shared output/);
        assert.equal((await readFile(join(directory, 'overflow.log'))).length, 0);
    } finally { budget.close(); }
}));

test('the shared suite deadline cancels a real driver process without admitting another attempt', async () => {
    const now = Date.now();
    const budget = androidStageBudget(now + 200);
    assert.equal(budget.deadline, now + 200);
    assert.equal(budget.bootstrapDeadline, now + 200);
    try {
        await assert.rejects(runBounded(process.execPath, ['-e', 'setInterval(()=>{},1000)'], {
            abortSignal: budget.signal, timeoutMs: 5000, outputBudget: budget,
        }));
        assert.match(budget.failure.message, /deadline/);
        assert.equal(budget.signal.aborted, true);
    } finally { budget.close(); }
    assert.throws(() => androidStageBudget(Date.now() - 1));
});

test('retained app producer attributes real phases, redacts split secrets, and closes only after the child', async () => temporary(async (directory) => {
    const budget = androidStageBudget(Date.now() + 10000);
    const phases = [];
    const payload = 'I/flutter (123): The Dart VM service is listening on http://127.0.0.1:12345/abcdefghijk=/\nI/flutter (123): private-token\nI/flutter (123): ANDROID_PHASE background\n';
    const source = `process.stdout.write(${JSON.stringify(payload.slice(0, 55))}); setTimeout(()=>process.stdout.write(${JSON.stringify(payload.slice(55))}),20); setInterval(()=>{},1000)`;
    let args;
    const logs = ownAndroidAppLogs({ adb: 'owned-adb', adbPort: 23456, serial: 'emulator-5580', env: process.env }, {
        pid: 123, port: 12345, directory, fixture: { token: 'private-token', scenario: 'baseline' }, budget,
        onPhase: (phase) => phases.push(phase),
    }, (executable, actual, options) => { assert.equal(executable, 'owned-adb'); args = actual; return runBounded(process.execPath, ['-e', source], options); });
    try {
        assert.equal(await logs.ready, 'http://127.0.0.1:12345/abcdefghijk=/');
        await logs.close();
        assert.deepEqual(phases, ['ANDROID_PHASE background']);
        assert.deepEqual(args.slice(0, 4), ['-P', '23456', '-s', 'emulator-5580']);
        assert.ok(args.includes('--pid=123'));
        const evidence = await readFile(join(directory, 'app.log'), 'utf8');
        assert.ok(!evidence.includes('private-token') && !evidence.includes('abcdefghijk='));
        assert.ok(evidence.includes('[redacted-local-capability-url]'));
    } finally { await logs.close(); budget.close(); }
}));

test('producer failure and duplicate/wrong-stage phases cancel observation without a driver retry', async () => {
    for (const payload of ['I/flutter (123): ANDROID_PHASE background\nI/flutter (123): ANDROID_PHASE background\n',
        'I/flutter (123): ANDROID_PHASE renew-loss\n', 'I/flutter (123): ANDROID_PHASE ambient-background\n', 'I/flutter (124): no\n', 'incomplete']) {
        await temporary(async (directory) => {
            const budget = androidStageBudget(Date.now() + 10000);
            const logs = ownAndroidAppLogs({ env: process.env }, { pid: 123, port: 12345, directory, fixture: { scenario: 'baseline' }, budget, onPhase() {} },
                (_, __, options) => runBounded(process.execPath, ['-e', `process.stdout.write(${JSON.stringify(payload)})`], options));
            try { await assert.rejects(logs.completed); assert.equal(budget.signal.aborted, true); await assert.rejects(logs.ready); }
            finally { await logs.close().catch(() => {}); budget.close(); }
        });
    }
});

test('read-only VM probes enforce coding, redirects, size, shared budget, cancellation and completed socket ownership', async () => {
    let mode = 'good', requests = 0;
    const sockets = new Set();
    const server = createServer((req, res) => {
        requests++; assert.match(req.url, /^\/abcdefghijk=\/get(?:VM|Isolate)/); assert.equal(req.headers['accept-encoding'], 'identity');
        if (mode === 'stall') return;
        if (mode === 'redirect') { res.writeHead(302, { Location: '/elsewhere' }); res.end(); return; }
        res.writeHead(200, { 'Content-Type': 'application/json', ...(mode === 'coding' ? { 'Content-Encoding': 'gzip' } : {}) });
        if (mode === 'oversize') res.end('x'.repeat(1048577));
        else if (mode === 'invalid') res.end(Buffer.from([255]));
        else if (mode === 'partial') { res.write('partial'); res.destroy(); }
        else res.end(JSON.stringify({ jsonrpc: '2.0', id: '', result: vm }));
    });
    server.on('connection', (socket) => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const uri = `http://127.0.0.1:${server.address().port}/abcdefghijk=/`;
    try {
        for (const selected of ['good', 'redirect', 'coding', 'oversize', 'invalid', 'partial', 'stall', 'budget']) {
            mode = selected;
            const budget = androidStageBudget(Date.now() + 10000, { bytes: selected === 'budget' ? 1 : 16777216 });
            const timer = selected === 'stall' ? setTimeout(() => budget.fail(new Error('cancelled fixture')), 50) : null;
            try {
                if (selected === 'good') assert.deepEqual(await androidVmRead(uri, 'getVM', {}, budget), vm);
                else await assert.rejects(androidVmRead(uri, 'getVM', {}, budget));
            } finally { clearTimeout(timer); budget.close(); }
        }
        assert.equal(requests, 8);
        const budget = androidStageBudget(Date.now() + 10000);
        try { await assert.rejects(androidVmRead(uri, 'resume', {}, budget)); assert.equal(requests, 8); }
        finally { budget.close(); }
    } finally { for (const socket of sockets) socket.destroy(); await new Promise((resolve) => server.close(resolve)); }
});

test('paused bootstrap rechecks PID/start/boot/forward identity and never resumes or replays app work', async () => {
    for (const failure of [null, 'pid', 'start', 'boot', 'forward', 'probe', 'uncertain-forward']) await temporary(async (directory) => {
        const budget = androidStageBudget(Date.now() + 10000);
        const calls = []; let forwarded = false, launched = false, probes = 0, logsClosed = 0, pidReads = 0;
        const owner = { serial: 'emulator-5580', bootId: 'owned-boot', async command(args) {
            calls.push(args);
            const key = args.join(' ');
            if (key === `shell pidof ${androidPackage} || true`) return launched && ++pidReads > 1 ? '123' : '';
            if (key === 'forward --list') return forwarded ? `${failure === 'forward' && probes ? 'emulator-5582' : owner.serial} tcp:12345 tcp:12345` : '';
            if (args[0] === 'forward') {
                forwarded = args[1] !== '--remove';
                if (failure === 'uncertain-forward' && args[1] === '--no-rebind') throw new Error('Accepted forward lost response');
                return '';
            }
            if (key.includes('boot_id')) return failure === 'boot' && probes ? 'new-boot' : owner.bootId;
            if (key === `shell pidof ${androidPackage}`) return failure === 'pid' && probes ? '124' : '123';
            if (key.endsWith('/cmdline')) return `${androidPackage}${'\0'.repeat(128)}`;
            if (key.endsWith('/stat')) return stat(failure === 'start' && probes ? '999' : '456');
            if (key.startsWith('shell am start')) { assert.equal(launched, false); launched = true; }
            return '';
        } };
        const dependencies = {
            ownLogs: () => ({ ready: Promise.resolve('http://127.0.0.1:12345/abcdefghijk=/'), async close() { logsClosed++; } }),
            readVm: async (_, method) => {
                probes++; if (failure === 'probe') throw new Error('probe fault');
                return method === 'getVM' ? vm : probes === 2 ? { ...isolate, runnable: true, pauseEvent: { kind: 'None' } } : isolate;
            },
            isListening: async () => false,
        };
        try {
            const operation = bootstrapAndroidDriver(owner, { fixture: {}, directory, port: 12345, budget, onPhase() {} }, dependencies);
            if (failure) await assert.rejects(operation);
            else { const result = await operation; await result.close(); assert.equal(logsClosed, 1); }
            assert.equal(calls.filter((args) => args.join(' ').startsWith('shell am start')).length, failure === 'uncertain-forward' ? 0 : 1);
            if (failure !== 'uncertain-forward') assert.ok(calls.some((args) => args.includes('start-paused')));
            assert.ok(!calls.some((args) => args.includes('resume') || args.includes('install')));
            if (failure !== 'forward') assert.equal(forwarded, false);
            else assert.ok(!calls.some((args) => args[1] === '--remove'));
        } finally { budget.close(); }
    });
});

test('host driver inventory freezes SDK package mappings and the transitive source graph', async () => temporary(async (workspace) => {
    const sdk = join(workspace, 'sdk'), mobile = join(workspace, 'clients/mobile');
    await mkdir(join(mobile, '.dart_tool'), { recursive: true }); await mkdir(join(mobile, 'test_driver'));
    const packages = [];
    for (const name of ['flutter_driver', 'integration_test', 'vm_service']) {
        const root = join(sdk, 'packages', name); await mkdir(join(root, 'lib'), { recursive: true });
        await writeFile(join(root, 'pubspec.yaml'), `name: ${name}\nversion: 15.2.0\n`);
        await writeFile(join(root, 'lib', `${name}.dart`), "import 'dart:io';\n");
        packages.push({ name, rootUri: pathToFileURL(root).href, packageUri: 'lib/' });
    }
    await writeFile(join(mobile, '.dart_tool/package_config.json'), JSON.stringify({ configVersion: 2, packages }));
    await writeFile(join(mobile, 'test_driver/vod_commands.dart'), "import 'package:flutter_driver/flutter_driver.dart';\nimport 'package:integration_test/integration_test.dart';\n");
    await writeFile(join(mobile, 'pubspec.lock'), 'locked'); const dart = join(sdk, 'dart.exe'); await writeFile(dart, 'verified executable');
    const first = await androidDriverInputs(workspace, sdk, dart);
    assert.ok(first.files.some(({ path }) => path === 'flutter_driver/lib/flutter_driver.dart'));
    await writeFile(join(sdk, 'packages/flutter_driver/lib/flutter_driver.dart'), 'changed');
    assert.notEqual((await androidDriverInputs(workspace, sdk, dart)).sha256, first.sha256);
    packages[0].rootUri = packages[1].rootUri;
    await writeFile(join(mobile, '.dart_tool/package_config.json'), JSON.stringify({ configVersion: 2, packages }));
    await assert.rejects(androidDriverInputs(workspace, sdk, dart));
}));
