import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Script, createContext } from 'node:vm';

export async function bundledEditor(client) {
    const bundle = new URL(`../../../${client}/node_modules/vite/dist/node/chunks/node.js`, import.meta.url);
    const source = await readFile(bundle, 'utf8');
    const modules = [...source.matchAll(/\/\/#region[^\n]*launch-editor@[^/\r\n]+\/node_modules\/launch-editor\/[^\n]*\n([\s\S]*?)\/\/#endregion/g)].map((match) => match[1]);
    if (modules.length < 3) throw new Error(`Unrecognized bundled editor layout: ${client}`);
    return modules.join('\n');
}

export function editorFixture(source, { platform = 'win32', wsl = false, files = [], editor = 'fixture-editor' } = {}) {
    const cwd = platform === 'win32' ? 'C:\\fixture' : '/mnt/c/fixture';
    const implementation = platform === 'win32' ? path.win32 : path.posix;
    const fixturePath = {
        ...implementation,
        resolve: (...args) => implementation.resolve(cwd, ...args),
        relative: (from, to) => implementation.relative(implementation.resolve(cwd, from), implementation.resolve(cwd, to)),
    };
    const launches = [];
    const checked = [];
    const errors = [];
    const child = { on() { return this; }, kill() { throw new Error('Unexpected child termination'); } };
    const capture = (kind) => (...args) => { launches.push({ kind, args }); return child; };
    const imports = {
        fs: { existsSync(file) { checked.push(file); return files.includes(file); } },
        os: { release: () => wsl ? 'fixture-Microsoft' : 'fixture' },
        path: fixturePath,
        url: { fileURLToPath: (file) => fileURLToPath(file, { windows: platform === 'win32' }) },
        child_process: { exec: capture('exec'), spawn: capture('spawn') },
    };
    const context = createContext({
        __require(name) {
            if (!(name in imports)) throw new Error(`Unexpected bundled import: ${name}`);
            return imports[name];
        },
        __commonJSMin(factory) {
            let loaded;
            return () => {
                if (!loaded) { loaded = { exports: {} }; factory(loaded.exports, loaded); }
                return loaded.exports;
            };
        },
        require_picocolors: () => ({ red: (value) => value }),
        fixtureEditor: editor,
        process: { platform, cwd: () => cwd, env: {} },
        console: { log() {} },
    }, { codeGeneration: { strings: false, wasm: false } });
    new Script(`${source}\nrequire_guess = () => () => [fixtureEditor];\nglobalThis.launch = require_launch_editor();`).runInContext(context, { timeout: 1000 });
    return {
        launches, checked, errors,
        launch(file) {
            context.input = file;
            context.onError = (filename, message) => errors.push({ filename, message });
            new Script('launch(input, undefined, onError)').runInContext(context, { timeout: 1000 });
        },
    };
}
