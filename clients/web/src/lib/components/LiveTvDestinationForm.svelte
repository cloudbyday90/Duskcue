<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { onDestroy } from 'svelte';
    import { destination } from '$lib/live-tv-management/forms.js';
    import { m } from '$lib/paraglide/messages.js';
    let { value, writable, secretEpoch, submit, review, before = async () => true, ondirty = () => {} } = $props();
    let feed = $state('playlist'); let origin = $state(''); let addresses = $state(''); let error = $state(false);
    function erase() { origin = ''; addresses = ''; ondirty(false); }
    $effect(() => { const epoch = secretEpoch; erase(); });
    onDestroy(() => ondirty(false));
    async function save(event) {
        event.preventDefault(); if (!writable) return; error = false;
        let data; try { data = destination(origin, addresses); } catch { error = true; return; }
        if (!await before()) return;
        erase(); await submit({ operation: 'approve_destination', feed, ...data });
    }
    async function revoke() { if (writable && await before() && await review(m.iptv_manage_revoke_question(), m.iptv_manage_revoke())) { erase(); await submit({ operation: 'revoke_destination', feed }); } }
</script>
<section aria-labelledby="destination-heading"><h2 id="destination-heading">{m.iptv_manage_destination()}</h2><p id="destination-help">{m.iptv_manage_destination_help()}</p>
    <p>{m.iptv_manage_approval({ feed: m.iptv_manage_playlist(), count: value.playlist_address_count })}</p><p>{m.iptv_manage_approval({ feed: m.iptv_manage_guide(), count: value.guide_address_count })}</p>
    {#if error}<div role="alert" id="destination-error"><p>{m.iptv_manage_destination_error()}</p><a href="#feed-origin">{m.iptv_manage_origin()}</a> <a href="#feed-addresses">{m.iptv_manage_addresses()}</a></div>{/if}
    <form onsubmit={save} autocomplete="off"><fieldset disabled={!writable}>
        <label for="destination-feed">{m.iptv_manage_feed()}</label><select id="destination-feed" bind:value={feed} onchange={erase}><option value="playlist">{m.iptv_manage_playlist()}</option><option value="guide">{m.iptv_manage_guide()}</option></select>
        <label for="feed-origin">{m.iptv_manage_origin()}</label><input id="feed-origin" type="text" inputmode="url" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false" required bind:value={origin} oninput={() => ondirty(true)} aria-invalid={error} aria-describedby={error ? 'destination-error destination-help' : 'destination-help'} />
        <label for="feed-addresses">{m.iptv_manage_addresses()}</label><textarea id="feed-addresses" autocomplete="off" autocapitalize="off" {...{ autocorrect: 'off' }} spellcheck="false" required bind:value={addresses} oninput={() => ondirty(true)} aria-invalid={error} aria-describedby={error ? 'destination-error destination-help' : 'destination-help'}></textarea>
        <div class="actions"><button class="primary-action" type="submit">{m.iptv_manage_approve()}</button><button class="secondary-action" type="button" onclick={revoke}>{m.iptv_manage_revoke()}</button></div>
    </fieldset></form>
</section>
