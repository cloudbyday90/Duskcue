import { execFile } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const execute = promisify(execFile);
const script = fileURLToPath(new URL('./qualification-processes.ps1', import.meta.url));

export async function inspectNativeProcesses(runtime) {
    const child = runtime.child;
    if (!Number.isInteger(child.pid) || !Number.isFinite(child.startedAt)) throw new Error('Native ownership requires the actual spawned PID and creation-time boundary.');
    const path = join(runtime.directory, 'native-process-identities.json');
    await writeFile(path, `${JSON.stringify({
        host: { pid: child.pid, parentPid: process.pid, name: basename(child.spawnfile), startedAt: child.startedAt },
        known: runtime.ownedIdentities || [],
    }, null, 2)}\n`, 'utf8');
    const response = await execute('pwsh.exe', ['-NoProfile', '-File', script, '-IdentityPath', path], { windowsHide: true, timeout: 10_000, maxBuffer: 1_000_000 });
    const snapshot = JSON.parse(response.stdout.trim());
    const known = new Map((runtime.ownedIdentities || []).map((identity) => [`${identity.pid}:${identity.createdAt}`, identity]));
    for (const identity of snapshot.processes) known.set(`${identity.pid}:${identity.createdAt}`, identity);
    runtime.ownedIdentities = [...known.values()];
    runtime.lastProcessSnapshot = snapshot;
    return snapshot;
}
