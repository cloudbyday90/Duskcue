<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount, onDestroy } from 'svelte';
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
        progressPercent,
    } from '../stores/player.js';
    import { notifications } from '../stores/notifications.js';
    import { submitQoeReport } from '../api/quality.js';
    import { formatTimestamp, formatDuration } from '../utils/format.js';
    import { PLAYER_CONTROLS_TIMEOUT_MS, PLAYER_SEEK_STEP_S, PLAYER_VOLUME_STEP } from '../utils/constants.js';

    let {
        mediaItem = null,
        mediaFileId = null,
        startPositionMs = 0,
        sessionId = null,
        title = null,
        onstop = null,
    } = $props();

    let videoEl = null;
    let containerEl = null;
    let hls = null;

    let isMounted = $state(false);
    let controlsVisible = $state(true);
    let isSeeking = $state(false);
    let seekValue = $state(0);
    let bufferedPercent = $state(0);
    let hideControlsTimer = null;
    let qoeTimer = null;
    let lastBufferStart = $state(null);

    let displayTitle = $derived(title || mediaItem?.title || $currentMediaItem?.title || 'Playing');

    onMount(async () => {
        isMounted = true;

        try {
            if (sessionId) {
                await player.resume(sessionId);
            } else if (mediaItem && mediaFileId) {
                await player.play(mediaItem, mediaFileId, {
                    startPositionMs,
                });
            }
        } catch (err) {
            notifications.error(`Failed to start playback: ${err.message || err}`);
        }

        startQoeReporting();
    });

    onDestroy(() => {
        destroyHls();
        clearHideControlsTimer();
        stopQoeReporting();
        player.destroy();
    });

    async function loadHlsJs() {
        const mod = await import('hls.js');
        return mod.default;
    }

    function destroyHls() {
        if (hls) {
            hls.destroy();
            hls = null;
        }
    }

    async function attachStream(url) {
        if (!videoEl) return;

        destroyHls();

        const isHlsStream = url.includes('.m3u8');

        if (isHlsStream) {
            const Hls = await loadHlsJs();

            if (Hls.isSupported()) {
                hls = new Hls({
                    enableWorker: true,
                    lowLatencyMode: false,
                    backBufferLength: 90,
                });

                hls.loadSource(url);
                hls.attachMedia(videoEl);

                hls.on(Hls.Events.MANIFEST_PARSED, () => {
                    videoEl.play().catch(() => {});
                });

                hls.on(Hls.Events.ERROR, (_event, data) => {
                    if (data.fatal) {
                        switch (data.type) {
                            case Hls.ErrorTypes.NETWORK_ERROR:
                                hls.startLoad();
                                break;
                            case Hls.ErrorTypes.MEDIA_ERROR:
                                hls.recoverMediaError();
                                break;
                            default:
                                destroyHls();
                                notifications.error('Playback error. The stream may be unavailable.');
                                break;
                        }
                    }
                });
            } else if (videoEl.canPlayType('application/vnd.apple.mpegurl')) {
                videoEl.src = url;
                videoEl.play().catch(() => {});
            }
        } else {
            videoEl.src = url;
            videoEl.play().catch(() => {});
        }
    }

    let lastAttachedUrl = null;
    $effect(() => {
        if (!isMounted) return;
        const url = $streamUrl;
        if (url && url !== lastAttachedUrl) {
            lastAttachedUrl = url;
            attachStream(url);
        }
    });

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
        lastBufferStart = Date.now();
    }

    function handlePlaying() {
        player.setBuffering(false);
        if (lastBufferStart) {
            sendQoeReport({ buffer_duration_ms: Date.now() - lastBufferStart });
            lastBufferStart = null;
        }
    }

    function handleTimeUpdate() {
        if (videoEl && !isSeeking) {
            player.setPosition(videoEl.currentTime * 1000);
            updateBuffered();
        }
    }

    function handleDurationChange() {
        if (videoEl) {
            player.setDuration(videoEl.duration * 1000);
        }
    }

    function handleLoadedMetadata() {
        if (videoEl) {
            player.setDuration(videoEl.duration * 1000);
            if (startPositionMs > 0 && videoEl.currentTime === 0) {
                videoEl.currentTime = startPositionMs / 1000;
            }
        }
    }

    function updateBuffered() {
        if (!videoEl || !videoEl.buffered.length || !videoEl.duration) return;
        const end = videoEl.buffered.end(videoEl.buffered.length - 1);
        bufferedPercent = (end / videoEl.duration) * 100;
    }

    function togglePlayPause() {
        if (!videoEl) return;
        if (videoEl.paused) {
            videoEl.play().catch(() => {});
        } else {
            videoEl.pause();
        }
    }

    function handleSeekInput(event) {
        seekValue = parseFloat(event.target.value);
    }

    function handleSeekStart() {
        isSeeking = true;
        seekValue = $player?.positionMs || 0;
    }

    function handleSeekEnd() {
        isSeeking = false;
        const positionMs = seekValue;
        const decision = $streamDecision;

        if (decision === 'direct_play' && videoEl) {
            videoEl.currentTime = positionMs / 1000;
            player.setPosition(positionMs);
        } else {
            player.seek(positionMs);
        }
    }

    function handleVolumeChange(event) {
        player.setVolume(parseFloat(event.target.value));
    }

    function toggleMute() {
        player.toggleMute();
    }

    function handlePlaybackRateChange(event) {
        player.setPlaybackRate(parseFloat(event.target.value));
    }

    async function toggleFullscreen() {
        if (!document.fullscreenElement) {
            await containerEl?.requestFullscreen?.().catch(() => {});
            player.setFullscreen(true);
        } else {
            await document.exitFullscreen?.().catch(() => {});
            player.setFullscreen(false);
        }
    }

    function showControls() {
        controlsVisible = true;
        clearHideControlsTimer();
        if ($isPlaying && !isSeeking) {
            hideControlsTimer = setTimeout(() => {
                controlsVisible = false;
            }, PLAYER_CONTROLS_TIMEOUT_MS);
        }
    }

    function clearHideControlsTimer() {
        if (hideControlsTimer) {
            clearTimeout(hideControlsTimer);
            hideControlsTimer = null;
        }
    }

    function handleMouseMove() {
        showControls();
    }

    function handleMouseLeave() {
        if ($isPlaying) {
            controlsVisible = false;
        }
    }

    function handleKeydown(event) {
        switch (event.key) {
            case ' ':
            case 'k':
                event.preventDefault();
                togglePlayPause();
                break;
            case 'ArrowLeft':
                event.preventDefault();
                if (videoEl) {
                    const newPos = Math.max(0, videoEl.currentTime - PLAYER_SEEK_STEP_S);
                    videoEl.currentTime = newPos;
                }
                break;
            case 'ArrowRight':
                event.preventDefault();
                if (videoEl) {
                    const newPos = videoEl.currentTime + PLAYER_SEEK_STEP_S;
                    videoEl.currentTime = Math.min(videoEl.duration || newPos, newPos);
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
            case 'Escape':
                if (onstop) {
                    handleClose();
                }
                break;
        }
        showControls();
    }

    async function handleClose() {
        destroyHls();
        clearHideControlsTimer();
        stopQoeReporting();
        await player.stop();
        if (onstop) {
            onstop();
        }
    }

    async function handleRetry() {
        player.clearError();
        if (mediaItem && mediaFileId) {
            try {
                await player.play(mediaItem, mediaFileId, { startPositionMs });
            } catch (err) {
                notifications.error(`Playback retry failed: ${err.message || err}`);
            }
        }
    }

    function startQoeReporting() {
        stopQoeReporting();
        qoeTimer = setInterval(() => {
            sendQoeReport({});
        }, 30000);
    }

    function stopQoeReporting() {
        if (qoeTimer) {
            clearInterval(qoeTimer);
            qoeTimer = null;
        }
    }

    async function sendQoeReport(extra = {}) {
        const state = $player;
        if (!state.sessionId) return;
        try {
            await submitQoeReport({
                session_id: state.sessionId,
                position_ms: Math.floor(state.positionMs),
                is_playing: state.isPlaying,
                is_buffering: state.isBuffering,
                ...extra,
            });
        } catch {
        }
    }

    let seekDisplayValue = $derived(isSeeking ? seekValue : ($player?.positionMs || 0));
    let durationMs = $derived($player?.durationMs || 0);
    let positionDisplay = $derived(formatTimestamp(seekDisplayValue));
    let durationDisplay = $derived(formatTimestamp(durationMs));
    let runtimeLabel = $derived.by(() => {
        const secs = mediaItem?.runtime_seconds || $currentMediaItem?.runtime_seconds;
        return secs ? formatDuration(secs) : null;
    });
</script>

<svelte:window onfullscreenchange={() => player.setFullscreen(!!document.fullscreenElement)} onkeydown={handleKeydown} />

<div
    bind:this={containerEl}
    class="player-container"
    class:controls-hidden={!controlsVisible}
    role="region"
    aria-label="Media player"
    onmousemove={handleMouseMove}
    onmouseleave={handleMouseLeave}
>
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
        playsinline
    ></video>

    {#if $playerLoading}
        <div class="player-overlay-center">
            <div class="loading-spinner" aria-label="Loading"></div>
        </div>
    {/if}

    {#if $isBuffering && !$playerLoading}
        <div class="player-overlay-center">
            <div class="loading-spinner" aria-label="Buffering"></div>
        </div>
    {/if}

    {#if $playerError}
        <div class="player-overlay-center">
            <div class="error-display">
                <p class="error-title">Playback error</p>
                <p class="error-message">{$playerError.message || 'An error occurred during playback.'}</p>
                <button class="error-retry" onclick={handleRetry}>Retry</button>
            </div>
        </div>
    {/if}

    <div class="player-controls" class:visible={controlsVisible}>
        <div class="seek-bar-wrapper">
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
                aria-label="Seek"
                aria-valuetext="{positionDisplay} of {durationDisplay}"
            />
        </div>

        <div class="controls-row">
            <div class="controls-left">
                <button class="control-btn" onclick={togglePlayPause} aria-label={$isPlaying ? 'Pause' : 'Play'}>
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
                    <button class="control-btn" onclick={toggleMute} aria-label={$player?.isMuted ? 'Unmute' : 'Mute'}>
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
                        aria-label="Volume"
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
                <select
                    class="speed-select"
                    value={$player?.playbackRate ?? 1}
                    onchange={handlePlaybackRateChange}
                    aria-label="Playback speed"
                >
                    <option value="0.5">0.5x</option>
                    <option value="0.75">0.75x</option>
                    <option value="1">1x</option>
                    <option value="1.25">1.25x</option>
                    <option value="1.5">1.5x</option>
                    <option value="2">2x</option>
                </select>

                <button class="control-btn" onclick={toggleFullscreen} aria-label="Fullscreen">
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

                <button class="control-btn close-btn" onclick={handleClose} aria-label="Close player">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M18 6L6 18M6 6l12 12" />
                    </svg>
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
        min-height: 360px;
        background-color: #000;
        overflow: hidden;
        outline: none;
        cursor: default;
    }

    .player-container.controls-hidden {
        cursor: none;
    }

    .player-video {
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
        left: 0;
        right: 0;
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
        height: 6px;
        margin-bottom: 0.625rem;
    }

    .seek-bar-track {
        position: absolute;
        top: 50%;
        left: 0;
        right: 0;
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

    .controls-row {
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
        flex-shrink: 0;
    }

    .controls-center {
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
        margin-left: 0.5rem;
    }

    .control-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
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
        height: 4px;
        appearance: none;
        -webkit-appearance: none;
        background: rgba(255, 255, 255, 0.25);
        border-radius: 2px;
        outline: none;
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

    .speed-select {
        font-size: 0.75rem;
        color: rgba(255, 255, 255, 0.9);
        background-color: transparent;
        border: 1px solid rgba(255, 255, 255, 0.2);
        border-radius: var(--radius-sm);
        padding: 0.25rem 0.5rem;
        cursor: pointer;
        outline: none;
    }

    .speed-select option {
        color: var(--color-text-primary);
        background-color: var(--color-bg-elevated);
    }

    @media (max-width: 640px) {
        .controls-center {
            display: none;
        }

        .volume-slider {
            display: none;
        }

        .player-controls {
            padding: 0.5rem 0.625rem;
        }
    }
</style>
