import { readFile, realpath, writeFile } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createGuardedDocker } from './guarded-docker.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const image = 'sha256:0cb8275be5dfb536c07d7433b15e2ba9fa03bc912a9ea884a30ab784ac5e3a5d';
let workflow;

async function container(directory, executable, args, mountedFonts = false) {
    return workflow.container(image, executable, args, {
        directory,
        mounts: mountedFonts ? [`type=bind,source=${join(workspace, 'server/assets/fonts')},target=/diagnostic-fonts,readonly`] : [],
    });
}

async function frame(directory, name, subtitle, mountedFonts) {
    const filter = subtitle
        ? `subtitles=filename=/fixtures/source.mkv:si=1${mountedFonts ? ':fontsdir=/diagnostic-fonts:force_style=FontName=Inter' : ''}`
        : 'null';
    const result = await container(directory, 'ffmpeg', [
        '-hide_banner', '-loglevel', 'verbose', '-i', '/fixtures/source.mkv',
        '-map', '0:v:0', '-vf', filter, '-ss', '3', '-frames:v', '1',
        '-an', '-sn', '-threads', '1', '-filter_threads', '1',
        '-c:v', 'rawvideo', '-pix_fmt', 'rgb24', '-f', 'rawvideo', '-y', `/fixtures/${name}.rgb`,
    ], mountedFonts);
    await writeFile(join(directory, `${name}.log`), result.stderr, 'utf8');
    if (result.code !== 0 || result.signal) throw new Error(`The ${name} diagnostic FFmpeg command failed.`);
    const pixels = await readFile(join(directory, `${name}.rgb`));
    if (pixels.length !== 320 * 180 * 3) throw new Error('The diagnostic frame dimensions are unexpected.');
    return { pixels, fontMessages: result.stderr.split(/\r?\n/).filter((line) => /font|glyph/i.test(line)) };
}

function differences(first, second) {
    let changedBytes = 0;
    for (let i = first.length / 2; i < first.length; i += 1) if (first[i] !== second[i]) changedBytes += 1;
    return changedBytes;
}

async function main() {
    const [path, ...extra] = process.argv.slice(2);
    if (extra.length || !path || process.platform !== 'win32') throw new Error('Provide one existing caption fixture directory on Windows.');
    const directory = await realpath(resolve(path));
    const cache = await realpath(join(workspace, '.cache', 'tonight-server-ffmpeg'));
    const scoped = relative(cache, directory);
    if (!scoped || scoped === '..' || scoped.startsWith(`..${sep}`) || isAbsolute(scoped)) throw new Error('The caption fixture is outside the ignored workspace cache.');
    await realpath(join(directory, 'source.mkv'));
    workflow = await createGuardedDocker(workspace);
    const inventory = await container(directory, 'find', ['/usr/share/fonts', '/usr/local/share/fonts', '-type', 'f']);
    const plain = await frame(directory, 'font-plain', false, false);
    const baseline = await frame(directory, 'font-baseline', true, false);
    const mounted = await frame(directory, 'font-mounted', true, true);
    const report = {
        timestamp: new Date().toISOString(), image,
        fontInventory: inventory.stdout.split(/\r?\n/).filter(Boolean),
        inventoryStatus: inventory.code, inventoryMessage: inventory.stderr.trim(),
        baselineCaptionChangedBytes: differences(plain.pixels, baseline.pixels),
        mountedCaptionChangedBytes: differences(plain.pixels, mounted.pixels),
        baselineFontMessages: baseline.fontMessages,
        mountedFontMessages: mounted.fontMessages,
    };
    await writeFile(join(directory, 'font-diagnostic-report.json'), `${JSON.stringify(report, null, 2)}\n`, 'utf8');
    console.log(JSON.stringify(report));
    if (report.baselineCaptionChangedBytes !== 0 || report.mountedCaptionChangedBytes <= 100) {
        throw new Error('The absent-font hypothesis was not established by this diagnostic.');
    }
}

try { await main(); }
catch (error) { console.error(error.message); process.exitCode = 1; }
finally {
    if (workflow) {
        try { await workflow.cleanup(); }
        catch (error) { console.error(error.message); process.exitCode = 1; }
    }
}
