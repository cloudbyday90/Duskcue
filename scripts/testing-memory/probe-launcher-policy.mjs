import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createGuardedDocker } from './guarded-docker.mjs';
import { sha256 } from './run-cached-rust-tests.mjs';
import { createHash } from 'node:crypto';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const image = 'sha256:0cb8275be5dfb536c07d7433b15e2ba9fa03bc912a9ea884a30ab784ac5e3a5d';
let workflow;
let provenance;
let reportPath;

async function main() {
    if (process.argv.length !== 2 || process.platform !== 'win32') throw new Error('The launcher checkpoint requires Windows memory workflow and no extra arguments.');
    workflow = await createGuardedDocker(workspace);
    const sourcePath = join(workspace, 'server/src/services/sandbox.rs');
    const sourceBytes = await readFile(sourcePath);
    const source = sourceBytes.toString('utf8');
    if (!source.includes('let abi = ABI::V3;') || !/SeccompFilter::new\(\s*rules,\s*SeccompAction::KillProcess,\s*SeccompAction::Allow,/.test(source)) {
        throw new Error('The source policy is no longer the recorded ABI3 allowlist.');
    }
    const vector = source.match(/let mut rules:[\s\S]*?=\s*vec!\[([\s\S]*?)\]\s*\.into_iter\(\)/)?.[1];
    if (!vector) throw new Error('The source syscall vector cannot be verified.');
    const syscalls = [];
    let cursor = 0;
    for (const match of vector.matchAll(/\(\s*libc::SYS_(\w+),\s*vec!\[\]\s*\)\s*,/g)) {
        if (vector.slice(cursor, match.index).trim()) throw new Error('The source vector contains a nonflat rule.');
        syscalls.push(match[1]);
        cursor = match.index + match[0].length;
    }
    if (vector.slice(cursor).trim()) throw new Error('The source vector contains an unverified rule or condition.');
    if (!source.includes('rules.insert(libc::SYS_arch_prctl, vec![]);')) throw new Error('The x86_64 source policy changed.');
    syscalls.push('arch_prctl');
    if (syscalls.length < 40 || new Set(syscalls).size !== syscalls.length || syscalls.includes('execve') || syscalls.includes('execveat')) throw new Error('The flat source syscall allowlist changed.');
    const directory = join(workspace, '.cache', 'tonight-launcher-probe', workflow.resourceId);
    const inputs = join(directory, 'input');
    const results = join(directory, 'results');
    await mkdir(inputs, { recursive: true });
    await mkdir(results, { recursive: true });
    await copyFile(join(workspace, 'scripts/testing-memory/launcher-policy-probe.py'), join(inputs, 'probe.py'));
    const policy = { abi: 3, landlockCompatibility: 'best_effort', mismatchAction: 'kill_process', matchAction: 'allow', sourceSha256: createHash('sha256').update(sourceBytes).digest('hex'), syscalls };
    await writeFile(join(inputs, 'policy.json'), `${JSON.stringify(policy, null, 2)}\n`, 'utf8');
    await writeFile(join(inputs, 'source.mkv'), 'owned source path fixture', 'utf8');
    reportPath = join(results, 'launcher-policy-report.json');
    provenance = {
        kind: 'source-policy-kernel-reproduction', actualServerBinaryExecuted: false, actualProductionLauncherProof: false,
        image, resourceId: workflow.resourceId, sourcePolicySha256: policy.sourceSha256,
        pythonProbeSha256: await sha256(join(inputs, 'probe.py')), policyManifestSha256: await sha256(join(inputs, 'policy.json')),
        status: 'prepared',
    };
    await writeFile(reportPath, `${JSON.stringify(provenance, null, 2)}\n`, 'utf8');
    const shell = 'apk add --no-cache python3 py3-libseccomp > /fixtures/install.log 2>&1 && apk info -v > /fixtures/package-versions.txt && sha256sum /usr/bin/ffmpeg /usr/local/bin/duskcue > /fixtures/checkpoint-binary-hashes.txt && exec python3 /probe/probe.py /probe/policy.json';
    const outcome = await workflow.container(image, '/bin/sh', ['-c', shell], {
        directory: results,
        mounts: [`type=bind,source=${inputs},target=/probe,readonly`],
    });
    await writeFile(join(results, 'checkpoint.log'), `${outcome.stdout}\n${outcome.stderr}`, 'utf8');
    const report = { ...provenance, ...JSON.parse(await readFile(reportPath, 'utf8')) };
    report.packageVersions = (await readFile(join(results, 'package-versions.txt'), 'utf8')).trim().split(/\r?\n/).filter((line) => /^(python3-|py3-libseccomp-|libseccomp-)/.test(line));
    report.checkpointBinaryHashes = (await readFile(join(results, 'checkpoint-binary-hashes.txt'), 'utf8')).trim().split(/\r?\n/);
    provenance = report;
    report.status = outcome.code === 0 ? 'observed' : 'failed';
    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
    console.log(JSON.stringify(report));
    if (outcome.code !== 0 || outcome.signal || !report.compatibilityConcernObserved || report.execSucceeded || report.signal !== 31) {
        throw new Error('The checkpoint did not establish enforced initial-exec denial.');
    }
}

try { await main(); }
catch (error) {
    console.error(error.message);
    process.exitCode = 1;
    if (provenance && reportPath) {
        provenance.status = 'failed';
        provenance.error = error.message;
        await writeFile(reportPath, `${JSON.stringify(provenance, null, 2)}\n`, 'utf8').catch(() => {});
    }
}
finally {
    if (workflow) {
        try { await workflow.cleanup(); }
        catch (error) { console.error(error.message); process.exitCode = 1; }
    }
}
