import { describe, expect, it } from 'vitest';
import { Buffer } from 'node:buffer';
import { byteRangeResponse } from '../fixtures/media-response.mjs';

describe('real media fixture byte ranges', () => {
    const content = Buffer.from('0123456789');

    it('serves a complete body when the decoder makes an ordinary request', () => {
        expect(byteRangeResponse(content, undefined)).toEqual({ status: 200, headers: { 'accept-ranges': 'bytes', 'content-length': '10' }, body: content });
    });

    it('serves initial, bounded and suffix ranges with exact byte headers', () => {
        const first = byteRangeResponse(content, 'bytes=0-');
        expect(first).toMatchObject({ status: 206, headers: { 'content-range': 'bytes 0-9/10', 'content-length': '10' } });
        expect(byteRangeResponse(content, 'bytes=3-5').body.toString()).toBe('345');
        expect(byteRangeResponse(content, 'bytes=-3').body.toString()).toBe('789');
        expect(byteRangeResponse(content, 'bytes=8-99').headers['content-range']).toBe('bytes 8-9/10');
    });

    it.each(['bytes=10-', 'bytes=6-2', 'bytes=-0', 'bytes=-', 'bytes=0-1,4-5', 'wrong'])('rejects an unsatisfiable or unsupported range %s', (range) => {
        const result = byteRangeResponse(content, range);
        expect(result.status).toBe(416);
        expect(result.headers['content-range']).toBe('bytes */10');
        expect(result.body.length).toBe(0);
    });
});
