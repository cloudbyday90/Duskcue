<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { fade, fly } from 'svelte/transition';

    const SKIP_LABELS = {
        intro: 'Skip Intro',
        credits: 'Skip Credits',
        recap: 'Skip Recap',
        preview: 'Skip Preview',
        outro: 'Skip Outro',
    };

    const HIGH_PROMINENCE_TIMEOUT_MS = 10000;
    const MEDIUM_PROMINENCE_TIMEOUT_MS = 5000;
    const HIGH_CONFIDENCE_THRESHOLD = 0.8;

    let {
        segments = [],
        positionMs = 0,
        autoSkipTypes = [],
        onskip = null,
    } = $props();

    let activeSegment = $derived.by(() => {
        for (const seg of segments) {
            if (positionMs >= seg.start_ms && positionMs < seg.end_ms) {
                return seg;
            }
        }
        return null;
    });

    let trackedSegmentId = $state(null);
    let enteredAtMs = $state(0);
    let autoSkippedIds = $state(new Set());
    let dismissedIds = $state(new Set());

    let prominence = $derived.by(() => {
        const seg = activeSegment;
        if (!seg) return 'high';
        if (seg.is_manual) return 'high';
        return seg.confidence >= HIGH_CONFIDENCE_THRESHOLD ? 'high' : 'medium';
    });

    let timeoutMs = $derived(
        prominence === 'high' ? HIGH_PROMINENCE_TIMEOUT_MS : MEDIUM_PROMINENCE_TIMEOUT_MS,
    );

    let visible = $derived.by(() => {
        const seg = activeSegment;
        if (!seg) return false;
        if (dismissedIds.has(seg.id)) return false;
        if (trackedSegmentId !== seg.id) return true;
        return positionMs - enteredAtMs < timeoutMs;
    });

    let displayLabel = $derived.by(() => {
        const seg = activeSegment;
        if (!seg) return '';
        return SKIP_LABELS[seg.segment_type] || `Skip ${seg.segment_type}`;
    });

    $effect(() => {
        const seg = activeSegment;
        if (!seg) {
            trackedSegmentId = null;
            enteredAtMs = 0;
            return;
        }
        if (trackedSegmentId !== seg.id) {
            trackedSegmentId = seg.id;
            enteredAtMs = positionMs;
            dismissedIds.delete(seg.id);
            if (autoSkipTypes.includes(seg.segment_type) && !autoSkippedIds.has(seg.id)) {
                autoSkippedIds = new Set([...autoSkippedIds, seg.id]);
                onskip?.(seg.skip_to_ms);
            }
        }
    });

    function handleClick() {
        const seg = activeSegment;
        if (!seg) return;
        dismissedIds = new Set([...dismissedIds, seg.id]);
        onskip?.(seg.skip_to_ms);
    }
</script>

{#if activeSegment && visible}
    <button
        class="skip-button prominence-{prominence}"
        onclick={handleClick}
        transition:fly={{ y: 16, duration: 200 }}
        aria-label={displayLabel}
    >
        <span class="skip-label">{displayLabel}</span>
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="M5 4l10 8-10 8V4zm12 0h2v16h-2V4z" />
        </svg>
    </button>
{/if}

<style>
    .skip-button {
        position: absolute;
        right: 1.25rem;
        bottom: 5.5rem;
        z-index: 10;
        display: inline-flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.625rem 1.125rem;
        font-family: var(--font-sans);
        font-size: 0.875rem;
        font-weight: 600;
        letter-spacing: 0.01em;
        color: var(--color-bg-deep);
        background-color: var(--color-accent);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-elevated);
        cursor: pointer;
        transition: background-color var(--transition-fast), transform var(--transition-fast);
    }

    .skip-button:hover {
        background-color: var(--color-accent-hover);
        transform: translateY(-1px);
    }

    .skip-button:focus-visible {
        outline: 2px solid var(--color-text-primary);
        outline-offset: 2px;
    }

    .skip-button.prominence-medium {
        padding: 0.5rem 0.875rem;
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text-primary);
        background-color: rgba(30, 33, 41, 0.92);
        border: 1px solid var(--color-border);
        backdrop-filter: blur(8px);
    }

    .skip-button.prominence-medium:hover {
        background-color: rgba(38, 42, 53, 0.95);
        transform: translateY(-1px);
    }

    .skip-label {
        white-space: nowrap;
    }

    @media (max-width: 640px) {
        .skip-button {
            right: 0.75rem;
            bottom: 4.75rem;
            padding: 0.5rem 0.875rem;
            font-size: 0.8125rem;
        }

        .skip-button.prominence-medium {
            padding: 0.4375rem 0.75rem;
            font-size: 0.75rem;
        }
    }
</style>
