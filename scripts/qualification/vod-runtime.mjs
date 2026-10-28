// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { join } from 'node:path';

export async function runVodRuntime({ ownership, image, pg, databaseUrl, password, resourceId, directory, reserve }) {
    const cases = [
        ['commands','domains::playback::commands::tests::command_protocol_retains_scope_readiness_delivery_and_confirmed_cleanup',true,1],
        ['configuration','domains::playback::commands::tests::same_viewer_operations_preserve_revisions_progress_claims_and_cleanup',true,1],
        ['retention', 'bounded_receipts_preserve_replay_accounting_and_owned_cleanup', true, 1, '/qualification/evidence/playback-retention'],
        ['maintenance-loss', 'services::playback_admission::owner::maintenance::tests::lost_maintenance_completion_is_retained_at_root', true, 1],
        ['owner', 'services::vod_lifecycle::tests::', false, 9],
        ['direct', 'domains::playback::file_delivery::tests::', false, 5],
        ['worker', 'services::transcoding::lifecycle::tests::actual_long_running_ffmpeg_is_terminated_before_its_permit_and_cache_are_released', true, 1],
        ['worker-abort', 'services::transcoding::lifecycle::tests::lost_worker_completion_quarantines_output_and_retains_encoder_capacity', false, 1],
        ['manager', 'services::transcoding::output_profile::tests::actual_completed_output_seek_recovery_and_root_retirement_are_owned', true, 1],
        ['root', 'services::vod_lifecycle::root_tests::root_deadline_keeps_database_open_until_owned_vod_file_completion', true, 1],
    ];
    const results = [];
    for (const [name, selector, ignored, count, entrypoint = '/qualification/evidence/vod-lib-tests'] of cases) {
        await reserve.admit();
        const container = await ownership.create(image, [selector, ...(ignored ? ['--exact', '--ignored'] : []), '--nocapture'], {
            entrypoint, memory: 4294967296, pids: 256,
            network: `container:${pg}`, user: '65532:65532', readOnly: true,
            tmpfs: { '/tmp': 'rw,nosuid,nodev,noexec,size=67108864,uid=65532,gid=65532,mode=1770',
                '/data': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770',
                '/cache': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770' },
            environment: {
                DUSKCUE_TEST_RESOURCE_ID: resourceId, DUSKCUE_DATABASE_URL: databaseUrl,
                ...(name === 'configuration' ? { DUSKCUE_TEST_PLAYBACK_SOURCE: '/qualification/vod-source/source.mkv' } : {}),
            },
        });
        const execution = await ownership.start(container, { log: join(directory, `vod-${name}.log`), redact: [databaseUrl, password], bytes: 16777216, timeoutMs: ['retention','commands','configuration'].includes(name) ? 660000 : 120000, abortSignal: reserve.signal });
        const observed = await ownership.inspect(container);
        const passed = execution.code === 0 && !execution.signal && observed.state.exitCode === 0 && !observed.state.oomKilled
            && execution.stdout.includes(`test result: ok. ${count} passed; 0 failed; 0 ignored;`);
        results.push({ name, passed, tests: passed ? count : 0 });
        await ownership.remove(container);
        if (!passed) throw new Error(`Managed Linux VOD ${name} qualification failed; inspect its bounded log.`);
    }
    return { passed: true, cases: results };
}
