// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { constants } from 'node:fs';
import { lstat, open } from 'node:fs/promises';

const architectures = Object.freeze({
    amd64: Object.freeze({ architecture: 'amd64', node: 'x64', runner: 'X64', platform: 'linux/amd64', machine: 62, rustHost: 'x86_64-unknown-linux-musl' }),
    arm64: Object.freeze({ architecture: 'arm64', node: 'arm64', runner: 'ARM64', platform: 'linux/arm64', machine: 183, rustHost: 'aarch64-unknown-linux-musl' }),
});

export function architectureSpec(architecture) {
    if (!Object.hasOwn(architectures, architecture)) throw new Error('Qualification supports only native Linux amd64 or arm64.');
    return architectures[architecture];
}

export function nativeArchitecture(nodeArchitecture, runnerArchitecture) {
    const specification = Object.values(architectures).find((entry) => entry.node === nodeArchitecture);
    if (!specification || specification.runner !== runnerArchitecture) throw new Error('Node and GitHub runner architectures do not match a supported native platform.');
    return specification;
}

export function assertNativeArchitecture(architecture, nodeArchitecture) {
    const specification = architectureSpec(architecture);
    if (specification.node !== nodeArchitecture) throw new Error('Qualification artifact architecture does not match the native process.');
    return specification;
}

export function validateImageArchitecture(info, architecture) {
    architectureSpec(architecture);
    if (info?.Os !== 'linux' || info.Architecture !== architecture) throw new Error('Qualification image does not match its native Linux platform.');
}

export function validateRustHost(output, architecture) {
    if (/^host: (\S+)$/m.exec(output)?.[1] !== architectureSpec(architecture).rustHost) throw new Error('Qualification Rust host does not match its native musl architecture.');
}

export function validateElfHeader(header, architecture) {
    const specification = architectureSpec(architecture);
    if (!Buffer.isBuffer(header) || header.length !== 64 || header.subarray(0, 4).toString('hex') !== '7f454c46'
        || header[4] !== 2 || header[5] !== 1 || header[6] !== 1 || ![2, 3].includes(header.readUInt16LE(16))
        || header.readUInt16LE(18) !== specification.machine || header.readUInt32LE(20) !== 1 || header.readUInt16LE(52) !== 64) throw new Error('Qualification artifact is not the expected native 64-bit little-endian ELF.');
}

export async function verifyElfArchitecture(path, architecture) {
    const before = await lstat(path);
    if (!before.isFile() || before.isSymbolicLink() || before.size < 64) throw new Error('Qualification ELF artifact is not a regular file with a complete header.');
    const file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW || 0));
    try {
        const current = await file.stat();
        if (!current.isFile() || current.dev !== before.dev || current.ino !== before.ino) throw new Error('Qualification ELF artifact changed while opening.');
        const header = Buffer.alloc(64);
        const { bytesRead } = await file.read(header, 0, header.length, 0);
        if (bytesRead !== header.length) throw new Error('Qualification ELF header was truncated.');
        validateElfHeader(header, architecture);
    } finally { await file.close(); }
}
