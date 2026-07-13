<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount, onDestroy } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { getMediaItem, listMediaFiles } from '$lib/api/media.js';
    import { getWatchData } from '$lib/api/playback.js';
    import { player } from '$lib/stores/player.js';
    import { notifications } from '$lib/stores/notifications.js';
    import Player from '$lib/components/Player.svelte';

    let itemId = $derived($page.params.id);
    let loading = $state(true);
    let item = $state(null);
    let mediaFileId = $state(null);
    let startPositionMs = $state(0);

    onMount(async () => {
        const queryFileId = $page.url.searchParams.get('file');
        try {
            const [itemData, filesData] = await Promise.all([
                getMediaItem(itemId),
                listMediaFiles(itemId),
            ]);
            item = itemData;
            const files = filesData.items || filesData || [];
            const healthyFiles = files.filter((file) => file.is_healthy !== false);
            if (!healthyFiles.length) {
                notifications.error(m.routes_play_id_page_no_playable_files_found());
                goto(`/media/${itemId}`);
                return;
            }
            const selected = healthyFiles.find((file) => file.id === queryFileId) || healthyFiles[0];
            mediaFileId = selected.id;

            try {
                const wd = await getWatchData(itemId);
                startPositionMs = wd.resume_position_ms || 0;
            } catch {
            }
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_play_id_page_failed_to_load_media());
            goto(`/media/${itemId}`);
        } finally {
            loading = false;
        }
    });

    onDestroy(() => {
        player.destroy();
    });

    function handleStop() {
        goto(`/media/${itemId}`);
    }
</script>

<div class="player-route">
    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
            <p>{m.routes_play_id_page_loading_player()}</p>
        </div>
    {:else if item && mediaFileId}
        <Player
            mediaItem={item}
            {mediaFileId}
            {startPositionMs}
            onstop={handleStop}
        />
    {/if}
</div>

<style>
    .player-route {
        position: fixed;
        inset: 0;
        background-color: #000;
        z-index: 1000;
    }

    .loading-state {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 1rem;
        color: var(--color-text-muted);
    }

    .loading-spinner {
        width: 32px;
        height: 32px;
        border: 3px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }
</style>
