import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';

function docker(args) {
    return new Promise((resolve, reject) => {
        execFile('docker.exe', args, { windowsHide: true, timeout: 10000, maxBuffer: 65536 }, (error, output) => {
            if (error) reject(new Error('The owned test containers could not be inspected or cleaned.'));
            else resolve(output.trim());
        });
    });
}

export async function cleanupOwnedContainers(markerPath, resourceId) {
    if (!markerPath) return [];
    let content;
    try { content = await readFile(markerPath, 'utf8'); }
    catch (error) { if (error.code === 'ENOENT') return []; throw error; }
    if (!/^[a-f0-9-]{36}$/.test(resourceId)) throw new Error('The test resource identity is invalid.');
    const candidates = new Set();
    for (const line of content.split('\n').filter(Boolean)) {
        const candidate = JSON.parse(line);
        if (candidate.resourceId !== resourceId || !/^duskcue-playback-[a-f0-9-]{36}$/.test(candidate.name)) throw new Error('The container marker is outside this testing workflow.');
        candidates.add(candidate.name);
    }
    const removed = [];
    for (const name of candidates) {
        const ids = (await docker(['ps', '-aq', '--filter', `name=^${name}$`, '--filter', `label=duskcue.test.resource-id=${resourceId}`])).split(/\s+/).filter(Boolean);
        for (const id of ids) {
            if (!/^[a-f0-9]{12,64}$/.test(id)) throw new Error('The Docker container identity is invalid.');
            const labels = JSON.parse(await docker(['inspect', '--format', '{{json .Config.Labels}}', id]));
            if (labels?.['duskcue.test.resource-id'] !== resourceId) throw new Error('The container no longer belongs to this testing workflow.');
            await docker(['rm', '--force', id]);
            removed.push(id);
        }
    }
    return removed;
}
