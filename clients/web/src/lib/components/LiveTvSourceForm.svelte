<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { onDestroy } from 'svelte';
    import { sourcePair } from '$lib/live-tv-management/forms.js';
    import { m } from '$lib/paraglide/messages.js';
    let { saved, writable, secretEpoch, submit, review, before = async () => true, ondirty = () => {} } = $props();
    let playlist = $state(''); let guide = $state(''); let show = $state(false); let mode = $state('rotate_credentials'); let error = $state(false);
    $effect(() => { const epoch = secretEpoch; playlist = ''; guide = ''; show = false; ondirty(false); });
    onDestroy(() => ondirty(false));
    async function save(event) {
        event.preventDefault(); error = false;
        if (!writable) return;
        let pair; try { pair = sourcePair(playlist, guide); } catch { error = true; return; }
        if (!await before()) return;
        if (saved && mode === 'replace_assignment' && !await review(m.iptv_manage_replace_question(), m.iptv_manage_replace())) return;
        playlist = ''; guide = ''; show = false; ondirty(false);
        await submit(saved ? mode : 'create', pair);
    }
</script>
<section aria-labelledby="source-heading"><h2 id="source-heading">{m.iptv_manage_credentials()}</h2><p id="source-help">{m.iptv_manage_source_help()}</p>
    {#if error}<div role="alert" id="source-error"><p>{m.iptv_manage_source_error()}</p><a href="#playlist-url">{m.iptv_manage_playlist_url()}</a> <a href="#guide-url">{m.iptv_manage_guide_url()}</a></div>{/if}
    <form onsubmit={save} autocomplete="off"><fieldset disabled={!writable}>
        {#if saved}<label for="source-operation">{m.iptv_manage_source_operation()}</label><select id="source-operation" bind:value={mode}><option value="rotate_credentials">{m.iptv_manage_rotate()}</option><option value="replace_assignment">{m.iptv_manage_replace()}</option></select><p>{m.iptv_manage_rotation_help()}</p>{/if}
        <label for="playlist-url">{m.iptv_manage_playlist_url()}</label><input id="playlist-url" type={show ? 'text' : 'password'} autocomplete="off" inputmode="url" autocapitalize="off" autocorrect="off" spellcheck="false" required bind:value={playlist} oninput={() => ondirty(true)} aria-invalid={error} aria-describedby={error ? 'source-error source-help' : 'source-help'} />
        <label for="guide-url">{m.iptv_manage_guide_url()}</label><input id="guide-url" type={show ? 'text' : 'password'} autocomplete="off" inputmode="url" autocapitalize="off" autocorrect="off" spellcheck="false" required bind:value={guide} oninput={() => ondirty(true)} aria-invalid={error} aria-describedby={error ? 'source-error source-help' : 'source-help'} />
        <div class="actions"><button class="secondary-action" type="button" aria-pressed={show} onclick={() => show = !show}>{show ? m.iptv_manage_hide_sources() : m.iptv_manage_show_sources()}</button><button class="primary-action" type="submit">{!saved ? m.iptv_manage_create() : mode === 'replace_assignment' ? m.iptv_manage_replace() : m.iptv_manage_rotate()}</button></div>
    </fieldset></form>
</section>
