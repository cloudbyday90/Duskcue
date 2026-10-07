import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { access, mkdir, mkdtemp, readFile, rmdir, unlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { ownedProcessBytes, stopOwnedProcess } from './owned-process.mjs';
import { acquireWorkflowLock } from './workflow-lock.mjs';

function alive(pid) {
    try { process.kill(pid, 0); return true; }
    catch (error) { if (error.code === 'ESRCH') return false; throw error; }
}

async function waitGone(pid, timeoutMs = 15000) {
    const deadline = Date.now() + timeoutMs;
    while (alive(pid)) {
        if (Date.now() > deadline) throw new Error('The known test helper did not exit.');
        await new Promise((resolve) => setTimeout(resolve, 25));
    }
}

async function helper(grandchild = false) {
    const startedAt = Date.now();
    const grandchildCode = 'setTimeout(() => process.exit(0), 12000);';
    const code = grandchild
        ? `import { spawn } from 'node:child_process'; const child = spawn(process.execPath, ['--max-old-space-size=16', '-e', ${JSON.stringify(grandchildCode)}], { windowsHide: true, stdio: 'ignore' }); child.once('spawn', () => process.stdout.write(JSON.stringify({ grandchildPid: child.pid }) + '\\n')); setTimeout(() => process.exit(0), 15000);`
        : "process.stdout.write(JSON.stringify({ ready: true }) + '\\n'); setTimeout(() => process.exit(0), 15000);";
    const child = spawn(process.execPath, ['--max-old-space-size=16', '--input-type=module', '-e', code], { windowsHide: true, stdio: ['ignore', 'pipe', 'ignore'] });
    child.startedAt = startedAt;
    const closed = once(child, 'close');
    const ready = await Promise.race([
        once(child.stdout, 'data').then(([chunk]) => JSON.parse(chunk.toString().trim())),
        closed.then(() => { throw new Error('The known helper exited before becoming ready.'); })
    ]);
    return { child, closed, grandchildPid: ready.grandchildPid };
}

async function cleanup(owned) {
    if (owned.child.exitCode === null && owned.child.signalCode === null) await stopOwnedProcess(owned.child);
    await owned.closed;
    if (owned.grandchildPid) await waitGone(owned.grandchildPid);
}

async function lockFixture() {
    const cache = fileURLToPath(new URL('../../.cache/testing-memory/', import.meta.url));
    await mkdir(cache, { recursive: true });
    const directory = await mkdtemp(join(cache, 'owned-process-'));
    const path = join(directory, 'active.json');
    return {
        path,
        async cleanup() {
            for (const file of [path, `${path}.recovery`]) {
                await unlink(file).catch((error) => { if (error.code !== 'ENOENT') throw error; });
            }
            await rmdir(directory);
        }
    };
}

test('telemetry includes only descendants and counts each process once', () => {
    const sample = { processes: [
        { pid: 1, parentPid: 99, privateBytes: 10, workingSetBytes: 20 },
        { pid: 3, parentPid: 2, privateBytes: 30, workingSetBytes: 40 },
        { pid: 2, parentPid: 1, privateBytes: 50, workingSetBytes: 60 },
        { pid: 4, parentPid: 99, privateBytes: 1000, workingSetBytes: 2000 }
    ] };
    assert.deepEqual(ownedProcessBytes(sample, 1), { processCount: 3, privateBytes: 90, workingSetBytes: 120 });
});

test('stops the actual minimal owned child and its grandchild', { skip: process.platform !== 'win32', concurrency: false }, async () => {
    const owned = await helper(true);
    try {
        assert.equal(alive(owned.child.pid), true);
        assert.equal(alive(owned.grandchildPid), true);
        await stopOwnedProcess(owned.child);
        await owned.closed;
        await waitGone(owned.grandchildPid);
        assert.notEqual(owned.child.exitCode, null);
    } finally {
        await cleanup(owned);
    }
});

test('refuses mismatched executable and creation time while leaving the helper alive', { skip: process.platform !== 'win32', concurrency: false }, async () => {
    const owned = await helper();
    try {
        const identity = { pid: owned.child.pid, exitCode: null, signalCode: null, startedAt: owned.child.startedAt, spawnfile: owned.child.spawnfile };
        await assert.rejects(stopOwnedProcess({ ...identity, spawnfile: 'unowned-fixture.exe' }), /no process was terminated/);
        assert.equal(alive(owned.child.pid), true);
        await assert.rejects(stopOwnedProcess({ ...identity, startedAt: identity.startedAt - 60000 }), /no process was terminated/);
        assert.equal(alive(owned.child.pid), true);
    } finally {
        await cleanup(owned);
    }
});

test('two contenders reclaim one stale lock without admitting two owners', { concurrency: false }, async () => {
    const fixture = await lockFixture();
    const exited = spawn(process.execPath, ['--max-old-space-size=16', '-e', 'process.exit(0)'], { windowsHide: true, stdio: 'ignore' });
    await once(exited, 'close');
    let winner;
    try {
        assert.equal(alive(exited.pid), false);
        await writeFile(fixture.path, JSON.stringify({ id: 'exited-helper', pid: exited.pid, childPid: null, workload: 'fixture' }), 'utf8');
        const contenders = await Promise.allSettled([acquireWorkflowLock(fixture.path, 'fixture-a'), acquireWorkflowLock(fixture.path, 'fixture-b')]);
        const acquired = contenders.filter((result) => result.status === 'fulfilled');
        assert.equal(acquired.length, 1);
        assert.equal(contenders.filter((result) => result.status === 'rejected').length, 1);
        winner = acquired[0].value;
        await winner.childStarted(exited.pid);
        const record = JSON.parse(await readFile(fixture.path, 'utf8'));
        assert.equal(record.pid, process.pid);
        assert.equal(record.childPid, exited.pid);
        await winner.release();
        winner = null;
        await assert.rejects(access(fixture.path), { code: 'ENOENT' });
    } finally {
        if (winner) await winner.release();
        await fixture.cleanup();
    }
});

test('a live reused owner PID and incomplete recovery remain conservatively busy', { concurrency: false }, async () => {
    const fixture = await lockFixture();
    try {
        const record = { id: 'old-record', pid: process.pid, childPid: null, workload: 'fixture', startedAt: '2000-01-01T00:00:00Z' };
        await writeFile(fixture.path, JSON.stringify(record), 'utf8');
        await assert.rejects(acquireWorkflowLock(fixture.path, 'fixture-contender'), /still active/);
        assert.deepEqual(JSON.parse(await readFile(fixture.path, 'utf8')), record);
        await writeFile(`${fixture.path}.recovery`, '', 'utf8');
        await assert.rejects(acquireWorkflowLock(fixture.path, 'fixture-contender'), /recovery is active or incomplete/);
        assert.deepEqual(JSON.parse(await readFile(fixture.path, 'utf8')), record);
    } finally {
        await fixture.cleanup();
    }
});
