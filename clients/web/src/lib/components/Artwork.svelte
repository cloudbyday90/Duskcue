<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { getArtwork } from '$lib/api/artwork.js';
    import posterPlaceholder from '$lib/assets/placeholder-poster.svg';
    import backdropPlaceholder from '$lib/assets/placeholder-backdrop.svg';
    import thumbnailPlaceholder from '$lib/assets/placeholder-thumbnail.svg';
    import logoPlaceholder from '$lib/assets/placeholder-logo.svg';

    let {
        itemId,
        type = 'poster',
        size = 'w342',
        alt = '',
        eager = false,
        mediaType = 'movie',
        ratio = type === 'poster' ? '2 / 3' : '16 / 9',
    } = $props();

    let container = $state();
    let source = $state(null);
    let failed = $state(false);
    let placeholder = $derived(({ poster: posterPlaceholder, backdrop: backdropPlaceholder, thumbnail: thumbnailPlaceholder, logo: logoPlaceholder })[type] || posterPlaceholder);

    $effect(() => {
        const id = itemId;
        const artworkType = type;
        const artworkSize = size;
        const target = container;
        const loadImmediately = eager;
        source = null;
        failed = false;
        if (!id || !target) return;

        const controller = new AbortController();
        let objectUrl = null;
        let observer = null;
        let started = false;

        async function load() {
            if (started) return;
            started = true;
            observer?.disconnect();
            try {
                const blob = await getArtwork(id, artworkType, artworkSize, { signal: controller.signal });
                if (controller.signal.aborted) return;
                if (!(blob instanceof Blob) || !blob.type.startsWith('image/')) {
                    failed = true;
                    return;
                }
                objectUrl = URL.createObjectURL(blob);
                source = objectUrl;
            } catch (error) {
                if (!controller.signal.aborted && error.name !== 'AbortError') failed = true;
            }
        }

        if (loadImmediately || typeof IntersectionObserver === 'undefined') {
            load();
        } else {
            observer = new IntersectionObserver((entries) => {
                if (entries.some((entry) => entry.isIntersecting)) load();
            }, { rootMargin: '240px' });
            observer.observe(target);
        }

        return () => {
            controller.abort();
            observer?.disconnect();
            if (objectUrl) URL.revokeObjectURL(objectUrl);
        };
    });
</script>

<div bind:this={container} class="artwork" class:failed style:aspect-ratio={ratio}>
    {#if source && !failed}
        <img src={source} {alt} onerror={() => failed = true} />
    {:else}
        <div class="placeholder" aria-hidden="true">
            <img src={placeholder} alt="" />
        </div>
    {/if}
</div>

<style>
    .artwork {
        width: 100%;
        height: 100%;
        overflow: hidden;
        background: var(--color-bg-elevated);
    }
    img, .placeholder {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }
    .placeholder {
        display: grid;
        place-items: center;
        background: linear-gradient(145deg, var(--color-bg-elevated), var(--color-bg-surface));
        color: var(--color-text-muted);
    }
</style>
