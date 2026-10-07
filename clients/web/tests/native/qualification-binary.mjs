import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';

export async function inspectQualificationBinary(path, identifier) {
    const digest = createHash('sha256');
    const needle = Buffer.from(identifier, 'utf8');
    let tail = Buffer.alloc(0);
    let containsIdentifier = false;
    for await (const chunk of createReadStream(path, { highWaterMark: 64 * 1024 })) {
        digest.update(chunk);
        if (containsIdentifier) continue;
        const searchable = Buffer.concat([tail, chunk]);
        containsIdentifier = searchable.includes(needle);
        tail = searchable.subarray(Math.max(0, searchable.length - needle.length + 1));
    }
    return { containsIdentifier, sha256: digest.digest('hex') };
}
