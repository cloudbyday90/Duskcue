import { open, readFile, unlink, writeFile, rename } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

function live(pid) {
    if (!Number.isInteger(pid) || pid <= 0) return false;
    try { process.kill(pid, 0); return true; } catch (error) { return error.code !== 'ESRCH'; }
}

export async function acquireWorkflowLock(path, workload) {
    const owner = { id: randomUUID(), pid: process.pid, childPid: null, workload, startedAt: new Date().toISOString() };
    for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
            const handle = await open(path, 'wx');
            await handle.writeFile(JSON.stringify(owner));
            await handle.close();
            return {
                async childStarted(pid) {
                    const saved = JSON.parse(await readFile(path, 'utf8'));
                    if (saved.id !== owner.id) throw new Error('The testing workflow no longer owns its lock.');
                    owner.childPid = pid;
                    const temporary = `${path}.${owner.id}.tmp`;
                    await writeFile(temporary, JSON.stringify(owner), 'utf8');
                    await rename(temporary, path);
                },
                async release() { const saved = JSON.parse(await readFile(path, 'utf8')); if (saved.id === owner.id) await unlink(path); },
            };
        } catch (error) {
            if (error.code !== 'EEXIST') throw error;
            const recoveryPath = `${path}.recovery`;
            let recovery;
            try { recovery = await open(recoveryPath, 'wx'); }
            catch (failure) { if (failure.code === 'EEXIST') throw new Error('Workflow-lock recovery is active or incomplete. No workload was started.'); throw failure; }
            try {
                await recovery.writeFile(JSON.stringify(owner));
                await recovery.close();
                let previous;
                try { previous = JSON.parse(await readFile(path, 'utf8')); }
                catch (failure) { if (failure.code === 'ENOENT') continue; throw failure; }
                if (live(previous.pid) || live(previous.childPid)) throw new Error(`A testing workflow is still active (${previous.workload}). Wait for its owned process to finish.`);
                await unlink(path);
            } finally {
                await recovery.close().catch(() => {});
                await unlink(recoveryPath);
            }
        }
    }
    throw new Error('Could not acquire the testing workflow lock.');
}
