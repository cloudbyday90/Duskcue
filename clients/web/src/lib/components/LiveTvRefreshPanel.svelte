<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { observationText } from '$lib/live-tv-management/presentation.js';
    import { formatInstant } from '$lib/time/presentation.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    let { resource, observation, writable, busy, controller, before = async () => true } = $props();
    let playlist = $state(''); let guide = $state(''); let error = $state(false);
    let statusText = $derived(observationText(observation));
    const pinContext = $derived([resource.assignment_id, resource.revision, resource.source_generation, resource.control.revision, resource.latest_attempt_id, resource.playlist_address_count, resource.guide_address_count].join(':'));
    $effect(() => { const value = pinContext; playlist = ''; guide = ''; });
    const ready = $derived(writable && observation?.service_available !== false && resource.control.enabled && resource.assignment_enabled && resource.playlist_address_count > 0 && resource.guide_address_count > 0);
    async function start(event) { event.preventDefault(); if (!ready) return; error = false; if (playlist === '' || guide === '') { error = true; return; } if (await before()) await controller.start(Number(playlist), Number(guide)); }
</script>
<section aria-labelledby="refresh-heading"><h2 id="refresh-heading">{m.iptv_manage_refresh()}</h2><p>{m.iptv_manage_refresh_help()}</p><p>{m.iptv_manage_watch_help()}</p>
    {#if !resource.control.enabled}<p>{m.iptv_manage_global_disabled()}</p>{/if}
    {#if observation?.service_available === false}<p>{m.iptv_manage_service_unavailable()}</p>{/if}
    <form onsubmit={start}><fieldset disabled={!ready}>
        <label for="playlist-pin">{m.iptv_manage_playlist_pin()}</label><select id="playlist-pin" required bind:value={playlist}><option value="">{m.iptv_manage_choose_pin()}</option>{#each Array(resource.playlist_address_count) as _, index}<option value={String(index)}>{m.iptv_manage_pin({ number: index + 1 })}</option>{/each}</select>
        <label for="guide-pin">{m.iptv_manage_guide_pin()}</label><select id="guide-pin" required bind:value={guide}><option value="">{m.iptv_manage_choose_pin()}</option>{#each Array(resource.guide_address_count) as _, index}<option value={String(index)}>{m.iptv_manage_pin({ number: index + 1 })}</option>{/each}</select>
        {#if error}<p role="alert">{m.iptv_manage_choose_pin()}</p>{/if}<button class="primary-action" type="submit">{m.iptv_manage_start()}</button>
    </fieldset></form>
    <div class="actions"><button class="secondary-action" type="button" aria-disabled={busy} onclick={() => !busy && controller.checkStatus()}>{m.iptv_manage_check_status()}</button><button class="secondary-action" type="button" onclick={() => controller.stopWatching()}>{m.iptv_manage_stop_watching()}</button></div>
    <h3>{m.iptv_manage_latest()}</h3><p role="status" aria-live="polite" aria-atomic="true">{statusText}</p>
    {#if observation?.latest}<p><bdi lang="">{observation.latest.attempt_id}</bdi></p><p>{m.iptv_manage_counts({ channels: observation.latest.channel_count, guides: observation.latest.guide_count, programmes: observation.latest.programme_count })}</p>
        <p>{m.iptv_manage_current_source()}: {observation.latest.current_source ? m.iptv_manage_state_enabled() : m.iptv_manage_state_disabled()}</p>
        <p>{m.iptv_manage_attempted()}: {formatInstant(observation.latest.attempted_at, getLocale())}</p>{#if observation.latest.imported_at}<p>{m.iptv_manage_imported()}: {formatInstant(observation.latest.imported_at, getLocale())}</p>{/if}
    {/if}
</section>
