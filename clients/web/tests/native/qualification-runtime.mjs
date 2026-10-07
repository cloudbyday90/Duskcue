import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createServer as createPortServer } from 'node:net';
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from '@playwright/test';
import { stopOwnedProcess } from '../../../../scripts/testing-memory/owned-process.mjs';
import { inspectNativeProcesses } from './qualification-processes.mjs';
import { captureNativeLog } from './qualification-log.mjs';

export async function startHealthFixtures(origins) {
    const requests = [];
    const servers = [];
    try {
        for (const origin of origins) {
            const server = createServer((request, response) => {
                requests.push({ origin, method: request.method, path: request.url });
                const healthy = request.method === 'GET' && request.url === '/health/ready';
                response.writeHead(healthy ? 200 : 501, { 'Content-Type': 'application/json' });
                response.end(JSON.stringify(healthy ? { status: 'ready', qualification_fixture: true } : { detail: 'Unexpected native fixture HTTP request' }));
            });
            const url = new URL(origin);
            await new Promise((resolve, reject) => {
                server.once('error', reject);
                server.listen(Number(url.port), url.hostname, resolve);
            });
            servers.push(server);
        }
        return { requests, close: () => Promise.all(servers.map((server) => new Promise((resolve) => server.close(resolve)))) };
    } catch (error) {
        await Promise.all(servers.map((server) => new Promise((resolve) => server.close(resolve))));
        throw error;
    }
}

async function unusedCdpPort() {
    const reservation = createPortServer();
    await new Promise((resolve, reject) => {
        reservation.once('error', reject);
        reservation.listen(0, '127.0.0.1', resolve);
    });
    const port = reservation.address().port;
    await new Promise((resolve) => reservation.close(resolve));
    return port;
}

export async function launchQualification(manifest) {
    const port = await unusedCdpPort();
    await mkdir(manifest.webviewData, { recursive: true });
    const startedAt = Date.now();
    const child = spawn(manifest.executable, [], {
        windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'],
        env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${port}`, WEBVIEW2_USER_DATA_FOLDER: manifest.webviewData },
    });
    child.startedAt = startedAt;
    let spawnError = null;
    child.on('error', (error) => { spawnError = error; });
    const runtime = { child, browser: null, page: null, endpoint: `http://127.0.0.1:${port}`, directory: manifest.directory, ownedIdentities: [], childClosed: false };
    runtime.log = captureNativeLog(child, join(manifest.directory, 'native-process.log'));
    child.once('close', () => { runtime.childClosed = true; });
    const deadline = Date.now() + 30_000;
    const endpoint = `http://127.0.0.1:${port}`;
    try {
        const initial = await inspectNativeProcesses(runtime);
        if (!initial.hostVerified) throw new Error('The spawned native host identity could not be verified.');
        while (Date.now() < deadline) {
            if (spawnError) throw spawnError;
            if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Native process exited before CDP readiness: ${child.exitCode ?? child.signalCode}`);
            const ready = await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(800) }).then((response) => response.ok).catch(() => false);
            if (ready) {
                const ownership = await inspectNativeProcesses(runtime);
                if (!ownership.hostVerified) throw new Error('The native host exited or changed identity before CDP attachment.');
                if (!ownership.processes.some((identity) => /^msedge(?:webview2)?\.exe$/i.test(identity.name))) throw new Error('A WebView2 browser descendant could not be verified under the owned native host; native process cleanup qualification is unavailable.');
                runtime.browser = await chromium.connectOverCDP(endpoint, { timeout: 10_000 });
                const context = runtime.browser.contexts()[0];
                if (!context) throw new Error('The native WebView2 context was unavailable.');
                runtime.page = context.pages()[0] || await context.waitForEvent('page', { timeout: 10_000 });
                return runtime;
            }
            await new Promise((resolve) => setTimeout(resolve, 150));
        }
        throw new Error('The live native process did not expose WebView2 CDP within 30 seconds.');
    } catch (error) {
        try { await stopQualification(runtime); }
        catch (cleanupError) { error.message += ` Native startup cleanup: ${cleanupError.message}`; }
        throw error;
    }
}

export async function stopQualification(runtime) {
    const evidence = { hostPid: runtime.child.pid ?? null, hostAlreadyExited: runtime.child.exitCode !== null || runtime.child.signalCode !== null, status: 'in_progress' };
    let failure;
    try {
        evidence.before = Number.isInteger(runtime.child.pid) ? await inspectNativeProcesses(runtime) : { hostExists: false, hostVerified: false, processes: [], unverifiedExitedHostDescendants: [], hostNeverSpawned: true };
        evidence.hostAlreadyExited ||= runtime.child.exitCode !== null || runtime.child.signalCode !== null || !evidence.before.hostExists;
        if (!evidence.hostAlreadyExited && Number.isInteger(runtime.child.pid)) {
            if (!evidence.before.hostVerified) throw new Error('Native host ownership changed; no process was terminated.');
            await stopOwnedProcess(runtime.child);
        } else {
            evidence.orphanedVerifiedProcesses = evidence.before.processes;
            evidence.unverifiedExitedHostDescendants = evidence.before.unverifiedExitedHostDescendants;
        }
        if (!runtime.childClosed) {
            let timer;
            try {
                await Promise.race([new Promise((resolve) => runtime.child.once('close', resolve)), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Owned native host pipes did not close within five seconds.')), 5_000); })]);
            } finally { clearTimeout(timer); }
        }
    } catch (error) {
        failure = error;
        if (Number.isInteger(runtime.child.pid) && runtime.child.exitCode === null && runtime.child.signalCode === null) {
            try { await stopOwnedProcess(runtime.child); }
            catch (stopError) { evidence.verifiedStopError = stopError.message; }
        }
    }
    if (runtime.browser) {
        let timer;
        try {
            await Promise.race([runtime.browser.close(), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('The owned WebView2 CDP connection did not close.')), 5_000); })]);
        } catch (error) { evidence.cdpCloseError = error.message; if (runtime.browser.isConnected()) failure ||= error; }
        finally { clearTimeout(timer); }
    }
    try {
        evidence.after = Number.isInteger(runtime.child.pid) ? await inspectNativeProcesses(runtime) : evidence.before;
        if (evidence.after.processes.length || evidence.after.unverifiedExitedHostDescendants.length) throw new Error('Native processes remain after host cleanup; inspect native-process-cleanup.json. No global process-name termination was attempted.');
    } catch (error) { failure ||= error; }
    try { await runtime.log?.close(); } catch (error) { failure ||= error; }
    evidence.status = failure ? 'failed' : 'passed';
    if (failure) evidence.error = failure.message;
    await writeFile(join(runtime.directory, 'native-process-cleanup.json'), `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
    if (failure) throw failure;
    return evidence;
}
