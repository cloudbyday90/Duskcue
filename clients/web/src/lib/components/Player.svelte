<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { onMount, onDestroy, tick } from 'svelte';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { createPlaybackRuntime } from '../playback/runtime.js';
    import { createAutoplayController } from '../playback/autoplay.js';
    import { createPlaybackAuthorizationRecovery } from '../playback/authorization.js';
    import { createCaptionLayout } from '../playback/caption-layout.js';
    import { playbackTrackChoices, playbackQualityChoices } from '../playback/choices.js';
    import { createTrackOverride } from '../playback/tracks.js';
    import { loadTitleEpisodes } from '../media/title-data.js';
    import PlayerMenus from './PlayerMenus.svelte';
    import AutoplayCard from './AutoplayCard.svelte';
    import { createControlsVisibility } from '../playback/visibility.js';
    import { createFullscreenController } from '../playback/fullscreen.js';
    import {
        player,
        isPlaying,
        isBuffering,
        streamUrl,
        streamDecision,
        currentMediaItem,
        playerVolume,
        playerError,
        playerLoading,
    } from '../stores/player.js';
    import { preferences } from '../stores/user.js';
    import { createMediaSource } from '../playback/source.js';
    import { canCompleteSource, sourcePlaybackDuration } from '../playback/timing.js';
    import { createPlaybackTelemetry } from '../playback/telemetry.js';
    import { createPlaybackAnnotations } from '../playback/annotations.js';
    import { formatTimestamp, formatDuration } from '../utils/format.js';
    import { PLAYER_SEEK_STEP_S, PLAYER_VOLUME_STEP } from '../utils/constants.js';
    import SkipButton from './SkipButton.svelte';
    import SeekPreview from './SeekPreview.svelte';

    let {
        mediaItem = null,
        entry = null,
        profileId = null,
        preferenceContext = null,
        onactivechange = (_state) => {},
        mediaFileId = null,
        startPositionMs = 0,
        sessionId = null,
        title = null,
        onstop = null,
        onexitintent = (_intent) => {},
    } = $props();

    let videoEl = null;
    let videoStage = null;
    let containerEl = null;
    let mediaSource;
    let sourceTimeline = $state(null);
    let captionLayout;

    let isMounted = $state(false);
    let controlsVisible = $state(true);
    let isSeeking = $state(false);
    let seekValue = $state(0);
    let bufferedPercent = $state(0);
    let controlsHovered = $state(false);
    let controlsFocused = $state(false);
    let fullscreenError = $state(false);
    let fullscreenController;
    let closing = $state(false);
    let exitRequested = $state(false);
    let closeFailed = $state(false);
    let closePromise = null;
    let closeRetryButton = $state();
    let sourceError = $state(false);
    let playbackState = $state(null);
    let autoplayState = $state(null);
    let menuOpen = $state(false);
    let episodes = $state([]);
    let episodesController;
    let episodeSeasonId;
    let episodeItemId;
    let autoplayFocused = $state(false);
    let menus;
    let retryButton = $state();
    let trackStatus = $derived(playbackState?.fallback?.audio || playbackState?.fallback?.subtitle ? m.tonight_player_track_fallback() : '');
    const runtime = createPlaybackRuntime({ player, onChange: (state) => { playbackState = state; } });
    const authorization = createPlaybackAuthorizationRecovery();
    const autoplay = createAutoplayController({ onChange: (state) => { autoplayState = state; }, onPlayNext: async (episode, options) => {
        if ($player.release?.phase === 'failed') {
            if (options.mode !== 'manual') throw $player.release.error;
            await player.retryRelease();
        }
        return runtime.transition(episode, options);
    } });
    let tracks = $derived(playbackTrackChoices(playbackState?.tracks || { audio: [], subtitles: [] }, playbackState?.selection, {
        locale: getLocale(), source: m.tonight_player_source_audio(), off: m.tonight_player_subtitles_off(), unknown: m.tonight_player_unknown_language(), description: m.tonight_player_audio_description(), sdh: m.tonight_player_sdh(), forced: m.tonight_player_forced(), unsupported: m.tonight_player_track_unsupported(),
        messageLocales: { source: messageLocale('tonight_player_source_audio'), off: messageLocale('tonight_player_subtitles_off'), unknown: messageLocale('tonight_player_unknown_language'), description: messageLocale('tonight_player_audio_description'), sdh: messageLocale('tonight_player_sdh'), forced: messageLocale('tonight_player_forced'), unsupported: messageLocale('tonight_player_track_unsupported') },
    }));
    let qualityChoices = $derived(playbackQualityChoices(playbackState?.quality, {
        auto: m.tonight_preferences_quality_auto(), maximum: m.tonight_preferences_quality_maximum(), bitrate: (bitrate) => m.tonight_player_quality_bitrate({ bitrate }),
        messageLocales: { auto: messageLocale('tonight_preferences_quality_auto'), maximum: messageLocale('tonight_preferences_quality_maximum'), bitrate: messageLocale('tonight_player_quality_bitrate') },
    }));
    let speedChoices = $derived([0.5, 0.75, 1, 1.25, 1.5, 2].map((value) => ({ value: String(value), label: `${value}×`, selected: $player.playbackRate === value })));
    let episodeChoices = $derived(episodes.map((episode) => ({ id: episode.id, locale: Number.isInteger(episode.episode_number) ? messageLocale('tonight_player_episode_label') : getLocale(), label: Number.isInteger(episode.episode_number) ? m.tonight_player_episode_label({ number: episode.episode_number, title: episode.title }) : episode.title, synopsis: episode.overview, selected: episode.id === playbackState?.item?.id, disabled: episode.availability?.can_play !== true })));
    const visibility = createControlsVisibility({ onChange: (visible) => { controlsVisible = visible; } });
    const telemetry = createPlaybackTelemetry({ player });
    const annotations = createPlaybackAnnotations({ onChange: (state) => { segments = state.segments; storyboard = state.storyboard; } });
    let segments = $state([]);
    let storyboard = $state(null);
    let isSeekHovering = $state(false);
    let isKeyboardSeeking = $state(false);
    let seekHoverRatio = $state(0);
    let seekHoverMs = $state(0);

    const SEGMENT_CONFIDENCE_THRESHOLD = 0.7;
    const SEGMENT_AUTO_SKIP_TYPES = ['intro', 'credits', 'recap', 'preview', 'outro'];

    let displayTitle = $derived(title || playbackState?.item?.title || $currentMediaItem?.title || mediaItem?.title || 'Playing');
    let controlsDisabled = $derived(exitRequested || closing || $player.sessionReleased || ($player.release?.phase || 'idle') !== 'idle');
    let closeRecovery = $derived(closeFailed || ($player.release?.phase === 'failed' && (!autoplayState || ['idle', 'none', 'transitioned'].includes(autoplayState.phase))));

    let surfaceableSegments = $derived(
        segments.filter((seg) => seg.is_manual || (seg.confidence ?? 0) >= SEGMENT_CONFIDENCE_THRESHOLD),
    );

    let autoSkipTypes = $derived.by(() => {
        const prefs = $preferences;
        if (!prefs) return [];
        return SEGMENT_AUTO_SKIP_TYPES.filter((type) => prefs[`autoSkip${type.charAt(0).toUpperCase() + type.slice(1)}`]);
    });

    onMount(async () => {
        isMounted = true;
        if (preferenceContext) player.setContext(preferenceContext);
        containerEl?.focus();
        captionLayout = createCaptionLayout({ container: containerEl, stage: videoStage, video: videoEl, readObstructions: () => [...containerEl.querySelectorAll('.player-heading, .player-controls, .track-fallback:not(:empty), .player-popover:not([hidden]), .autoplay-card, .fullscreen-error, .skip-button, .seek-preview')] });
        mediaSource = createMediaSource({ video: videoEl, onError: () => { sourceError = true; player.setPlaying(false); }, onPlaybackBlocked: () => player.setPlaying(false), onTimelineChange: (timeline) => {
            if (!isMounted) return;
            sourceTimeline = timeline;
            if (timeline.mode === 'hls-mse' && timeline.complete !== null) handleDurationChange();
        } });
        fullscreenController = createFullscreenController({ element: containerEl, onChange: (state) => {
            player.setFullscreen(state.fullscreen);
            fullscreenError = !!state.error;
        } });
        window.addEventListener('duskcue:desktop-playback-toggle', handleDesktopPlaybackToggle);
        document.addEventListener('visibilitychange', handleDocumentVisibility);
        handleDocumentVisibility();

        try {
            if (sessionId) await player.resume(sessionId);
            else if (entry) await runtime.start(entry, { profileId, context: preferenceContext });
            else if (mediaItem && mediaFileId) await player.play(mediaItem, mediaFileId, { startPositionMs });
        } catch {}
        if (!isMounted) return;
        telemetry.start();
    });

    onDestroy(() => {
        isMounted = false;
        runtime.dispose();
        authorization.dispose();
        captionLayout?.dispose();
        autoplay.dispose();
        episodesController?.abort();
        document.removeEventListener('visibilitychange', handleDocumentVisibility);
        window.removeEventListener('duskcue:desktop-playback-toggle', handleDesktopPlaybackToggle);
        mediaSource?.dispose();
        annotations.dispose();
        visibility.dispose();
        telemetry.dispose();
        fullscreenController?.dispose();
        player.stop().catch(() => {});
        player.destroy();
    });

    let lastAttachedUrl = null;
    $effect(() => {
        const subtitle = playbackState?.selection?.subtitle;
        const enabled = Number.isInteger(subtitle?.index) && subtitle.index >= 0;
        controlsVisible; menuOpen; autoplayState?.phase; fullscreenError; seekPreviewVisible; seekHoverRatio;
        tick().then(() => { if (isMounted) captionLayout?.update(enabled); });
    });
    $effect(() => {
        if (!isMounted || exitRequested || closing || playbackState?.loading || $playerLoading || $player.sessionReleased || $player.release?.phase === 'failed') return;
        const url = $streamUrl;
        if (url && url !== lastAttachedUrl) {
            lastAttachedUrl = url;
            sourceError = false;
            mediaSource?.attach(url);
        }
    });

    $effect(() => {
        autoplay.setPreference({ autoplayEnabled: playbackState?.autoplayEnabled === true, preferencesReady: playbackState?.preferencesReady === true });
        if (!isMounted || !playbackState?.item || playbackState.loading || playbackState.error) return;
        const state = playbackState;
        if (state.item.id !== episodeItemId) {
            episodeItemId = state.item.id;
            annotations.load(state.item.id, state.file.id);
            loadEpisodes(state.item);
            onactivechange(state);
        }
    });

    async function loadEpisodes(item) {
        episodesController?.abort();
        if (item.type !== 'episode' || !item.season_id) { episodes = []; episodeSeasonId = null; return; }
        if (episodeSeasonId !== item.season_id) episodes = [];
        episodeSeasonId = item.season_id;
        const request = new AbortController();
        episodesController = request;
        try {
            const result = await loadTitleEpisodes(item.season_id, { signal: request.signal });
            if (isMounted && !request.signal.aborted) episodes = result;
        } catch {}
    }

    function handleDocumentVisibility() { autoplay.setPaused({ documentHidden: document.hidden }); }
    function handleEnded(event) {
        const state = runtime.getState();
        if (!isMounted || closing || exitRequested || state.loading || state.error || $playerLoading
            || !event?.isTrusted || !videoEl?.ended || sourceTimeline?.url !== $streamUrl || !canCompleteSource(sourceTimeline)) return;
        updateDuration(true);
        if (Number.isFinite(videoEl?.currentTime)) player.setPosition(videoEl.currentTime * 1000 + ($player.streamOffsetMs || 0));
        player.setPlaying(false);
        autoplay.ended({ item: state.item, profileId, autoplayEnabled: state.autoplayEnabled, preferencesReady: state.preferencesReady });
    }
    function handleSeeked() {
        if (Number.isFinite(videoEl?.duration) && videoEl.currentTime < videoEl.duration - 0.25) autoplay.reset();
    }
    async function handleChoice(kind, value) {
        if (kind === 'speed') { player.setPlaybackRate(Number(value)); return; }
        if (kind === 'episode' && (value === playbackState?.item?.id || !episodes.some((item) => item.id === value && item.availability?.can_play))) return;
        autoplay.reset();
        sourceError = false;
        mediaSource?.release();
        lastAttachedUrl = null;
        try {
            if (kind === 'episode') {
                const episode = episodes.find((item) => item.id === value);
                if (episode && episode.id !== playbackState?.item?.id) await runtime.transition(episode, { profileId });
            } else if (kind === 'quality') {
                await runtime.restart({ quality: { quality_mode: ['auto', 'maximum'].includes(value) ? value : 'manual', max_streaming_bitrate: ['auto', 'maximum'].includes(value) ? null : Number(value) } });
            } else {
                const track = (kind === 'audio' ? playbackState.tracks.audio : playbackState.tracks.subtitles).find((row) => String(row.index) === value) || null;
                await runtime.restart({ [kind]: createTrackOverride(kind, track, playbackState.file.id) });
            }
        } catch {}
    }

    $effect(() => {
        if (videoEl && isMounted) {
            videoEl.volume = $playerVolume;
        }
    });

    $effect(() => {
        if (videoEl && isMounted) {
            videoEl.muted = $player?.isMuted ?? false;
        }
    });

    $effect(() => {
        if (videoEl && isMounted) {
            videoEl.playbackRate = $player?.playbackRate ?? 1;
        }
    });

    function handlePlay() {
        player.setPlaying(true);
    }

    function handlePause() {
        player.setPlaying(false);
    }

    function handleWaiting() {
        player.setBuffering(true);
        telemetry.waiting();
    }

    function handlePlaying() {
        player.setBuffering(false);
        telemetry.playing();
    }

    function handleTimeUpdate() {
        if (isMounted && !exitRequested && videoEl && !isSeeking && !$player.isBuffering) {
            player.setPosition(videoEl.currentTime * 1000 + ($player.streamOffsetMs || 0));
            updateBuffered();
        }
    }

    function handleDurationChange() {
        updateDuration();
    }

    function updateDuration(naturalEnded = false) {
        if (!isMounted || closing || exitRequested || !videoEl || sourceTimeline?.url !== $streamUrl || sourceTimeline?.mode === 'detached') return;
        const runtimeSeconds = playbackState?.item?.runtime_seconds || $currentMediaItem?.runtime_seconds || mediaItem?.runtime_seconds || playbackState?.file?.runtime_seconds;
        const duration = sourcePlaybackDuration({ source: sourceTimeline, mediaDurationSeconds: videoEl.duration, runtimeSeconds, streamOffsetMs: $player.streamOffsetMs, naturalEnded });
        if (duration > 0 && duration !== $player.durationMs) player.setDuration(duration);
    }

    function handleLoadedMetadata() {
        if (isMounted && !closing && !exitRequested && videoEl && sourceTimeline?.url === $streamUrl && sourceTimeline?.mode !== 'detached') {
            handleDurationChange();
            const position = $player.positionMs;
            if ($streamDecision === 'direct_play' && position > 0 && videoEl.currentTime === 0) videoEl.currentTime = position / 1000;
            for (const track of videoEl.textTracks) track.mode = 'disabled';
        }
    }

    function updateBuffered() {
        if (!videoEl || !videoEl.buffered.length || !durationMs) return;
        const end = videoEl.buffered.end(videoEl.buffered.length - 1);
        bufferedPercent = Math.max(0, Math.min(100, ((end * 1000 + ($player.streamOffsetMs || 0)) / durationMs) * 100));
    }

    function togglePlayPause() {
        if (!videoEl || controlsDisabled) return;
        if (videoEl.paused) {
            videoEl.play().catch(() => {});
        } else {
            videoEl.pause();
        }
    }

    function handleDesktopPlaybackToggle() {
        togglePlayPause();
    }

    function handleSeekInput(event) {
        seekValue = parseFloat(event.target.value);
        if (!isSeeking) {
            isKeyboardSeeking = true;
        }
    }

    function handleSeekStart() {
        autoplay.reset();
        isSeeking = true;
        isKeyboardSeeking = false;
        seekValue = $player?.positionMs || 0;
    }

    function handleSeekEnd() {
        isSeeking = false;
        isKeyboardSeeking = false;
        const positionMs = seekValue;
        const decision = $streamDecision;

        if (decision === 'direct_play' && videoEl) {
            videoEl.currentTime = positionMs / 1000;
            player.setPosition(positionMs);
        } else {
            player.seek(positionMs).catch(() => { sourceError = true; });
        }
    }

    function handleSeekBarMouseMove(event) {
        if (!durationMs) return;
        const rect = event.currentTarget.getBoundingClientRect();
        const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
        seekHoverRatio = ratio;
        seekHoverMs = ratio * durationMs;
        isSeekHovering = true;
    }

    function handleSeekBarTouchMove(event) {
        if (!durationMs || !event.touches || event.touches.length === 0) return;
        const rect = event.currentTarget.getBoundingClientRect();
        const touch = event.touches[0];
        const ratio = Math.max(0, Math.min(1, (touch.clientX - rect.left) / rect.width));
        seekHoverRatio = ratio;
        seekHoverMs = ratio * durationMs;
        isSeekHovering = true;
    }

    function handleSeekBarHoverEnd() {
        isSeekHovering = false;
    }

    function handleSkip(skipToMs) {
        if (!videoEl || skipToMs == null) return;
        autoplay.reset();
        const decision = $streamDecision;
        showControls();
        if (decision === 'direct_play') {
            const clamped = Math.max(0, Math.min(videoEl.duration || skipToMs / 1000, skipToMs / 1000));
            videoEl.currentTime = clamped;
            player.setPosition(clamped * 1000);
        } else {
            player.seek(skipToMs).catch(() => { sourceError = true; });
        }
    }

    function handleVolumeChange(event) {
        player.setVolume(parseFloat(event.target.value));
    }

    function toggleMute() {
        player.toggleMute();
    }

    async function toggleFullscreen() {
        try { await fullscreenController?.toggle(); } catch { fullscreenError = true; }
        showControls();
    }

    $effect(() => {
        visibility.setState({ playing: $isPlaying, seeking: isSeeking, hovered: controlsHovered, focusWithin: controlsFocused, disclosureOpen: menuOpen });
        autoplay.setPaused({ focusWithin: autoplayFocused, disclosureOpen: menuOpen });
    });

    function showControls() { visibility.activity(); }
    function handleMouseMove() { showControls(); }
    function handleMouseLeave() { controlsHovered = false; }

    function handleControlFocus(event) {
        const target = event.target;
        controlsFocused = target !== containerEl;
        showControls();
        if (!(target instanceof HTMLElement) || target === containerEl) return;
        if (target.closest('.player-popover')) return;
        tick().then(() => {
            if (isMounted && document.activeElement === target && containerEl?.contains(target) && containerEl.scrollHeight > containerEl.clientHeight) {
                target.scrollIntoView({ block: 'nearest', inline: 'nearest' });
            }
        });
    }


    function handleKeydown(event) {
        if (event.key === 'Escape') {
            if (menus?.close(true)) { event.preventDefault(); return; }
            if ($player?.isFullscreen) fullscreenController?.exit().catch(() => { fullscreenError = true; });
            showControls();
            return;
        }
        if (!containerEl?.contains(document.activeElement)) return;
        showControls();
        if (event.altKey || event.ctrlKey || event.metaKey || event.target?.closest?.('input, select, textarea, button, a, [contenteditable="true"]')) {
            return;
        }
        switch (event.key) {
            case ' ':
            case 'k':
                event.preventDefault();
                togglePlayPause();
                break;
            case 'ArrowLeft':
                autoplay.reset();
                event.preventDefault();
                if (videoEl) {
                    const newPos = Math.max(0, $player.positionMs - PLAYER_SEEK_STEP_S * 1000);
                    if ($streamDecision === 'direct_play') videoEl.currentTime = newPos / 1000;
                    else player.seek(newPos).catch(() => { sourceError = true; });
                }
                break;
            case 'ArrowRight':
                autoplay.reset();
                event.preventDefault();
                if (videoEl) {
                    const newPos = Math.min(durationMs || Infinity, $player.positionMs + PLAYER_SEEK_STEP_S * 1000);
                    if ($streamDecision === 'direct_play') videoEl.currentTime = newPos / 1000;
                    else player.seek(newPos).catch(() => { sourceError = true; });
                }
                break;
            case 'ArrowUp':
                event.preventDefault();
                player.setVolume(Math.min(1, ($playerVolume || 0) + PLAYER_VOLUME_STEP));
                break;
            case 'ArrowDown':
                event.preventDefault();
                player.setVolume(Math.max(0, ($playerVolume || 0) - PLAYER_VOLUME_STEP));
                break;
            case 'f':
                toggleFullscreen();
                break;
            case 'm':
                toggleMute();
                break;
        }
        showControls();
    }

    export function requestClose({ retry = false } = {}) {
        if (closePromise) return closePromise;
        const heldFocus = containerEl?.contains(document.activeElement);
        const requestedContext = preferenceContext ? { ...preferenceContext } : null;
        if (!exitRequested && videoEl?.readyState > 0 && Number.isFinite(videoEl.currentTime)) player.setPosition(videoEl.currentTime * 1000 + ($player.streamOffsetMs || 0));
        exitRequested = true;
        closing = true;
        closeFailed = false;
        autoplay.reset();
        menus?.close(false);
        mediaSource?.release();
        lastAttachedUrl = null;
        telemetry.dispose();
        showControls();
        const operation = (async () => {
            try {
                await fullscreenController?.exit().catch(() => {});
                await player.stop({ retry });
                runtime.dispose();
                autoplay.dispose();
                annotations.dispose();
                return true;
            } catch (error) {
                if (error?.status === 401) {
                    const recovery = await authorization.recover(error, requestedContext);
                    if (recovery.status === 'expired' || recovery.status === 'superseded') return false;
                }
                closeFailed = true;
                return false;
            } finally {
                closing = false;
                await tick();
                if (isMounted && closeFailed && (heldFocus || document.activeElement === document.body)) closeRetryButton?.focus();
            }
        })();
        closePromise = operation;
        operation.then(() => { if (closePromise === operation) closePromise = null; });
        return operation;
    }

    async function handleClose() {
        onexitintent('title');
        if (await requestClose() && onstop) await onstop(playbackState?.destination);
    }

    async function retryClose() {
        if (await requestClose({ retry: true }) && onstop) await onstop(playbackState?.destination);
    }

    async function handleRetry(event) {
        const heldFocus = document.activeElement === event.currentTarget;
        player.clearError();
        sourceError = false;
        mediaSource?.release();
        lastAttachedUrl = null;
        try {
            if ($player.release?.phase === 'failed') await player.retryRelease();
            await runtime.retry();
            autoplay.reset();
        } catch {}
        finally {
            await tick();
            if (isMounted && !closing && heldFocus && document.activeElement === document.body) (retryButton || containerEl)?.focus();
        }
    }

    let seekDisplayValue = $derived((isSeeking || isKeyboardSeeking) ? seekValue : ($player?.positionMs || 0));
    let durationMs = $derived($player?.durationMs || 0);
    let positionDisplay = $derived(formatTimestamp(seekDisplayValue));
    let durationDisplay = $derived(formatTimestamp(durationMs));
    let runtimeLabel = $derived.by(() => {
        const secs = $currentMediaItem?.runtime_seconds || mediaItem?.runtime_seconds;
        return secs ? formatDuration(secs) : null;
    });

    let seekPreviewVisible = $derived((isSeekHovering || isSeeking || isKeyboardSeeking) && !!storyboard);
    let seekPreviewMs = $derived((isSeeking || isKeyboardSeeking) ? seekValue : seekHoverMs);
    let seekPreviewRatio = $derived(durationMs > 0 ? Math.max(0, Math.min(1, seekPreviewMs / durationMs)) : 0);
