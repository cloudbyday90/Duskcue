export class PlaybackReleaseError extends Error {
    constructor(code) {
        super(code);
        this.code = code;
    }
}

export function playbackContextKey(context) {
    if (typeof context?.userId !== 'string' || !context.userId || !context.serverOrigin) return null;
    try {
        const origin = new URL(context.serverOrigin);
        return ['http:', 'https:'].includes(origin.protocol) ? JSON.stringify([origin.origin, context.userId]) : null;
    } catch { return null; }
}

export function createPlaybackRelease({ readContext, drain, send, onChange = (_state) => {} }) {
    let revision = 0;
    let captured = null;
    let pending = null;
    let state = { phase: 'idle', error: null };

    function publish(phase, error = null) {
        state = { phase, error };
        onChange({ ...state });
    }

    function requireCurrent(record, token) {
        if (token !== revision || !record.contextKey || record.contextKey !== readContext()) {
            throw new DOMException('Playback context changed', 'AbortError');
        }
    }

    function request(snapshot = null, { retry = false } = {}) {
        if (pending) return pending;
        if (captured && state.phase === 'failed' && !retry) return Promise.reject(state.error);
        if (captured && snapshot?.sessionId && captured.sessionId !== snapshot.sessionId) return Promise.reject(new PlaybackReleaseError('RELEASE_PENDING'));
        if (!captured && !snapshot?.sessionId) return Promise.resolve();
        if (!captured) {
            const position = Number(snapshot.positionMs);
            captured = Object.freeze({
                sessionId: snapshot.sessionId,
                positionMs: Number.isFinite(position) ? Math.floor(Math.max(0, position)) : 0,
                cancelledBeforeStart: snapshot.cancelledBeforeStart === true,
                contextKey: snapshot.contextKey,
            });
        }
        const record = captured;
        const token = revision;
        publish('releasing');
        const operation = Promise.resolve().then(async () => {
            requireCurrent(record, token);
            await drain(record.sessionId);
            requireCurrent(record, token);
            return send(record, () => token === revision && record.contextKey === readContext());
        }).then((result) => {
            requireCurrent(record, token);
            captured = null;
            publish('idle');
            return result;
        }).catch((error) => {
            if (token === revision) publish('failed', error);
            throw error;
        }).finally(() => { if (pending === operation) pending = null; });
        pending = operation;
        return operation;
    }

    function invalidate() {
        revision += 1;
        captured = null;
        pending = null;
        publish('idle');
    }

    onChange({ ...state });
    return { request, invalidate, hasPending: () => !!captured, getState: () => ({ ...state }) };
}
