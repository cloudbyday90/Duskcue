export class EventStreamFramingError extends Error {
    constructor() { super('The event stream exceeded its frame limit.'); this.code = 'FRAME_LIMIT'; }
}

export function createEventStreamParser({ onEvent = (_event) => {}, onId = (_id) => {}, onRetry = (_retry) => {}, maxFrameChars = 1024 * 1024 } = {}) {
    let line = '';
    let data = '';
    let type = '';
    let id = '';
    let skipLf = false;
    let first = true;
    let frameChars = 0;
    let disposed = false;

    function processLine() {
        const value = line;
        line = '';
        if (!value) {
            const event = data ? { type: type || 'message', data: data.slice(0, -1), lastEventId: id } : null;
            data = '';
            type = '';
            frameChars = 0;
            onId(id);
            if (event) onEvent(event);
            return;
        }
        if (value.startsWith(':')) return;
        const colon = value.indexOf(':');
        const field = colon === -1 ? value : value.slice(0, colon);
        let content = colon === -1 ? '' : value.slice(colon + 1);
        if (content.startsWith(' ')) content = content.slice(1);
        if (field === 'data') data += `${content}\n`;
        else if (field === 'event') type = content;
        else if (field === 'id' && !content.includes('\0')) id = content;
        else if (field === 'retry' && /^[0-9]+$/.test(content)) onRetry(content);
    }

    return {
        push(text) {
            if (disposed) return;
            for (const character of text) {
                if (disposed) break;
                if (first) { first = false; if (character === '\uFEFF') continue; }
                if (skipLf) { skipLf = false; if (character === '\n') continue; }
                if (character === '\r' || character === '\n') {
                    processLine();
                    skipLf = character === '\r';
                } else {
                    frameChars += character.length;
                    if (frameChars > maxFrameChars) throw new EventStreamFramingError();
                    line += character;
                }
            }
        },
        dispose() { disposed = true; line = ''; data = ''; type = ''; id = ''; },
    };
}
