// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { randomBytes, randomUUID } from 'node:crypto';
import { appendFile, lstat, mkdir, realpath, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout } from 'node:timers/promises';
import { createGuardedDocker } from './guarded-docker.mjs';
import { runBounded } from '../qualification/commands.mjs';
import { qualificationSources, hashFile } from '../qualification/source.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));

async function fixture(root, path, directory = false) {
    const canonical = await realpath(path);
    const scoped = relative(root, canonical);
    const info = await lstat(canonical);
    if (!scoped || scoped.startsWith('..') || isAbsolute(scoped) || info.isSymbolicLink() || (directory ? !info.isDirectory() : !info.isFile())) throw new Error('VOD fixture must be a regular owned cache input.');
    return canonical;
}

async function main() {
    const args = process.argv.slice(2);
    if (args.length !== 6 || args[0] !== '--ffmpeg-bin' || args[2] !== '--source' || args[4] !== '--hls') throw new Error('Provide --ffmpeg-bin, --source and --hls ignored fixture paths.');
    const workflow = await createGuardedDocker(workspace);
    const cache = await realpath(join(workspace, '.cache'));
    const ffmpeg = await fixture(cache, args[1], true);
    const source = await fixture(cache, args[3]);
    const hls = await fixture(cache, args[5], true);
    const directory = join(cache, 'vod-lifecycle', workflow.resourceId);
    await mkdir(directory);
    if (dirname(await realpath(directory)) !== await realpath(join(cache, 'vod-lifecycle'))) throw new Error('VOD evidence left its owner directory.');
    const result = { version: 1, resourceId: workflow.resourceId, status: 'in_progress', cases: [], cleanup: false };
    const password = randomBytes(24).toString('hex');
    const checked = async (command) => {
        const execution = await workflow.command(command);
        if (execution.code !== 0 || execution.signal) throw new Error('Owned VOD Docker operation failed.');
        return execution.stdout.trim();
    };
    let failure;
    try {
        const endpoint = await checked(['context', 'inspect', '--format', '{{.Endpoints.docker.Host}}']);
        if (!endpoint.startsWith('npipe:') && !endpoint.startsWith('unix:///')) throw new Error('VOD PostgreSQL requires a local Docker engine.');
        const version = await runBounded(join(ffmpeg, 'ffmpeg.exe'), ['-version'], { bytes: 32768 });
        if (version.code !== 0 || !/^ffmpeg version 8\.1\.2(?:-|\s)/.test(version.stdout)) throw new Error('Verify FFmpeg 8.1.2 for native qualification.');
        const probe = await runBounded(join(ffmpeg, 'ffprobe.exe'), ['-v', 'error', '-show_entries', 'format=duration:stream=index,codec_name,codec_type,width,height', '-of', 'json', source], { bytes: 32768 });
        if (probe.code !== 0) throw new Error('VOD source probe failed.');
        const evidence = JSON.parse(probe.stdout);
        if (Number(evidence.format?.duration) < 8 || Number(evidence.format?.duration) > 9 || !evidence.streams?.some((s) => s.index === 1 && s.codec_name === 'h264' && s.width === 320 && s.height === 180)) throw new Error('VOD qualification requires the established eight-second audio-first source.');
        result.source = await qualificationSources(workspace);
        result.driverSha256 = await hashFile(fileURLToPath(import.meta.url));
        const image = JSON.parse(await checked(['image', 'inspect', 'postgres:18', '--format', '{{json .}}']));
        if (!/^sha256:[a-f0-9]{64}$/.test(image.Id || '') || image.Os !== 'linux') throw new Error('VOD PostgreSQL image must be immutable Linux PostgreSQL18.');
        result.postgresImage = image.Id;
        const name = `duskcue-playback-${randomUUID()}`;
        await appendFile(process.env.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE, `${JSON.stringify({ resourceId: workflow.resourceId, name })}\n`);
        const database = 'duskcue_transcode_test_vod';
        await checked(['run', '--detach', '--pull=never', '--name', name, '--label', `duskcue.test.resource-id=${workflow.resourceId}`,
            '--memory=512m', '--memory-swap=512m', '--cpus=2', '--pids-limit=128', '--cap-drop=ALL', '--security-opt=no-new-privileges', '--read-only', '--user=postgres',
            '--tmpfs', '/tmp:rw,nosuid,nodev,noexec,size=536870912,mode=1777', '--tmpfs', '/var/run/postgresql:rw,nosuid,nodev,noexec,size=16777216,mode=1777',
            '--publish', '127.0.0.1::5432', '--env', `POSTGRES_PASSWORD=${password}`, '--env', `POSTGRES_DB=${database}`, '--env', 'PGDATA=/tmp/duskcue-postgres',
            image.Id, 'postgres', '-c', 'shared_buffers=16MB', '-c', 'max_connections=16']);
        const observed = JSON.parse(await checked(['inspect', name, '--format', '{{json .}}']));
        if (observed.Config?.Labels?.['duskcue.test.resource-id'] !== workflow.resourceId || observed.Image !== image.Id || observed.HostConfig?.Memory !== 536870912 || observed.HostConfig?.MemorySwap !== 536870912 || observed.HostConfig?.NanoCpus !== 2000000000 || observed.HostConfig?.PidsLimit !== 128 || !observed.HostConfig?.ReadonlyRootfs) throw new Error('VOD PostgreSQL ownership or bounds changed.');
        const port = (await checked(['port', name, '5432/tcp'])).match(/^127\.0\.0\.1:(\d+)$/)?.[1];
        if (!port) throw new Error('VOD PostgreSQL must be loopback-only.');
        const deadline = Date.now() + 30000;
        while (true) {
            const ready = await workflow.command(['exec', name, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres', '-d', database]);
            if (ready.code === 0) break;
            if (Date.now() >= deadline) throw new Error('VOD PostgreSQL readiness expired.');
            await setTimeout(100);
        }
        const url = `postgres://postgres:${password}@127.0.0.1:${port}/${database}`;
        const env = { ...process.env, PATH: `${ffmpeg};${process.env.PATH}`, DUSKCUE_TEST_PLAYBACK_SOURCE: source, DUSKCUE_HLS_TEST_MEDIA_DIR: hls, DUSKCUE_DATABASE_URL: url, DUSKCUE_TRANSCODE_ACCESS_TESTS: 'disposable' };
        const cases = [
            ['retention', '--test', 'playback_retention', true],
            ['maintenance-loss', '--lib', 'lost_maintenance_completion_is_retained_at_root', true],
            ['owner', '--lib', 'services::vod_lifecycle::tests::', false],
            ['direct', '--lib', 'domains::playback::file_delivery::tests::', false],
            ['worker', '--lib', 'actual_long_running_ffmpeg_is_terminated_before_its_permit_and_cache_are_released', true],
            ['worker-abort', '--lib', 'lost_worker_completion_quarantines_output_and_retains_encoder_capacity', false],
            ['manager', '--lib', 'actual_completed_output_seek_recovery_and_root_retirement_are_owned', true],
            ['hls-http', '--lib', 'real_fmp4_http_paths_preserve_auth_policy_and_reject_traversal', true],
            ['root', '--lib', 'root_deadline_keeps_database_open_until_owned_vod_file_completion', true],
            ['stop', '--test', 'playback_stop_contract', true],
        ];
        for (const [name, target, selector, ignored] of cases) {
            const args = ['test', '-p', 'duskcue', '--locked', '--offline', '-j', '2', target, selector, '--', ...(ignored ? ['--ignored'] : []), '--nocapture'];
            const execution = await runBounded('cargo.exe', args, { cwd: workspace, env, log: join(directory, `${name}.log`), redact: [password, url], timeoutMs: 1800000, bytes: 16777216,
                onLine: name === 'retention' ? (line) => { const marker = line.match(/\bRETENTION_CASE [a-z_]+ passed$/)?.[0]; if (marker) console.log(marker); } : undefined });
            const match = execution.stdout.match(/test result: ok\. (\d+) passed; 0 failed; 0 ignored;/);
            const passed = execution.code === 0 && !execution.signal && !!match && Number(match[1]) > 0 && (['owner', 'direct'].includes(name) || Number(match[1]) === 1);
            result.cases.push({ name, passed, tests: match ? Number(match[1]) : 0 });
            console.log(`VOD_CASE ${JSON.stringify(result.cases.at(-1))}`);
            if (!passed) throw new Error(`VOD ${name} qualification failed; inspect its bounded log.`);
        }
        if ((await qualificationSources(workspace)).sha256 !== result.source.sha256 || await hashFile(fileURLToPath(import.meta.url)) !== result.driverSha256) throw new Error('VOD qualification sources changed during execution.');
        result.status = 'passed';
    } catch (error) { failure = error; result.status = 'failed'; result.error = error.message; }
    finally {
        try { await workflow.cleanup(); result.cleanup = true; }
        catch (error) { failure ||= error; result.status = 'failed'; }
        await writeFile(join(directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`);
    }
    console.log(`VOD_OUTPUT_RESULT ${JSON.stringify({ status: result.status, cleanup: result.cleanup, resourceId: workflow.resourceId })}`);
    if (failure) throw failure;
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
