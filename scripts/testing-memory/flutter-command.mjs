// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, realpath, mkdir, writeFile } from 'node:fs/promises';
import { join, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runBounded } from '../qualification/commands.mjs';
import { hashFile } from '../qualification/source.mjs';
import { flutterInputs } from './flutter-inputs.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const mobile = join(workspace, 'clients/mobile');
const expected = JSON.parse(await readFile(new URL('./flutter-toolchain.json', import.meta.url), 'utf8'));
const [action, ...extra] = process.argv.slice(2);
const commands = new Map([
    ['storage-patch-source', ['pub', 'cache', 'add', 'flutter_secure_storage', '--version', '10.3.4']],
    ['pub', ['pub', 'get', '--offline']],
    ['analyze', ['analyze', '--no-pub']],
    ['test', ['test', '--no-pub', '--concurrency=1']],
    ['version', ['--version', '--machine']],
    ['build', ['build', 'apk', '--debug', '--no-pub', '--flavor', 'qualification', '--target=integration_test/vod_commands_test.dart', '--android-project-arg=kotlin.compiler.execution.strategy=in-process']],
    ['build-production', ['build', 'apk', '--debug', '--no-pub', '--flavor', 'production', '--android-project-arg=kotlin.compiler.execution.strategy=in-process']],
]);

