import { createWriteStream } from 'node:fs';
import { finished } from 'node:stream/promises';

export function captureNativeLog(child, path) {
    const output = createWriteStream(path, { flags: 'a', highWaterMark: 64 * 1024 });
    let error;
    output.on('error', (failure) => { error = failure; });
    child.stdout?.pipe(output, { end: false });
    child.stderr?.pipe(output, { end: false });
    return {
        async close() {
            child.stdout?.unpipe(output);
            child.stderr?.unpipe(output);
            output.end();
            const complete = finished(output).catch((failure) => { error ||= failure; });
            let timer;
            try {
                await Promise.race([complete, new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Native log did not finish within five seconds.')), 5_000); })]);
                if (error) throw error;
            } finally { clearTimeout(timer); }
        },
    };
}
