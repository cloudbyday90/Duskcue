// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function codecConfiguration(codec, input, sampleEntry = codec === 'hevc' ? 'hvc1' : 'av01') {
    const bytes = Buffer.from(input);
    if (!bytes.length || bytes.length > 65536) throw new Error('Invalid codec configuration bound.');
    if (codec === 'hevc') {
        if (!['hvc1', 'hev1'].includes(sampleEntry) || bytes.length < 23 || bytes[0] !== 1 || (bytes[13] & 0xf0) !== 0xf0 || (bytes[15] & 0xfc) !== 0xfc || (bytes[16] & 0xfc) !== 0xfc || (bytes[17] & 0xf8) !== 0xf8 || (bytes[18] & 0xf8) !== 0xf8) throw new Error('Invalid HEVC configuration record.');
        const profile = bytes[1] & 31;
        const bitDepth = 8 + (bytes[17] & 7);
        if (![1, 2].includes(profile) || ![8, 10].includes(bitDepth) || bitDepth !== 8 + (bytes[18] & 7) || (bytes[16] & 3) !== 1 || bytes[12] === 0) throw new Error('Unsupported HEVC fixture profile.');
        let compatibility = 0;
        const source = bytes.readUInt32BE(2);
        for (let bit = 0; bit < 32; bit++) compatibility = (compatibility * 2 + ((source >>> bit) & 1)) >>> 0;
        const constraints = [...bytes.subarray(6, 12)];
        while (constraints.at(-1) === 0) constraints.pop();
        let offset = 23;
        const arrays = new Set();
        for (let array = 0; array < bytes[22]; array++) {
            if (offset + 3 > bytes.length) throw new Error('Truncated HEVC parameter array.');
            const kind = bytes[offset++] & 63;
            const count = bytes.readUInt16BE(offset); offset += 2;
            if (!count || count > 64) throw new Error('Invalid HEVC parameter count.');
            for (let nal = 0; nal < count; nal++) {
                if (offset + 2 > bytes.length) throw new Error('Truncated HEVC parameter length.');
                const length = bytes.readUInt16BE(offset); offset += 2;
                if (length < 2 || offset + length > bytes.length || ((bytes[offset] >>> 1) & 63) !== kind) throw new Error('Invalid HEVC parameter payload.');
                offset += length;
            }
            arrays.add(kind);
        }
        if (offset !== bytes.length || ![32, 33, 34].every((kind) => arrays.has(kind))) throw new Error('Incomplete HEVC parameter sets.');
        const space = ['', 'A', 'B', 'C'][bytes[1] >>> 6];
        const tier = bytes[1] & 32 ? 'H' : 'L';
        const suffix = constraints.map((value) => value.toString(16).toUpperCase().padStart(2, '0')).join('.');
        return { codec, sampleEntry, codecString: `${sampleEntry}.${space}${profile}.${compatibility.toString(16).toUpperCase()}.${tier}${bytes[12]}${suffix ? `.${suffix}` : ''}`, profile, level: bytes[12], tier, bitDepth, configurationHex: bytes.toString('hex') };
    }
    if (codec === 'av1') {
        if (sampleEntry !== 'av01' || bytes.length < 5 || bytes[0] !== 0x81 || (bytes[3] & 0xe0) !== 0 || (!(bytes[3] & 0x10) && (bytes[3] & 15) !== 0)) throw new Error('Invalid AV1 configuration record.');
        const profile = bytes[1] >>> 5;
        const level = bytes[1] & 31;
        const tier = bytes[2] & 128 ? 'H' : 'M';
        const bitDepth = bytes[2] & 64 ? bytes[2] & 32 ? 12 : 10 : 8;
        if (profile !== 0 || level > 23 || ![8, 10].includes(bitDepth) || bytes[2] & 16 || !(bytes[2] & 8) || !(bytes[2] & 4) || level < 8 && tier === 'H' || bytes[2] & 32) throw new Error('Unsupported AV1 fixture configuration.');
        let offset = 4;
        let sequenceHeaders = 0;
        while (offset < bytes.length) {
            const header = bytes[offset++];
            const type = (header >>> 3) & 15;
            if (header & 0x81 || !(header & 2) || ![1, 5].includes(type) || sequenceHeaders === 0 && type !== 1) throw new Error('Invalid AV1 configuration OBU.');
            if (header & 4) {
                if (offset >= bytes.length || bytes[offset++] & 7) throw new Error('Invalid AV1 OBU extension.');
            }
            let length = 0;
            let shift = 0;
            let octet;
            do {
                if (offset >= bytes.length || shift >= 28) throw new Error('Invalid AV1 OBU length.');
                octet = bytes[offset++];
                length += (octet & 127) * 2 ** shift;
                shift += 7;
            } while (octet & 128);
            if (length < 1 || offset + length > bytes.length) throw new Error('Truncated AV1 configuration OBU.');
            if (type === 1) sequenceHeaders++;
            offset += length;
        }
        if (sequenceHeaders !== 1) throw new Error('Expected one AV1 sequence header OBU.');
        return { codec, sampleEntry, codecString: `av01.${profile}.${String(level).padStart(2, '0')}${tier}.${String(bitDepth).padStart(2, '0')}`, profile, level, tier, bitDepth, configurationHex: bytes.toString('hex') };
    }
    throw new Error('Unsupported reference codec.');
}

export function mp4CodecConfiguration(input) {
    const bytes = Buffer.from(input);
    if (bytes.length < 8 || bytes.length > 8388608) throw new Error('Invalid MP4 fixture bound.');
    const found = [];
    let boxes = 0;
    const walk = (start, end, depth, parent) => {
        if (depth > 10) throw new Error('MP4 fixture nesting exceeded its bound.');
        let offset = start;
        while (offset < end) {
            if (++boxes > 1024 || offset + 8 > end) throw new Error('Invalid MP4 fixture box.');
            let length = bytes.readUInt32BE(offset);
            const kind = bytes.toString('ascii', offset + 4, offset + 8);
            let header = 8;
            if (length === 1) {
                if (offset + 16 > end) throw new Error('Truncated extended MP4 box.');
                const wide = bytes.readBigUInt64BE(offset + 8);
                if (wide > BigInt(bytes.length)) throw new Error('Oversized MP4 fixture box.');
                length = Number(wide); header = 16;
            } else if (length === 0) length = end - offset;
            if (length < header || offset + length > end) throw new Error('Truncated MP4 fixture box.');
            const payload = offset + header;
            const next = offset + length;
            if (['moov', 'trak', 'mdia', 'minf', 'stbl'].includes(kind)) walk(payload, next, depth + 1, kind);
            else if (kind === 'stsd') {
                if (payload + 8 > next || bytes.readUInt32BE(payload) !== 0 || bytes.readUInt32BE(payload + 4) > 16) throw new Error('Invalid sample description.');
                walk(payload + 8, next, depth + 1, 'stsd');
            } else if (['hvc1', 'hev1', 'av01'].includes(kind) && parent === 'stsd') {
                if (payload + 78 > next) throw new Error('Truncated visual sample entry.');
                walk(payload + 78, next, depth + 1, kind);
            } else if (kind === 'hvcC' && ['hvc1', 'hev1'].includes(parent) || kind === 'av1C' && parent === 'av01') found.push(codecConfiguration(kind === 'hvcC' ? 'hevc' : 'av1', bytes.subarray(payload, next), parent));
            offset = next;
        }
    };
    walk(0, bytes.length, 0, 'root');
    if (found.length !== 1) throw new Error('Expected one actual MP4 video configuration.');
    return found[0];
}
