import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { promisify } from 'node:util';
import { appendFile, mkdir, open, readFile, stat, unlink, writeFile } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const execute = promisify(execFile);
const directory = fileURLToPath(new URL('../../../../.cache/tonight-web/media/', import.meta.url));
const marker = join(directory, 'ready.json');
const lock = join(directory, 'generation.lock');
const resourceId = process.env.DUSKCUE_TEST_RESOURCE_ID || randomUUID();
const resourceLabel = 'duskcue.test.resource-id';
const resourceDirectory = fileURLToPath(new URL('../../../../.cache/testing-memory/', import.meta.url));

async function command(binary, args, timeout = 60_000) {
    return execute(binary, args, { timeout, maxBuffer: 2_000_000, windowsHide: true });
}

function ffmpegArguments(args) {
    const bounded = ['-filter_threads', '2', '-filter_complex_threads', '2'];
    for (const argument of args) {
        if (argument === '-i') bounded.push('-threads', '2');
        bounded.push(argument);
    }
    bounded.splice(bounded.length - 1, 0, '-threads', '2');
    return bounded;
}

async function recordContainer(name) {
    const file = process.env.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE;
    if (!file) return;
    const scoped = relative(resourceDirectory, resolve(file));
    if (!isAbsolute(file) || !scoped || scoped.startsWith('..') || isAbsolute(scoped)) throw new Error('The fixture container marker must stay inside the testing-memory directory.');
    await appendFile(file, `${JSON.stringify({ name, resourceId })}\n`, 'utf8');
}

async function removeOwnedContainer(name) {
    let container;
    try {
        const result = await command('docker', ['container', 'inspect', name, '--format', '{"id":{{json .Id}},"resourceId":{{json (index .Config.Labels "duskcue.test.resource-id")}}}'], 10_000);
        container = JSON.parse(result.stdout.trim());
    } catch (error) {
        if (/No such (container|object)/i.test(error.stderr || '')) return;
        throw new Error(`Could not inspect the fixture container ${name} for cleanup.`, { cause: error });
    }
    if (container.resourceId !== resourceId || !/^[a-f0-9]{64}$/i.test(container.id || '')) {
        throw new Error(`The container ${name} is not the owned fixture container; no container was removed.`);
    }
    await command('docker', ['container', 'rm', '--force', container.id], 10_000);
}

async function dockerEncoder(image, args) {
    const name = `duskcue-playback-${randomUUID()}`;
    await recordContainer(name);
    const base = ['run', '--rm', '--pull=never', '--name', name, '--label', `${resourceLabel}=${resourceId}`, '--memory=512m', '--memory-swap=512m', '--cpus=2', '--mount', `type=bind,source=${directory},target=/fixtures`, '--entrypoint', 'ffmpeg', image];
    try {
        return await command('docker', [...base, ...args]);
    } catch (error) {
        try { await removeOwnedContainer(name); }
        catch (cleanupError) { throw new AggregateError([error, cleanupError], `The fixture container ${name} failed and its cleanup could not be verified.`); }
        throw error;
    }
}

async function encoder() {
    const binary = process.env.DUSKCUE_TEST_FFMPEG || 'ffmpeg';
    try {
        const result = await command(binary, ['-version']);
        return { version: result.stdout.split('\n')[0], run: (args) => command(binary, ffmpegArguments(args)), path: (name) => join(directory, name) };
    } catch (error) {
        if (process.env.DUSKCUE_TEST_FFMPEG) throw error;
    }
    const image = process.env.DUSKCUE_TEST_FFMPEG_IMAGE || 'duskcue:local';
    try {
        await command('docker', ['image', 'inspect', image, '--format', '{{.Id}}']);
        const result = await dockerEncoder(image, ['-version']);
        return { version: result.stdout.split('\n')[0], run: (args) => dockerEncoder(image, ffmpegArguments(args)), path: (name) => `/fixtures/${name.replaceAll('\\', '/')}` };
    } catch (error) {
        throw new Error('Playback browser tests require host FFmpeg or an existing DUSKCUE_TEST_FFMPEG_IMAGE with FFmpeg.', { cause: error });
    }
}

async function existing() {
    try {
        const data = JSON.parse(await readFile(marker, 'utf8'));
        if (data.version !== 2) return null;
        for (const name of ['standard.mp4', 'ending.mp4', 'standard/index.m3u8', 'ending/index.m3u8', 'standard/segment-000.ts', 'ending/segment-000.ts', 'storyboard.webp']) {
            if (!(await stat(join(directory, name))).size) return null;
        }
        return { directory, ...data };
    } catch {
        return null;
    }
}

export async function ensurePlaybackMedia() {
    await mkdir(directory, { recursive: true });
    const ready = await existing();
    if (ready) return ready;
    let handle;
    try {
        handle = await open(lock, 'wx');
    } catch (error) {
        if (error.code !== 'EEXIST') throw error;
        const deadline = Date.now() + 65_000;
        while (Date.now() < deadline) {
            const result = await existing();
            if (result) return result;
            await new Promise((resolve) => setTimeout(resolve, 100));
        }
        throw new Error(`Playback media generation did not finish. Check ${lock}.`);
    }
    try {
        const ffmpeg = await encoder();
        await ffmpeg.run(['-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=320x180:rate=24', '-f', 'lavfi', '-i', 'anullsrc=r=48000:cl=stereo', '-t', '20', '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '32', '-pix_fmt', 'yuv420p', '-profile:v', 'baseline', '-level:v', '3.0', '-g', '48', '-sc_threshold', '0', '-c:a', 'aac', '-b:a', '48k', '-metadata:s:a:0', 'language=eng', '-movflags', '+faststart', ffmpeg.path('standard.mp4')]);
        await ffmpeg.run(['-hide_banner', '-loglevel', 'error', '-y', '-i', ffmpeg.path('standard.mp4'), '-t', '2', '-c', 'copy', '-movflags', '+faststart', ffmpeg.path('ending.mp4')]);
        for (const name of ['standard', 'ending']) {
            await mkdir(join(directory, name), { recursive: true });
            await ffmpeg.run(['-hide_banner', '-loglevel', 'error', '-y', '-i', ffmpeg.path(`${name}.mp4`), '-c', 'copy', '-hls_time', '2', '-hls_playlist_type', 'vod', '-hls_flags', 'independent_segments', '-hls_segment_filename', ffmpeg.path(`${name}/segment-%03d.ts`), ffmpeg.path(`${name}/index.m3u8`)]);
            await writeFile(join(directory, name, 'manifest.m3u8'), '#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-STREAM-INF:BANDWIDTH=600000,RESOLUTION=320x180\nv0/index.m3u8\n');
        }
        await ffmpeg.run(['-hide_banner', '-loglevel', 'error', '-y', '-i', ffmpeg.path('ending.mp4'), '-frames:v', '1', '-vf', 'scale=160:90', ffmpeg.path('storyboard.webp')]);
        const data = { version: 2, encoder: ffmpeg.version, standardDuration: 20, endingDuration: 2 };
        await writeFile(marker, JSON.stringify(data));
        return { directory, ...data };
    } finally {
        await handle.close();
        await unlink(lock);
    }
}
