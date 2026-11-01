// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { lstat, readFile, realpath } from 'node:fs/promises';
import { join, relative, isAbsolute } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { hashFile } from '../qualification/source.mjs';

export async function androidDriverInputs(workspace, sdk, dart) {
    const mobile = join(workspace, 'clients/mobile');
    const config = join(mobile, '.dart_tool/package_config.json');
    const configBytes = await readFile(config);
    if (configBytes.length > 1048576) throw new Error('Android host package configuration is oversized.');
    const parsed = JSON.parse(configBytes);
    if (parsed.configVersion !== 2 || !Array.isArray(parsed.packages) || parsed.packages.length > 512) throw new Error('Invalid Android host package configuration.');
    const roots = new Map();
    for (const pkg of parsed.packages) {
        if (!/^[a-z_][a-z0-9_]*$/.test(pkg.name) || roots.has(pkg.name) || pkg.packageUri !== 'lib/') throw new Error('Ambiguous Android host package mapping.');
        const root = new URL(pkg.rootUri.endsWith('/') ? pkg.rootUri : `${pkg.rootUri}/`, pathToFileURL(config));
        if (root.protocol !== 'file:' || root.search || root.hash) throw new Error('Android host packages must be local.');
        roots.set(pkg.name, await realpath(fileURLToPath(root)));
    }
    for (const name of ['integration_test', 'flutter_driver']) {
        if (roots.get(name) !== await realpath(join(sdk, 'packages', name))) throw new Error('Android SDK driver package was substituted.');
    }
    if (!/\nversion: 15\.2\.0\s*(?:\r?\n|$)/.test(await readFile(join(roots.get('vm_service'), 'pubspec.yaml'), 'utf8'))) throw new Error('Android host VM-service version differs from the locked stack.');
    const files = new Map(), visited = new Set();
    const add = async (path, label) => {
        const info = await lstat(path);
        if (!info.isFile() || info.isSymbolicLink() || info.size > 16777216 || files.size >= 4096) throw new Error('Android driver source inventory exceeded its regular-file bound.');
        files.set(label, await hashFile(path));
    };
    const visit = async (path, root, label) => {
        const actual = await realpath(path);
        const rel = relative(root, actual);
        if (rel.startsWith('..') || isAbsolute(rel)) throw new Error('Android driver source escaped its package.');
        if (visited.has(actual)) return;
        visited.add(actual); await add(path, label);
        const text = await readFile(path, 'utf8');
        for (const directive of text.matchAll(/^\s*(?:import|export|part)\s+(?!of\b)([^;]+);/gm)) {
            for (const quoted of directive[1].matchAll(/['"]([^'"\r\n]+)['"]/g)) {
                const uri = quoted[1];
                if (uri.startsWith('dart:')) continue;
                if (uri.startsWith('package:')) {
                    const match = uri.match(/^package:([a-z_][a-z0-9_]*)\/([a-zA-Z0-9_./-]+\.dart)$/);
                    const pkgRoot = match && roots.get(match[1]);
                    if (!pkgRoot) throw new Error('Unresolved Android driver package input.');
                    const metadata = `${match[1]}/pubspec.yaml`;
                    if (!files.has(metadata)) await add(join(pkgRoot, 'pubspec.yaml'), metadata);
                    await visit(join(pkgRoot, 'lib', match[2]), pkgRoot, `${match[1]}/lib/${match[2]}`);
                } else {
                    if (!/^[a-zA-Z0-9_./-]+\.dart$/.test(uri)) throw new Error('Unapproved Android driver import.');
                    const target = fileURLToPath(new URL(uri, pathToFileURL(path)));
                    await visit(target, root, `${label.slice(0, label.indexOf('/'))}/${relative(root, target).replaceAll('\\', '/')}`);
                }
            }
        }
    };
    await add(config, 'package_config.json');
    await add(join(mobile, 'pubspec.lock'), 'pubspec.lock');
    await visit(join(mobile, 'test_driver/vod_commands.dart'), mobile, 'mobile/test_driver/vod_commands.dart');
    const manifest = [...files].sort(([a], [b]) => a.localeCompare(b)).map(([path, sha256]) => ({ path, sha256 }));
    const executable = await hashFile(dart);
    return { config, files: manifest, dart: executable, sha256: createHash('sha256').update(JSON.stringify({ executable, manifest })).digest('hex') };
}