</script>

<svelte:window onkeydown={handleKeydown} />

<div
    bind:this={containerEl}
    class="player-container"
    class:controls-hidden={!controlsVisible}
    role="region"
    tabindex="-1"
    aria-label={m.lib_components_player_media_player()}
    onmousemove={handleMouseMove}
    onmouseleave={handleMouseLeave}
    ontouchstart={showControls}
    onfocusin={handleControlFocus}
    onfocusout={(event) => { controlsFocused = event.relatedTarget instanceof Node && containerEl.contains(event.relatedTarget) && event.relatedTarget !== containerEl; }}
>

    <div class="player-heading" class:visible={controlsVisible}>
        <span>{displayTitle}</span>
                <button class="control-btn close-btn" disabled={closing} onclick={handleClose} aria-label={m.lib_components_player_close_player()}>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M18 6L6 18M6 6l12 12" />
                    </svg>
                </button>
    </div>
    {#if fullscreenError}
        <p lang={messageLocale('tonight_player_fullscreen_failed')} class="fullscreen-error" role="status">{m.tonight_player_fullscreen_failed()}</p>
    {/if}
    <p lang={messageLocale('tonight_player_track_fallback')} class="track-fallback" class:visible={controlsVisible} role="status" aria-live="polite">{trackStatus}</p>

    <div class="player-stage" bind:this={videoStage}>
    <video
        bind:this={videoEl}
        class="player-video"
        onplay={handlePlay}
        onpause={handlePause}
        onwaiting={handleWaiting}
        onplaying={handlePlaying}
        ontimeupdate={handleTimeUpdate}
        ondurationchange={handleDurationChange}
        onloadedmetadata={handleLoadedMetadata}
        onprogress={updateBuffered}
        onended={handleEnded}
        onseeked={handleSeeked}
        playsinline
    ></video>
    </div>
    <p lang={closing ? messageLocale('tonight_player_closing') : closeRecovery ? messageLocale('tonight_player_close_failed') : undefined} class="visually-hidden" role="status" aria-live="polite">{closing ? m.tonight_player_closing() : closeRecovery ? m.tonight_player_close_failed() : ''}</p>

    {#if closeRecovery && !closing}
        <div class="player-overlay-center">
            <div class="error-display">
                <p lang={messageLocale('tonight_player_close_failed')} class="error-message">{m.tonight_player_close_failed()}</p>
                <button lang={messageLocale('tonight_player_retry_close')} bind:this={closeRetryButton} class="error-retry" onclick={retryClose}>{m.tonight_player_retry_close()}</button>
            </div>
        </div>
    {/if}

    {#if $playerLoading || playbackState?.loading}
        <div class="player-overlay-center">
            <div class="loading-spinner" aria-label={m.lib_components_player_loading()}></div>
        </div>
    {/if}

    {#if $isBuffering && !$playerLoading}
        <div class="player-overlay-center">
            <div class="loading-spinner" aria-label={m.lib_components_player_buffering()}></div>
        </div>
    {/if}

    {#if !exitRequested && !closeRecovery && $player.release?.phase !== 'failed' && ($playerError || playbackState?.error || sourceError)}
        <div class="player-overlay-center">
            <div class="error-display">
                <p class="error-title">{m.lib_components_player_playback_error()}</p>
                <p lang={sourceError ? messageLocale('tonight_player_transport_failed') : !playbackState?.preferencesReady ? messageLocale('tonight_player_preferences_failed') : messageLocale('lib_components_player_playback_error_the_stream_may_be_unavailable')} class="error-message" role="alert">{sourceError ? m.tonight_player_transport_failed() : !playbackState?.preferencesReady ? m.tonight_player_preferences_failed() : m.lib_components_player_playback_error_the_stream_may_be_unavailable()}</p>
                <button bind:this={retryButton} class="error-retry" onclick={handleRetry}>{m.lib_components_player_retry()}</button>
            </div>
        </div>
    {/if}

    {#if surfaceableSegments.length > 0 && !$playerLoading && !$playerError}
        <SkipButton
            segments={surfaceableSegments}
            positionMs={$player?.positionMs || 0}
            autoSkipTypes={autoSkipTypes}
            onskip={handleSkip}
        />
    {/if}

    <div class="autoplay-position"><AutoplayCard state={autoplayState} onplaynext={() => autoplay.playNext()} oncancel={() => autoplay.cancel()} onretry={() => autoplay.retry()} onfocuschange={(focused) => { autoplayFocused = focused; }} /></div>
    <div class="player-controls" role="group" aria-label={m.lib_components_player_media_player()} class:visible={controlsVisible} onmouseenter={() => { controlsHovered = true; }} onmouseleave={() => { controlsHovered = false; }}>
        <div
            class="seek-bar-wrapper"
            role="presentation"
            onmousemove={handleSeekBarMouseMove}
            onmouseenter={() => { isSeekHovering = true; }}
            onmouseleave={handleSeekBarHoverEnd}
            ontouchmove={handleSeekBarTouchMove}
            ontouchend={handleSeekBarHoverEnd}
        >
            {#if storyboard}
                <SeekPreview
                    mediaItemId={$currentMediaItem?.id || mediaItem?.id}
                    storyboard={storyboard}
                    visible={seekPreviewVisible}
                    positionMs={seekPreviewMs}
                    hoverRatio={seekPreviewRatio}
                />
            {/if}
            <div class="seek-bar-track">
                <div class="seek-buffered" style="width: {bufferedPercent}%"></div>
                <div class="seek-progress" style="width: {durationMs > 0 ? Math.min(100, (seekDisplayValue / durationMs) * 100) : 0}%"></div>
            </div>
            <input
                type="range"
                class="seek-bar"
                min="0"
                max={durationMs || 0}
                step="100"
                value={seekDisplayValue}
                oninput={handleSeekInput}
                onmousedown={handleSeekStart}
                ontouchstart={handleSeekStart}
                onmouseup={handleSeekEnd}
                ontouchend={handleSeekEnd}
                onblur={() => {
                    if (isKeyboardSeeking) handleSeekEnd();
                }}
                aria-label={m.lib_components_player_seek()}
                aria-valuetext="{positionDisplay} of {durationDisplay}"
                disabled={controlsDisabled}
            />
        </div>

        <div class="controls-row">
            <div class="controls-left">
                <button lang={$isPlaying ? messageLocale('tonight_player_pause') : messageLocale('tonight_player_play')} class="control-btn" disabled={controlsDisabled} onclick={togglePlayPause} aria-label={$isPlaying ? m.tonight_player_pause() : m.tonight_player_play()}>
                    {#if $isPlaying}
                        <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor">
                            <rect x="6" y="5" width="4" height="14" rx="1" />
                            <rect x="14" y="5" width="4" height="14" rx="1" />
                        </svg>
                    {:else}
                        <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M8 5v14l11-7z" />
                        </svg>
                    {/if}
                </button>

                <div class="volume-control">
                    <button lang={$player?.isMuted ? messageLocale('tonight_player_unmute') : messageLocale('tonight_player_mute')} class="control-btn" disabled={controlsDisabled} onclick={toggleMute} aria-label={$player?.isMuted ? m.tonight_player_unmute() : m.tonight_player_mute()}>
                        {#if $player?.isMuted || $playerVolume === 0}
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                                <path d="M3 9v6h4l5 5V4L7 9H3zm13.59 3L19 9.59 17.59 8.17 15.17 10.59 12.76 8.17 11.34 9.59 13.76 12l-2.42 2.41 1.42 1.42L15.17 13.41 17.59 15.83 19 14.41z" />
                            </svg>
                        {:else}
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                                <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z" />
                            </svg>
                        {/if}
                    </button>
                    <input
                        type="range"
                        class="volume-slider"
                        min="0"
                        max="1"
                        step="0.05"
                        value={$playerVolume}
                        oninput={handleVolumeChange}
                        aria-label={m.lib_components_player_volume()}
                        disabled={controlsDisabled}
                    />
                </div>

                <span class="time-display">{positionDisplay} / {durationDisplay}</span>
            </div>

            <div class="controls-center">
                <span class="player-title">{displayTitle}</span>
                {#if runtimeLabel}
                    <span class="player-runtime">{runtimeLabel}</span>
                {/if}
            </div>

            <div class="controls-right">
                <PlayerMenus bind:this={menus} episodes={episodeChoices} audioChoices={tracks.audio} subtitleChoices={tracks.subtitles} {qualityChoices} {speedChoices} busy={playbackState?.loading || $playerLoading} disabled={controlsDisabled || !playbackState?.preferencesReady} onselect={handleChoice} onopenchange={(open) => { menuOpen = open; showControls(); }} />

                <button lang={$player?.isFullscreen ? messageLocale('tonight_player_fullscreen_exit') : messageLocale('lib_components_player_fullscreen')} class="control-btn" disabled={controlsDisabled} onclick={toggleFullscreen} aria-label={$player?.isFullscreen ? m.tonight_player_fullscreen_exit() : m.lib_components_player_fullscreen()}>
                    {#if $player?.isFullscreen}
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M5 16h3v3h2v-5H5v2zm3-8H5v2h5V5H8v3zm6 11h2v-3h3v-2h-5v5zm2-11V5h-2v5h5V8h-3z" />
                        </svg>
                    {:else}
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M7 14H5v5h5v-2H7v-3zm-2-4h2V7h3V5H5v5zm12 7h-3v2h5v-5h-2v3zM14 5v2h3v3h2V5h-5z" />
                        </svg>
                    {/if}
                </button>


            </div>
        </div>
    </div>
</div>

<style>
    .player-container {
        position: relative;
        width: 100%;
        height: 100%;
        min-height: 0;
        background-color: #000;
        overflow: hidden;
        outline: none;
        cursor: default;
    }

    .player-container:focus-visible { outline: 2px solid var(--color-accent); outline-offset: -3px; }
    .player-heading { position: absolute; inset-inline: 1rem; top: 1rem; z-index: 2; display: flex; align-items: center; justify-content: space-between; gap: 1rem; opacity: 0; pointer-events: none; }
    .player-heading.visible { opacity: 1; pointer-events: auto; }
    .player-heading span { text-shadow: 0 1px 5px #000; }
    .player-heading .close-btn { background: rgba(0,0,0,.65); }
    .fullscreen-error { position: absolute; top: 5rem; inset-inline: 1rem; z-index: 3; padding: .75rem; background: var(--color-bg-surface); color: var(--color-text-primary); }
    .track-fallback { position: absolute; top: 3.75rem; inset-inline: 1rem; z-index: 3; max-width: 35rem; color: var(--color-text-primary); text-shadow: 0 1px 5px #000; font-size: .8rem; opacity: 0; pointer-events: none; }
    .track-fallback.visible { opacity: 1; }
    .control-btn:focus-visible, .volume-slider:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; }
    .seek-bar-wrapper:focus-within { outline: 2px solid var(--color-accent); outline-offset: 3px; }
    @media (prefers-reduced-motion: reduce) { .loading-spinner { animation: none; } .player-controls, .seek-progress, .seek-buffered { transition: none; } }

    .player-container.controls-hidden {
        cursor: none;
    }

    .player-stage { position: absolute; inset: 0; overflow: hidden; }

    .player-video {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        object-fit: contain;
        display: block;
    }

    .player-overlay-center {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        pointer-events: none;
    }

    .loading-spinner {
        width: 56px;
        height: 56px;
        border: 3px solid rgba(255, 255, 255, 0.15);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .error-display {
        text-align: center;
        padding: 2rem;
        pointer-events: auto;
    }

    .error-title {
        font-size: 1.125rem;
        font-weight: 600;
        color: var(--color-text-primary);
        margin-bottom: 0.5rem;
    }

    .error-message {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        margin-bottom: 1rem;
    }

    .error-retry {
        min-height: 44px;
        min-width: 44px;
        padding: 0.5rem 1.5rem;
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-bg-deep);
        background-color: var(--color-accent);
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .error-retry:hover {
        background-color: var(--color-accent-hover);
    }

    .player-controls {
        position: absolute;
        bottom: 0;
        inset-inline: 0;
        padding: 0.75rem 1rem 0.625rem;
        background: linear-gradient(to top, rgba(0, 0, 0, 0.85) 0%, rgba(0, 0, 0, 0.4) 60%, transparent 100%);
        opacity: 0;
        transform: translateY(10px);
        transition: opacity var(--transition-normal), transform var(--transition-normal);
        pointer-events: none;
    }

    .player-controls.visible {
        opacity: 1;
        transform: translateY(0);
        pointer-events: auto;
    }

    .seek-bar-wrapper {
        position: relative;
        height: 44px;
        margin-bottom: 0.375rem;
        cursor: pointer;
    }

    .seek-bar-track {
        position: absolute;
        top: 50%;
        inset-inline: 0;
        height: 4px;
        transform: translateY(-50%);
        background-color: rgba(255, 255, 255, 0.2);
        border-radius: 2px;
        overflow: hidden;
        pointer-events: none;
    }

    .seek-buffered {
        height: 100%;
        background-color: rgba(255, 255, 255, 0.35);
        transition: width var(--transition-normal);
    }

    .seek-progress {
        height: 100%;
        background-color: var(--color-accent);
        transition: width 100ms linear;
    }

    .seek-bar {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        margin: 0;
        opacity: 0;
        cursor: pointer;
    }

    .autoplay-position { position: absolute; bottom: 9rem; inset-inline-end: 1rem; width: min(380px, calc(100% - 2rem)); z-index: 3; max-height: calc(100% - 13rem); overflow: auto; }

    .controls-row {
        flex-wrap: wrap;
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
    }

    .controls-left,
    .controls-right {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        flex-wrap: wrap;
        max-width: 100%;
    }

    .controls-center {
        display: none;
        flex: 1;
        text-align: center;
        overflow: hidden;
    }

    .player-title {
        font-size: 0.8125rem;
        color: var(--color-text-primary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        font-weight: 500;
    }

    .player-runtime {
        font-size: 0.6875rem;
        color: var(--color-text-secondary);
        margin-inline-start: 0.5rem;
    }

    .control-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 44px;
        height: 44px;
        color: rgba(255, 255, 255, 0.9);
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
    }

    .control-btn:hover {
        color: #fff;
        background-color: rgba(255, 255, 255, 0.1);
    }

    .close-btn:hover {
        color: var(--color-error);
    }

    .volume-control {
        display: flex;
        align-items: center;
        gap: 0.25rem;
    }

    .volume-slider {
        width: 80px;
        height: 44px;
        appearance: none;
        -webkit-appearance: none;
        background: linear-gradient(rgba(255,255,255,.25), rgba(255,255,255,.25)) center / 100% 4px no-repeat;
        border-radius: 2px;
        cursor: pointer;
    }

    .volume-slider::-webkit-slider-thumb {
        appearance: none;
        -webkit-appearance: none;
        width: 12px;
        height: 12px;
        border-radius: 50%;
        background: #fff;
        cursor: pointer;
    }

    .volume-slider::-moz-range-thumb {
        width: 12px;
        height: 12px;
        border: none;
        border-radius: 50%;
        background: #fff;
        cursor: pointer;
    }

    .time-display {
        font-size: 0.75rem;
        color: rgba(255, 255, 255, 0.8);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    @media (max-width: 640px) {
        .controls-center {
            display: none;
        }

        .player-controls {
            padding: 0.5rem 0.625rem;
        }
    }

    @media (max-height: 420px) {
        .player-container { display: flex; flex-direction: column; overflow-y: auto; scroll-padding-block: 72px 12px; }
        .player-heading { position: sticky; top: 0; inset-inline: auto; min-height: 60px; padding: .5rem .75rem; flex-shrink: 0; order: 0; background: #000; z-index: 5; }
        .player-stage { position: relative; inset: auto; height: 55dvh; min-height: 80px; flex-shrink: 0; order: 1; }
        .player-controls { position: relative; bottom: auto; inset-inline: auto; transform: none; flex-shrink: 0; order: 3; background: var(--color-bg-surface); }
        .player-controls.visible { transform: none; }
        .autoplay-position { position: relative; inset: auto; max-height: none; width: auto; margin: .75rem; overflow: visible; flex-shrink: 0; order: 2; }
        .player-overlay-center { position: relative; inset: auto; min-height: 80px; flex-shrink: 0; order: 2; }
        .fullscreen-error, .track-fallback { position: relative; inset: auto; margin: .5rem .75rem; flex-shrink: 0; order: 1; }
        .track-fallback:empty { display: none; }
        .player-heading button, .player-controls input, .player-controls :global(button), .autoplay-position :global(button) { scroll-margin-block: 0; }
    }
</style>
