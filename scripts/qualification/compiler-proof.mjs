// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function validateCompilerDurability(text) {
    if (Buffer.byteLength(text) > 65536) throw new Error('Compiler durability evidence exceeds its bound.');
    const markers = text.split(/\r?\n/).filter((line) => line.startsWith('DUSKCUE_BUILD_DURABILITY='));
    if (markers.length !== 1) throw new Error('One completed compiler durability result is required.');
    const result = JSON.parse(markers[0].slice('DUSKCUE_BUILD_DURABILITY='.length));
    if (result.passed !== true || result.tests !== 17 || result.actual_cargo !== true || result.changed_and_reverted_backdated_source !== true) throw new Error('Actual Cargo/cache durability qualification is incomplete.');
    return result;
}
