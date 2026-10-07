import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

export function byteRangeResponse(body, range) {
    const headers = { 'accept-ranges': 'bytes', 'content-length': String(body.length) };
    if (!range) return { status: 200, headers, body };
    const match = /^bytes=(\d*)-(\d*)$/.exec(range);
    const suffix = match && !match[1] ? Number(match[2]) : null;
    const start = suffix === null ? Number(match?.[1]) : Math.max(0, body.length - suffix);
    const end = suffix === null && match?.[2] ? Math.min(Number(match[2]), body.length - 1) : body.length - 1;
    if (!match || (!match[1] && !match[2]) || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start > end || start >= body.length || suffix === 0) {
        return { status: 416, headers: { ...headers, 'content-range': `bytes */${body.length}`, 'content-length': '0' }, body: Buffer.alloc(0) };
    }
    const bytes = body.subarray(start, end + 1);
    return { status: 206, headers: { ...headers, 'content-length': String(bytes.length), 'content-range': `bytes ${start}-${end}/${body.length}` }, body: bytes };
}

export function createMediaResponder(media) {
    const buffers = new Map();
    return async function serve(route, name, contentType) {
        if (!buffers.has(name)) buffers.set(name, await readFile(join(media.directory, name)));
        const body = buffers.get(name);
        const response = byteRangeResponse(body, route.request().headers().range);
        return route.fulfill({ ...response, contentType });
    };
}
