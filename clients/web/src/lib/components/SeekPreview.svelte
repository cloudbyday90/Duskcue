<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { fade } from 'svelte/transition';
    import { parseStoryboardVtt, findCueForTime } from '../utils/storyboards.js';
    import { formatTimestamp } from '../utils/format.js';

    const DEFAULT_DISPLAY_WIDTH = 160;
    const FALLBACK_GRID = { columns: 10, rows: 20 };

    let {
        storyboard = null,
        visible = false,
        positionMs = 0,
        hoverRatio = 0,
        displayWidth = DEFAULT_DISPLAY_WIDTH,
    } = $props();

    let cues = $state([]);
    let loadedKey = null;
    let fetchId = 0;

    $effect(() => {
        const sb = storyboard;
        if (!sb || !sb.index_url) {
            cues = [];
            loadedKey = null;
            return;
        }

        const key = sb.media_file_id || sb.index_url;
        if (loadedKey === key) return;
        loadedKey = key;

        const currentFetchId = ++fetchId;
        const absoluteBase = resolveAbsoluteUrl(sb.index_url);

        fetch(sb.index_url, { credentials: 'same-origin' })
            .then((res) => {
                if (!res.ok) throw new Error(`HTTP ${res.status}`);
                return res.text();
            })
            .then((text) => {
                if (currentFetchId !== fetchId) return;
                cues = parseStoryboardVtt(text, absoluteBase);
            })
            .catch(() => {
                if (currentFetchId !== fetchId) return;
                cues = [];
            });
    });

    let currentCue = $derived.by(() => {
        if (cues.length === 0) return null;
        return findCueForTime(cues, positionMs);
    });

    let resolvedDisplayWidth = $derived.by(() => {
        const sb = storyboard;
        if (!sb || !sb.width) return displayWidth;
        return Math.min(displayWidth, sb.width);
    });

    let displayHeight = $derived.by(() => {
        const sb = storyboard;
        if (!sb || !sb.width || !sb.height) {
            return Math.round(resolvedDisplayWidth * 9 / 16);
        }
        return Math.round(resolvedDisplayWidth * (sb.height / sb.width));
    });

    let gridShape = $derived.by(() => {
        const sb = storyboard;
        if (!sb || !Array.isArray(sb.sprites) || sb.sprites.length === 0) {
            return FALLBACK_GRID;
        }
        const first = sb.sprites[0];
        return {
            columns: first.columns || FALLBACK_GRID.columns,
            rows: first.rows || FALLBACK_GRID.rows,
        };
    });

    let thumbnailStyle = $derived.by(() => {
        const cue = currentCue;
        const sb = storyboard;
        if (!cue || !sb) return '';

        const nativeW = sb.width || cue.w;
        const nativeH = sb.height || cue.h;
        const scale = resolvedDisplayWidth / nativeW;
        const { columns, rows } = gridShape;

        const sheetWidth = columns * nativeW * scale;
        const sheetHeight = rows * nativeH * scale;
        const bgX = -(cue.x * scale);
        const bgY = -(cue.y * scale);

        return [
            `background-image: url('${cue.spriteUrl}')`,
            `background-position: ${bgX}px ${bgY}px`,
            `background-size: ${sheetWidth}px ${sheetHeight}px`,
        ].join('; ');
    });

    let timeLabel = $derived(formatTimestamp(positionMs));

    function resolveAbsoluteUrl(relativeUrl) {
        if (typeof window === 'undefined') return relativeUrl;
        try {
            return new URL(relativeUrl, window.location.href).href;
        } catch {
            return relativeUrl;
        }
    }
</script>

{#if visible && currentCue}
    <div
        class="seek-preview"
        style="--hover-ratio: {hoverRatio}; --thumb-width: {resolvedDisplayWidth}px; --thumb-height: {displayHeight}px;"
        transition:fade={{ duration: 100 }}
    >
        <div class="preview-thumbnail" style={thumbnailStyle}></div>
        <div class="preview-time">{timeLabel}</div>
    </div>
{/if}

<style>
    .seek-preview {
        position: absolute;
        bottom: 100%;
        margin-bottom: 10px;
        left: clamp(
            calc(var(--thumb-width) / 2),
            calc(var(--hover-ratio) * 100%),
            calc(100% - var(--thumb-width) / 2)
        );
        transform: translateX(-50%);
        width: var(--thumb-width);
        pointer-events: none;
        z-index: 20;
        border-radius: var(--radius-sm);
        overflow: hidden;
        box-shadow: var(--shadow-elevated);
        background-color: var(--color-bg-deep);
    }

    .preview-thumbnail {
        width: 100%;
        height: var(--thumb-height);
        background-repeat: no-repeat;
        background-color: #000;
    }

    .preview-time {
        padding: 0.2rem 0.5rem;
        font-size: 0.75rem;
        font-weight: 600;
        text-align: center;
        color: var(--color-text-primary);
        background-color: rgba(14, 15, 19, 0.95);
        font-variant-numeric: tabular-nums;
        line-height: 1.4;
    }

    @media (max-width: 480px) {
        .seek-preview {
            margin-bottom: 8px;
        }

        .preview-time {
            padding: 0.15rem 0.4rem;
            font-size: 0.6875rem;
        }
    }
</style>
