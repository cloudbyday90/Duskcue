import { readFile, realpath, writeFile } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createGuardedDocker } from './guarded-docker.mjs';
import { sha256 } from './run-cached-rust-tests.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const baseImage = 'sha256:0cb8275be5dfb536c07d7433b15e2ba9fa03bc912a9ea884a30ab784ac5e3a5d';
const samples = {
    latin: 'Caption: café — Ελληνικά — Привет',
    arabic: 'مرحبا بالعالم',
    cjk: '中文字幕 日本語字幕 한국어 자막',
};
const focusedPackages = ['font-noto', 'font-noto-arabic', 'font-noto-hebrew', 'font-noto-devanagari', 'font-noto-thai', 'font-noto-cjk'];
let workflow;
let provenance;
let provenancePath;

async function retainProvenance() {
    await writeFile(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`, 'utf8');
}

async function checked(args) {
    const result = await workflow.command(args);
    if (result.code !== 0 || result.signal) throw new Error(`The owned Docker operation failed: ${args[0]}.`);
    return result.stdout.trim();
}

async function size(image) {
    const bytes = Number(await checked(['image', 'inspect', '--format', '{{.Size}}', image]));
    if (!Number.isSafeInteger(bytes) || bytes <= 0) throw new Error('The runtime image size was invalid.');
    return bytes;
}

async function qualify(directory, image, name, variant) {
    const filter = name === 'plain' ? 'null' : `subtitles=filename=/fixtures/script-${name}.srt`;
    const result = await workflow.container(image, 'ffmpeg', [
        '-hide_banner', '-loglevel', 'verbose', '-i', '/fixtures/source.mkv',
        '-map', '0:v:0', '-vf', filter, '-ss', '3', '-frames:v', '1',
        '-an', '-sn', '-threads', '1', '-filter_threads', '1',
        '-c:v', 'rawvideo', '-pix_fmt', 'rgb24', '-f', 'rawvideo', '-y', `/fixtures/runtime-${variant}-${name}.rgb`,
    ], { directory });
    await writeFile(join(directory, `runtime-${variant}-${name}.log`), result.stderr, 'utf8');
    if (result.code !== 0 || result.signal) throw new Error(`Default caption rendering failed for ${name}.`);
    if (/fontselect: failed|failed to find any fallback|Failed to load fontconfig fonts/i.test(result.stderr)) {
        throw new Error(`The default font provider failed for ${name}.`);
    }
    const pixels = await readFile(join(directory, `runtime-${variant}-${name}.rgb`));
    if (pixels.length !== 320 * 180 * 3) throw new Error('The default-caption frame dimensions are unexpected.');
    return { pixels, fontSelections: result.stderr.split(/\r?\n/).filter((line) => /fontselect:|font provider/.test(line)) };
}

async function main() {
    const [path, variant = 'broad', ...extra] = process.argv.slice(2);
    if (!path || extra.length || !['broad', 'focused'].includes(variant) || process.platform !== 'win32') throw new Error('Provide one existing caption fixture directory and a broad/focused variant on Windows.');
    const directory = await realpath(resolve(path));
    const cache = await realpath(join(workspace, '.cache', 'tonight-server-ffmpeg'));
    const scoped = relative(cache, directory);
    if (!scoped || scoped === '..' || scoped.startsWith(`..${sep}`) || isAbsolute(scoped)) throw new Error('The runtime fixture is outside the ignored workspace cache.');
    await realpath(join(directory, 'source.mkv'));
    workflow = await createGuardedDocker(workspace);
    const before = await size(baseImage);
    const packages = variant === 'focused' ? focusedPackages : ['font-noto-all', 'font-noto-cjk'];
    const installed = await workflow.container(baseImage, 'apk', ['add', '--no-cache', ...packages], { retain: true });
    await writeFile(join(directory, `runtime-font-install-${variant}.log`), `${installed.stdout}\n${installed.stderr}`, 'utf8');
    if (installed.code !== 0 || installed.signal) throw new Error('The bounded runtime-font package install failed.');
    const ownership = JSON.parse(await checked(['inspect', '--format', '{{json .}}', installed.name]));
    if (!/^[a-f0-9]{64}$/.test(ownership.Id || '') || ownership.Image !== baseImage
        || ownership.Config?.Labels?.['duskcue.test.resource-id'] !== workflow.resourceId
        || ownership.State?.Running || ownership.State?.ExitCode !== 0
        || ownership.HostConfig?.Memory !== 536870912 || ownership.HostConfig?.MemorySwap !== 536870912
        || ownership.HostConfig?.NanoCpus !== 2000000000 || ownership.HostConfig?.PidsLimit !== 128) {
        throw new Error('The retained package container ownership or resource bounds changed.');
    }
    let fontConfiguration = null;
    if (variant === 'focused') {
        const config = await realpath(join(directory, 'focused-font-fallback.conf'));
        if (relative(directory, config) !== 'focused-font-fallback.conf') throw new Error('The candidate font configuration left the fixture directory.');
        fontConfiguration = { path: relative(workspace, config), sha256: await sha256(config) };
        await checked(['cp', config, `${ownership.Id}:/etc/fonts/conf.d/60-duskcue-sans-fallback.conf`]);
    }
    const tag = `duskcue-font-qualification:${workflow.resourceId}`;
    const image = await checked([
        'commit', '--change', 'LABEL duskcue.test.font-qualification=true', ownership.Id, tag,
    ]);
    if (!/^sha256:[a-f0-9]{64}$/.test(image)) throw new Error('The qualification image identity was invalid.');
    provenancePath = join(directory, `runtime-font-qualification-${variant}-${workflow.resourceId}.json`);
    provenance = {
        timestamp: new Date().toISOString(), resourceId: workflow.resourceId,
        baseImage, image, tag, installedContainerId: ownership.Id,
        variant, requestedPackages: packages, fontConfiguration,
        baseImageBytes: before, qualifiedImageBytes: null, addedImageBytes: null,
        packageVersions: null, systemFontFiles: null, scripts: {},
        mountedFonts: false, forcedFontFamily: false,
        fullProductionImageRebuild: false, status: 'image_committed',
    };
    await retainProvenance();
    const after = await size(image);
    provenance.qualifiedImageBytes = after;
    provenance.addedImageBytes = after - before;
    await retainProvenance();
    const versions = await workflow.container(image, 'apk', ['info', '-v']);
    if (versions.code !== 0 || versions.signal) throw new Error('Runtime package versions could not be read.');
    const packageVersions = versions.stdout.split(/\r?\n/).filter((line) => /^(font|ffmpeg-|libass-|harfbuzz-|freetype-)/.test(line));
    provenance.packageVersions = packageVersions;
    await retainProvenance();
    const inventory = await workflow.container(image, 'find', ['/usr/share/fonts', '-type', 'f']);
    if (inventory.code !== 0 || inventory.signal) throw new Error('Runtime font inventory could not be read.');
    provenance.systemFontFiles = inventory.stdout.split(/\r?\n/).filter(Boolean).length;
    await retainProvenance();
    const plain = await qualify(directory, image, 'plain', variant);
    for (const [name, text] of Object.entries(samples)) {
        provenance.status = 'rendering_sample';
        provenance.currentSample = name;
        await retainProvenance();
        await writeFile(join(directory, `script-${name}.srt`), `1\n00:00:01,000 --> 00:00:07,000\n${text}\n\n`, 'utf8');
        const rendered = await qualify(directory, image, name, variant);
        let changedBytes = 0;
        for (let index = plain.pixels.length / 2; index < plain.pixels.length; index += 1) {
            if (plain.pixels[index] !== rendered.pixels[index]) changedBytes += 1;
        }
        provenance.scripts[name] = { text, changedBytes, fontSelections: rendered.fontSelections };
        await retainProvenance();
        if (changedBytes <= 100) throw new Error(`The ${name} default caption did not render visible glyphs.`);
        if (name === 'arabic' && !rendered.fontSelections.some((line) => /NotoSansArabic/.test(line))) {
            throw new Error('Arabic fallback did not select the installed Arabic text family.');
        }
    }
    provenance.status = 'samples_passed';
    delete provenance.currentSample;
    await retainProvenance();
    console.log(JSON.stringify(provenance));
}

try { await main(); }
catch (error) {
    console.error(error.message);
    process.exitCode = 1;
    if (provenance) {
        provenance.status = 'failed';
        provenance.error = error.message;
        await retainProvenance().catch(() => { console.error('The partial runtime provenance could not be retained.'); });
    }
}
finally {
    if (workflow) {
        try { await workflow.cleanup(); }
        catch (error) { console.error(error.message); process.exitCode = 1; }
    }
}
