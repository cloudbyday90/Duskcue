import { get } from 'svelte/store';
import { player as sharedPlayer } from '../stores/player.js';
import { viewingPreferences } from '../stores/viewing-preferences.js';
import { requiresAuthenticatedMediaSource } from '../api/core.js';
import { normalizePreferenceScope, scopeChangedError } from '../preferences/model.js';
import { DEFAULT_DEVICE_PREFERENCES, normalizeDevicePreferences } from '../preferences/device.js';
import { loadPlaybackEntry, playbackTitleDestination } from './entry.js';
import { normalizeTracks, resolveTrackSelection } from './tracks.js';

export class PlaybackRuntimeError extends Error {
    constructor(code) {
        super(code);
        this.code = code;
    }
}

function position(value) {
    return Number.isFinite(Number(value)) ? Math.max(0, Number(value)) : 0;
}

function cancelled() {
    return new DOMException('Playback request cancelled', 'AbortError');
}

function scopeMatches(scope, snapshot) {
    const actual = snapshot?.scope;
    return !!actual && snapshot.profileId === scope.profileId && actual.profileId === scope.profileId
        && actual.userId === scope.userId && actual.serverOrigin === scope.serverOrigin
        && (!scope.deviceId || actual.deviceId === scope.deviceId);
}

function trackFallback(tracks, selection, overrides) {
    const missingIdentity = (kind) => {
        const override = overrides[kind];
        return override?.mode === 'preferred' && !override.language
            && (override.fileId !== tracks.fileId || !(kind === 'audio' ? tracks.audio : tracks.subtitles)
                .some((track) => track.selectable && track.index === override.index));
    };
    return {
        audio: selection.audioFallback || missingIdentity('audio'),
        subtitle: selection.subtitleFallback || missingIdentity('subtitle'),
    };
}

