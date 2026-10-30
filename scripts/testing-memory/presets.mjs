// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

export const PRESETS = {
    'flutter-storage-patch-source': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'storage-patch-source'] },
    'android-tooling-test': { policy: 'unit', heapMiB: 128, command: 'node', args: ['--test', '--test-concurrency=1', 'scripts/testing-memory/android-tooling.test.mjs', 'scripts/testing-memory/android-fault-gateway.test.mjs', 'scripts/testing-memory/presets.test.mjs', 'scripts/qualification/qualification.test.mjs'] },
    'android-worker': { policy: 'cargo', heapMiB: 512, command: 'node', args: ['scripts/testing-memory/qualify-vod-output.mjs', '--android-worker-only'] },
    'android-playback': { policy: 'native', heapMiB: 512, command: 'node', args: ['scripts/testing-memory/qualify-vod-output.mjs', '--android-only'] },
    'android-boot': { policy: 'native', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/android-boot.mjs'] },
    'android-sdk': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/android-sdk.mjs'] },
    'flutter-tooling-test': { policy: 'unit', heapMiB: 128, command: 'node', args: ['--test', '--test-concurrency=1', 'scripts/testing-memory/flutter-release.test.mjs'] },
    'flutter-sdk-update': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-sdk.mjs'] },
    'flutter-version': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'version'] },
    'flutter-pub': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'pub'] },
    'flutter-check': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'analyze'] },
    'flutter-test': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'test'] },
    'flutter-format': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'format'] },
    'flutter-format-write': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'format-write'] },
    'flutter-fix-braces': { policy: 'check', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'fix'] },
    'flutter-apk': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'build'] },
    'flutter-apk-production': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/flutter-command.mjs', 'build-production'] },
    'vod-client': { policy: 'browser', heapMiB: 3072, command: 'node', args: ['scripts/testing-memory/qualify-vod-output.mjs', '--client-only'] },
    'vod-client-native': { policy: 'native', heapMiB: 1536, command: 'node', args: ['scripts/testing-memory/qualify-vod-output.mjs', '--client-only'] },
    'vod-linux': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs', '--vod-output'] },
    'vod-output': { policy: 'cargo', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/qualify-vod-output.mjs'] },
    'playback-admission': { policy: 'cargo', command: 'cargo.exe', args: ['test', '-p', 'duskcue', '--locked', '-j', '2', '--features', 'playback-qualification', '--test', 'playback_admission', 'shared_viewer_ledger_and_owned_resources', '--', '--ignored', '--nocapture'] },
    'management-client': { policy: 'browser', heapMiB: 3072, command: 'cargo.exe', args: ['test', '-p', 'duskcue', '--locked', '-j', '2', '--test', 'live_tv_management_client', '--', '--ignored', '--nocapture'] },
    'owner-client': { policy: 'browser', heapMiB: 3072, command: 'cargo.exe', args: ['test', '-p', 'duskcue', '--locked', '-j', '2', '--test', 'household_client', '--', '--ignored', '--nocapture'] },
    'iptv-refresh': { policy: 'cargo', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-feed-authority.mjs', '--refresh'] },
    'iptv-import-compression': { policy: 'cargo', command: 'cargo.exe', args: ['test', '--manifest-path', 'server/tests/fixtures/import-compression/Cargo.toml', '--target-dir', 'target/import-compression', '--locked', '--offline', '-j', '2'] },
    'iptv-imports': { policy: 'cargo', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-feed-authority.mjs', '--imports'] },
    'iptv-feed-authority': { policy: 'cargo', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-feed-authority.mjs'] },
    'iptv-transport-unit': { policy: 'unit', heapMiB: 128, command: 'node', args: ['--test', '--test-concurrency=1', 'scripts/testing-memory/live-transport-proof.test.mjs', 'scripts/testing-memory/workflow-evidence.test.mjs'] },
    'cargo-iptv-clippy': { policy: 'cargo', command: 'cargo.exe', args: ['clippy', '-p', 'duskcue', '--features', 'playback-qualification', '--all-targets', '--locked', '-j', '2', '--', '-D', 'warnings'] },
    'iptv-transport-qualify': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/qualify-live-transport.mjs'] },
    'vite-editor-qualify': { policy: 'unit', heapMiB: 128, command: 'node', args: ['--test', '--test-concurrency=1', 'clients/web/tests/unit/vite-editor-security.test.mjs'] },
    'web-unit': { policy: 'unit', heapMiB: 1536, args: ['--prefix', 'clients/web', 'run', 'test:unit', '--', '--run', '--maxWorkers=1'] },
    'web-install': { policy: 'build', heapMiB: 1536, args: ['--prefix', 'clients/web', 'ci', '--ignore-scripts'] },
    'desktop-install': { policy: 'build', heapMiB: 1536, args: ['--prefix', 'clients/desktop', 'ci', '--ignore-scripts'] },
    'web-e2e': { policy: 'browser', heapMiB: 3072, args: ['--prefix', 'clients/web', 'run', 'test:e2e', '--', '--workers=1'] },
    'web-check': { policy: 'check', heapMiB: 3072, args: ['--prefix', 'clients/web', 'run', 'check'] },
    'web-build': { policy: 'build', heapMiB: 3072, args: ['--prefix', 'clients/web', 'run', 'build'] },
    'desktop-build': { policy: 'build', heapMiB: 3072, args: ['--prefix', 'clients/desktop', 'run', 'build'] },
    'tauri-build': { policy: 'native', heapMiB: 3072, args: ['--prefix', 'clients/desktop', 'run', 'tauri:build', '--'] },
    'cargo-check': { policy: 'cargo', command: 'cargo.exe', args: ['check', '-p', 'duskcue', '--locked', '-j', '2'] },
    'desktop-cargo-check': { policy: 'cargo', command: 'cargo.exe', args: ['check', '-p', 'duskcue-desktop', '--locked', '-j', '2'] },
    'linux-cargo-check': { policy: 'cargo', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-linux-cargo-check.mjs'] },
    'cargo-clippy': { policy: 'cargo', command: 'cargo.exe', args: ['clippy', '-p', 'duskcue', '--all-targets', '--locked', '-j', '2', '--', '-D', 'warnings'] },
    'cargo-test': { policy: 'cargo', command: 'cargo.exe', args: ['test', '-p', 'duskcue', '--locked', '-j', '2'] },
    'compose-build': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-compose-build.mjs'] },
    'rust-cached-unit': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-cached-rust-tests.mjs'] },
    'caption-font-probe': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/probe-caption-fonts.mjs'] },
    'caption-runtime-qualify': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/qualify-caption-runtime.mjs'] },
    'playback-recovery-qualify': { policy: 'build', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs'] },
    'playback-direct-qualify': { policy: 'build', heapMiB: 512, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs', '--direct-browser'] },
    'playback-live-qualify': { policy: 'native', heapMiB: 1536, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs', '--live-clients'] },
    'playback-native-recovery-qualify': { policy: 'native', heapMiB: 1536, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs', '--native-recovery'] },
    'playback-format-qualify': { policy: 'native', heapMiB: 1536, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs', '--format-corpus'] },
    'playback-source-qualify': { policy: 'native', heapMiB: 1536, command: 'node', args: ['scripts/testing-memory/qualify-playback-recovery.mjs', '--source-capabilities'] },
    'launcher-policy-probe': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/probe-launcher-policy.mjs'] },
    'native-qa': { policy: 'native', heapMiB: 1536, command: 'node', args: ['clients/web/tests/native/qualify.mjs'] },
};

export function workloadEnvironment(preset, inherited) {
    const environment = { ...inherited, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '1', UV_THREADPOOL_SIZE: '2' };
    if (preset.heapMiB) {
        const options = (inherited.NODE_OPTIONS || '').replace(/--max[-_]old[-_]space[-_]size(?:[-_]percentage)?(?:=|\s+)(?:"[^"]*"|'[^']*'|\S+)/g, '').trim();
        environment.NODE_OPTIONS = `${options} --max-old-space-size=${preset.heapMiB}`.trim();
    }
    return environment;
}

export function validateExtraArguments(args) {
    if (args.some((arg) => /^--(?:workers|maxWorkers|max-workers|jobs|test-threads|max[-_]old[-_]space[-_]size(?:[-_]percentage)?)(?:=|$)/.test(arg) || /^-j(?:\d|=|$)/.test(arg))) {
        throw new Error('Worker, Cargo-job and heap limits belong to this resource workflow. Extra arguments cannot override them.');
    }
}