async function main() {
    const format = action === 'format' || action === 'format-write';
    const fix = action === 'fix';
    const build = action === 'build' || action === 'build-production';
    if (!commands.has(action) && !format && !fix) throw new Error('Unknown managed Flutter action.');
    if (extra.length > 24 || extra.some((arg) => arg.length > 512 || !/^[a-zA-Z0-9_./=-]+$/.test(arg))) throw new Error('Invalid managed Flutter arguments.');
    if (action !== 'test' && !format && extra.length) throw new Error('This Flutter action accepts no extra arguments.');
    if (extra.some((arg) => arg.startsWith('-') || arg.includes('..') || isAbsolute(arg)
        || !/^(?:test|test_driver|lib|integration_test)\//.test(arg))) throw new Error('Only scoped Flutter source/test paths are allowed.');
    const properties = await readFile(join(mobile, 'android/local.properties'), 'utf8');
    const declared = properties.match(/^flutter\.sdk=(.+)$/m)?.[1]?.trim().replaceAll('\\\\', '\\');
    if (!declared || !isAbsolute(declared)) throw new Error('The existing explicit Flutter SDK is required.');
    const sdk = await realpath(declared);
    const version = JSON.parse(await readFile(join(sdk, 'bin/cache/flutter.version.json'), 'utf8'));
    if (version.frameworkVersion !== expected.flutter || version.dartSdkVersion !== expected.dart || version.frameworkRevision !== expected.revision) throw new Error('The qualified Flutter/Dart identity changed.');
    const dart = join(sdk, 'bin/cache/dart-sdk/bin/dart.exe');
    const snapshot = join(sdk, 'bin/cache/flutter_tools.snapshot');
    const inputs = await flutterInputs(workspace);
    const tools = { dart: await hashFile(dart), flutter: await hashFile(snapshot), version };
    const args = ['--old_gen_heap_size=1536', ...(fix ? ['fix', '--apply', '--code=curly_braces_in_flow_control_structures'] : format
        ? ['format', ...(action === 'format' ? ['--output=none', '--set-exit-if-changed'] : []), ...extra]
        : [snapshot, '--suppress-analytics', ...commands.get(action), ...extra])];
    const environment = { ...process.env, FLUTTER_ROOT: sdk, CI: 'true', FLUTTER_SUPPRESS_ANALYTICS: 'true', DART_SUPPRESS_ANALYTICS: 'true',
        GRADLE_OPTS: '-Dorg.gradle.daemon=false -Dorg.gradle.workers.max=1 -Dorg.gradle.jvmargs="-Xmx3072m -XX:MaxMetaspaceSize=1024m -XX:ReservedCodeCacheSize=256m"',
        JAVA_OPTS: '-Xmx256m', JAVA_TOOL_OPTIONS: '', DEBUG: '' };
    delete environment.JAVA_TOOL_OPTIONS;
    const directory = join(workspace, '.cache/android-playback');
    await mkdir(directory, { recursive: true });
    const artifactPath = `clients/mobile/build/app/outputs/flutter-apk/app-${action === 'build-production' ? 'production' : 'qualification'}-debug.apk`;
    const artifact = join(workspace, artifactPath);
    const receipt = join(directory, action === 'build-production' ? 'apk-production-receipt.json' : 'apk-receipt.json');
    const graph = action === 'build-production' ? null : join(mobile, 'build/app/outputs/qualification/runtime-graph.json');
    let aapt;
    if (build) {
        const declaredAndroid = properties.match(/^sdk\.dir=(.+)$/m)?.[1]?.trim().replaceAll('\\\\', '\\').replaceAll('\\:', ':');
        if (!declaredAndroid || !isAbsolute(declaredAndroid)) throw new Error('An explicit Android SDK is required.');
        environment.ANDROID_HOME = environment.ANDROID_SDK_ROOT = await realpath(declaredAndroid);
        const java = await realpath(process.env.JAVA_HOME || 'C:/Program Files/Eclipse Adoptium/jdk-17.0.19.10-hotspot');
        if (!(await readFile(join(java, 'release'), 'utf8')).includes('JAVA_VERSION="17.0.19"')) throw new Error('The qualified JDK identity changed.');
        environment.JAVA_HOME = java;
        tools.java = await hashFile(join(java, 'bin/java.exe'));
        tools.android = await hashFile(join(environment.ANDROID_HOME, 'build-tools/36.0.0/source.properties'));
        aapt = join(environment.ANDROID_HOME, 'build-tools/36.0.0/aapt.exe');
        tools.aapt = await hashFile(aapt);
        environment.GRADLE_USER_HOME = join(directory, 'gradle');
        await mkdir(environment.GRADLE_USER_HOME, { recursive: true });
        await writeFile(join(environment.GRADLE_USER_HOME, 'gradle.properties'),
            'org.gradle.daemon=false\norg.gradle.workers.max=1\norg.gradle.jvmargs=-Xmx3072m -XX:MaxMetaspaceSize=1024m -XX:ReservedCodeCacheSize=256m\nkotlin.compiler.execution.strategy=in-process\n');
        try {
            const saved = JSON.parse(await readFile(receipt, 'utf8'));
            if (saved.source === inputs.sha256 && JSON.stringify(saved.tools) === JSON.stringify(tools)
                && saved.apk === await hashFile(artifact) && saved.graph === (graph ? await hashFile(graph) : null)) {
                process.stdout.write('Reused verified completed APK. Runtime tests must still run fresh.\n');
                return;
            }
        } catch (error) { if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error; }
    }
    const execution = await runBounded(dart, args, { cwd: mobile, env: environment,
        log: join(directory, `${action}-${Date.now()}.log`), timeoutMs: build ? 1800000 : 600000, bytes: 16 * 1024 ** 2 });
    process.stdout.write(execution.stdout);
    process.stderr.write(execution.stderr);
    if (execution.code !== 0 || execution.signal) throw new Error(`Managed Flutter ${action} failed.`);
    if (!format && !fix && (await flutterInputs(workspace)).sha256 !== inputs.sha256) throw new Error('Flutter source changed during measured execution.');
    if (build) {
        const packageName = action === 'build-production' ? 'com.duskcue.mobile' : 'com.duskcue.mobile.qualification';
        const activity = action === 'build-production' ? 'MainActivity' : 'QualificationActivity';
        const badging = await runBounded(aapt, ['dump', 'badging', artifact], { cwd: mobile, timeoutMs: 10000, bytes: 1048576 });
        if (badging.code !== 0 || !badging.stdout.includes(`package: name='${packageName}'`)
            || !badging.stdout.includes(`launchable-activity: name='com.duskcue.mobile.${activity}'`)) {
            throw new Error('The actual APK package/launcher identity failed.');
        }
        process.stdout.write(`Verified APK package: ${packageName}.\n`);
    }
    if (build) await writeFile(receipt, JSON.stringify({ source: inputs.sha256, tools, apk: await hashFile(artifact), graph: graph ? await hashFile(graph) : null,
        artifact: artifactPath, observedAt: new Date().toISOString() }, null, 2));
}

main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
