export const PRESETS = {
    'web-unit': { policy: 'unit', heapMiB: 1536, args: ['--prefix', 'clients/web', 'run', 'test:unit', '--', '--run', '--maxWorkers=1'] },
    'web-e2e': { policy: 'browser', heapMiB: 3072, args: ['--prefix', 'clients/web', 'run', 'test:e2e', '--', '--workers=1'] },
    'web-check': { policy: 'check', heapMiB: 3072, args: ['--prefix', 'clients/web', 'run', 'check'] },
    'web-build': { policy: 'build', heapMiB: 3072, args: ['--prefix', 'clients/web', 'run', 'build'] },
    'desktop-build': { policy: 'build', heapMiB: 3072, args: ['--prefix', 'clients/desktop', 'run', 'build'] },
    'tauri-build': { policy: 'native', heapMiB: 3072, args: ['--prefix', 'clients/desktop', 'run', 'tauri:build', '--'] },
    'cargo-check': { policy: 'cargo', command: 'cargo.exe', args: ['check', '-p', 'duskcue', '--locked', '-j', '2'] },
    'cargo-clippy': { policy: 'cargo', command: 'cargo.exe', args: ['clippy', '-p', 'duskcue', '--all-targets', '--locked', '-j', '2', '--', '-D', 'warnings'] },
    'cargo-test': { policy: 'cargo', command: 'cargo.exe', args: ['test', '-p', 'duskcue', '--locked', '-j', '2'] },
    'rust-cached-unit': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/run-cached-rust-tests.mjs'] },
    'caption-font-probe': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/probe-caption-fonts.mjs'] },
    'caption-runtime-qualify': { policy: 'unit', heapMiB: 128, command: 'node', args: ['scripts/testing-memory/qualify-caption-runtime.mjs'] },
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
