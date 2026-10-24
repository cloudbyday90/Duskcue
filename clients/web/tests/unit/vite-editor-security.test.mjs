import test from 'node:test';
import assert from 'node:assert/strict';
import { bundledEditor, editorFixture } from '../fixtures/vite-editor.mjs';

for (const client of ['web', 'desktop']) {
    const source = await bundledEditor(client);

    test(`${client}: Windows canonicalizes after stripping line and column`, () => {
        for (const input of ['C:\\fixture\\missing\\..:1:1', 'file:///C:/fixture/missing/..:1:1']) {
            const fixture = editorFixture(source, { files: ['C:\\fixture'] });
            fixture.launch(input);
            assert.deepEqual(fixture.checked, ['C:\\fixture']);
            assert.equal(fixture.launches.length, 1);
            assert.equal(fixture.launches[0].args[0], 'fixture-editor C:\\fixture');
        }
    });

    test(`${client}: Windows refuses percent expansion and line breaks before the process sink`, () => {
        const files = ['literal%EDITOR_FIXTURE%.svelte', 'line\nbreak.svelte', 'line\rbreak.svelte'].map((name) => `C:\\fixture\\${name}`);
        const inputs = [...files.map((file) => `${file}:12:3`), 'file:///C:/fixture/literal%25EDITOR_FIXTURE%25.svelte:12:3'];
        for (const input of inputs) {
            const fixture = editorFixture(source, { files });
            fixture.launch(input);
            assert.equal(fixture.launches.length, 0, input);
            assert.equal(fixture.errors.length, 1, input);
        }
    });

    test(`${client}: Windows preserves normal Svelte paths and file URLs with positions`, () => {
        const file = 'C:\\fixture\\src\\routes\\[id]\\+page.svelte';
        for (const input of [`${file}:12:3`, 'file:///C:/fixture/src/routes/[id]/+page.svelte:12:3']) {
            const fixture = editorFixture(source, { files: [file], editor: 'code' });
            fixture.launch(input);
            assert.deepEqual(fixture.checked, [file]);
            assert.equal(fixture.errors.length, 0);
            assert.equal(fixture.launches.length, 1);
            assert.equal(fixture.launches[0].args[0], `code -r -g ${file}:12:3`);
        }
    });

    test(`${client}: WSL makes plus and minus filenames explicit relative arguments`, () => {
        for (const name of ['+fixture.svelte', '-fixture.svelte']) {
            const file = `/mnt/c/fixture/${name}`;
            for (const input of [file, `${file}:12:3`, `file:///mnt/c/fixture/${name}:12:3`]) {
                const fixture = editorFixture(source, { platform: 'linux', wsl: true, files: [file], editor: 'code' });
                fixture.launch(input);
                assert.equal(fixture.errors.length, 0);
                assert.equal(fixture.launches.length, 1);
                assert.equal(fixture.launches[0].kind, 'spawn');
                assert.deepEqual(Array.from(fixture.launches[0].args[1]), input === file ? [`./${name}`] : ['-r', '-g', `./${name}:12:3`]);
            }
        }
    });

    test(`${client}: ordinary Linux paths remain absolute without WSL conversion`, () => {
        const file = '/mnt/c/fixture/src/+page.svelte';
        const fixture = editorFixture(source, { platform: 'linux', files: [file] });
        fixture.launch(file);
        assert.equal(fixture.errors.length, 0);
        assert.equal(fixture.launches.length, 1);
        assert.deepEqual(Array.from(fixture.launches[0].args[1]), [file]);
    });

    test(`${client}: missing files and Windows UNC paths never launch`, () => {
        const fixture = editorFixture(source);
        fixture.launch('C:\\fixture\\missing.svelte');
        fixture.launch('\\\\fixture.invalid\\share\\page.svelte');
        assert.equal(fixture.launches.length, 0);
        assert.deepEqual(fixture.checked, ['C:\\fixture\\missing.svelte']);
    });
}