export function createPlaybackRuntime({
    player = null,
    preferences = viewingPreferences,
    loadEntry = loadPlaybackEntry,
    getPlayerState = null,
    requiresAuthenticatedSource = requiresAuthenticatedMediaSource,
    onChange = (_state) => {},
} = {}) {
    player ||= sharedPlayer;
    getPlayerState ||= () => get(player);
    let state = {
        item: null, file: null, preferencesReady: false, autoplayEnabled: false,
        tracks: { fileId: null, audio: [], subtitles: [] }, selection: null,
        overrides: { audio: null, subtitle: null }, quality: { ...DEFAULT_DEVICE_PREFERENCES },
        fallback: { audio: false, subtitle: false }, error: null, loading: false, destination: '',
    };
    let disposed = false;
    let generation = 0;
    let operation;
    let scope;
    let context;
    let confirmedKey;
    let confirmedPreferences;
    let devicePreferences;
    let qualityOverride = null;
    let ownedSessionId = null;
    let lastRequest;

    function getState() {
        return { ...state, overrides: { ...state.overrides }, fallback: { ...state.fallback } };
    }

    function publish(update) {
        if (disposed) return;
        state = { ...state, ...update };
        onChange(getState());
    }

    function acceptPreferences(snapshot) {
        if (!scope || !confirmedKey || disposed) return;
        if (!scopeMatches(scope, snapshot) || snapshot.scope.key !== confirmedKey
            || !snapshot.preferences || !['ready', 'saving'].includes(snapshot.status)) {
            confirmedPreferences = null;
            operation?.controller.abort();
            publish({ preferencesReady: false, autoplayEnabled: false, loading: false, error: scopeChangedError() });
            return;
        }
        confirmedPreferences = snapshot.preferences;
        devicePreferences = snapshot.devicePreferences || DEFAULT_DEVICE_PREFERENCES;
        publish({ preferencesReady: true, autoplayEnabled: confirmedPreferences.autoplay_next_episode === true });
    }

    const unsubscribe = preferences.subscribe(acceptPreferences);

    function requireCurrent(current) {
        if (disposed || current.generation !== generation || current.controller.signal.aborted) throw cancelled();
        if (confirmedKey && (!confirmedPreferences || !scopeMatches(scope, preferences.getSnapshot())
            || preferences.getSnapshot().scope?.key !== confirmedKey)) throw scopeChangedError();
    }

    function requireOwnership(current, sessionId = null) {
        requireCurrent(current);
        const live = getPlayerState();
        const expected = sessionId || ownedSessionId || current.sessionId;
        if ((live?.sessionId || null) !== (expected || null)) throw cancelled();
        if (!sessionId && !current.initialLoading && live?.loading) throw cancelled();
    }

    function begin(signal) {
        if (disposed) throw cancelled();
        operation?.controller.abort();
        operation?.release();
        const controller = new AbortController();
        const abort = () => controller.abort();
        signal?.addEventListener('abort', abort, { once: true });
        if (signal?.aborted) controller.abort();
        const live = getPlayerState();
        operation = {
            generation: ++generation, controller, sessionId: live?.sessionId || null,
            initialLoading: live?.loading === true,
            release: () => signal?.removeEventListener('abort', abort),
        };
        publish({ loading: true, error: null });
        return operation;
    }

    async function loadPreferences(current, profileId, nextContext) {
        confirmedKey = null;
        confirmedPreferences = null;
        publish({ preferencesReady: false, autoplayEnabled: false });
        scope = normalizePreferenceScope(profileId, nextContext);
        context = { ...nextContext };
        const snapshot = await preferences.load(profileId, context);
        requireCurrent(current);
        if (!scopeMatches(scope, snapshot) || !snapshot.preferences
            || !['ready', 'saving'].includes(snapshot.status)) throw scopeChangedError();
        confirmedKey = snapshot.scope.key;
        acceptPreferences(snapshot);
        requireCurrent(current);
    }

    async function prepareEntry(current, request) {
        if (request.kind === 'start') {
            await loadPreferences(current, request.profileId, request.context);
            if (!request.revalidate) return request.entry;
            return loadEntry(request.entry.item.id, {
                fileId: request.entry.file.id, returnTo: request.entry.destination, signal: current.controller.signal,
            });
        }
        if (!scope || !confirmedPreferences || !state.preferencesReady) throw scopeChangedError();
        if (request.profileId && request.profileId !== scope.profileId) throw scopeChangedError();
        if (!state.item || !state.file) throw new PlaybackRuntimeError('ENTRY_UNAVAILABLE');
        requireOwnership(current);
        if (request.kind === 'transition') {
            if (state.item.type !== 'episode' || !state.item.series_id || !state.item.season_id
                || !request.nextEpisode?.id) throw new PlaybackRuntimeError('TRANSITION_UNAVAILABLE');
            const entry = await loadEntry(request.nextEpisode.id, {
                returnTo: state.destination, signal: current.controller.signal,
            });
            if (entry.item.id !== request.nextEpisode.id || entry.item.type !== 'episode'
                || entry.item.series_id !== state.item.series_id || entry.item.season_id !== state.item.season_id) {
                throw new PlaybackRuntimeError('TRANSITION_UNAVAILABLE');
            }
            return entry;
        }
        const entry = await loadEntry(state.item.id, {
            fileId: state.file.id, returnTo: state.destination, signal: current.controller.signal,
        });
        requireOwnership(current);
        return { ...entry, startPositionMs: position(getPlayerState()?.positionMs) };
    }

    async function execute(request, signal = undefined) {
        const current = begin(signal);
        lastRequest = request;
        try {
            requireCurrent(current);
            const entry = await prepareEntry(current, request);
            requireOwnership(current);
            if (!entry?.item?.id || !['movie', 'episode'].includes(entry.item.type)
                || !entry.file?.id || entry.file.is_healthy !== true) throw new PlaybackRuntimeError('FILES_UNAVAILABLE');
            const tracks = normalizeTracks(entry.file);
            const overrides = request.kind === 'start' ? { audio: null, subtitle: null } : { ...state.overrides };
            const changes = request.changes || {};
            for (const kind of ['audio', 'subtitle']) {
                if (changes[kind] !== undefined) overrides[kind] = changes[kind];
            }
            const nextQualityOverride = changes.quality !== undefined
                ? changes.quality === null ? null : normalizeDevicePreferences(changes.quality)
                : request.kind === 'start' ? null : qualityOverride;
            const quality = normalizeDevicePreferences(nextQualityOverride || devicePreferences || DEFAULT_DEVICE_PREFERENCES);
            const selection = resolveTrackSelection(tracks, confirmedPreferences, overrides);
            const startPositionMs = position(entry.startPositionMs);
            const result = await player.play(entry.item, entry.file.id, {
                startPositionMs, signal: current.controller.signal,
                forceTranscode: selection.needsTranscode || requiresAuthenticatedSource(),
                qualityMode: quality.quality_mode, maxBitrate: quality.max_streaming_bitrate,
                audioStreamIndex: ['preferred', 'override'].includes(selection.audioSelectionMode) ? selection.audio_stream_index : null,
                subtitleStreamIndex: selection.subtitle_stream_index,
            });
            requireCurrent(current);
            if (!result?.session_id) throw new PlaybackRuntimeError('START_FAILED');
            requireOwnership(current, result.session_id);
            ownedSessionId = result.session_id;
            const decision = result.stream_decision || result.playback_type || getPlayerState()?.streamDecision;
            if (startPositionMs > 0 && ['transcode', 'direct_stream'].includes(decision)) {
                try {
                    const seekResult = await player.seek(startPositionMs, { signal: current.controller.signal });
                    requireOwnership(current, result.session_id);
                    if (!seekResult) throw new PlaybackRuntimeError('SEEK_FAILED');
                } catch (error) {
                    if (!disposed && current.generation === generation && !current.controller.signal.aborted
                        && getPlayerState()?.sessionId === result.session_id) player.setPlaying(false);
                    throw error;
                }
            }
            requireOwnership(current, result.session_id);
            qualityOverride = nextQualityOverride;
            publish({
                item: entry.item, file: entry.file, tracks, selection, overrides, quality,
                fallback: trackFallback(tracks, selection, overrides),
                destination: playbackTitleDestination(entry.item, entry.destination), loading: false, error: null,
            });
            return getState();
        } catch (error) {
            if (!disposed && current.generation === generation) publish({ loading: false, error });
            throw error;
        } finally {
            current.release();
            if (operation === current) operation = null;
        }
    }

    function start(entry, { profileId = undefined, context = undefined, signal = undefined } = {}) {
        return execute({ kind: 'start', entry, profileId, context }, signal);
    }

    function restart({ audio = undefined, subtitle = undefined, quality = undefined } = {}) {
        return execute({ kind: 'restart', changes: { audio, subtitle, quality } });
    }

    function transition(nextEpisode, { signal = undefined, profileId = undefined } = {}) {
        return execute({ kind: 'transition', nextEpisode, profileId }, signal);
    }

    function refreshPreferences() {
        acceptPreferences(preferences.getSnapshot());
        return getState();
    }

    function retry() {
        if (!lastRequest) return Promise.reject(new PlaybackRuntimeError('ENTRY_UNAVAILABLE'));
        return execute({ ...lastRequest, revalidate: lastRequest.kind === 'start' });
    }

    function dispose() {
        if (disposed) return;
        disposed = true;
        generation += 1;
        operation?.controller.abort();
        operation?.release();
        operation = null;
        unsubscribe();
    }

    onChange(getState());
    return { start, restart, transition, refreshPreferences, retry, getState, dispose };
}
