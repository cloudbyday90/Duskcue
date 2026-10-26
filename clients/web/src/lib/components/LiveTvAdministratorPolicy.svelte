<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { onDestroy, untrack } from 'svelte';
    import { parseCap } from '$lib/live-tv-management/schema.js';
    import { m } from '$lib/paraglide/messages.js';
    let { value, writable, submit, before = async () => true, ondirty = () => {} } = $props();
    let enabled = $state(false); let cap = $state(''); let profiles = $state(false); let limit = $state(false); let error = $state(false);
    const policyContext = $derived([value.assignment_id, value.revision, value.source_generation].join(':'));
    $effect(() => { const key = policyContext; untrack(() => { enabled = value.enabled; cap = value.max_live_streams === null ? '' : String(value.max_live_streams); profiles = value.allow_owner_profile_access; limit = value.allow_owner_live_limit; ondirty(false); }); });
    onDestroy(() => ondirty(false));
    async function save(event) {
        event.preventDefault(); if (!writable) return; error = false;
        let max; try { max = parseCap(cap); } catch { error = true; return; }
        if (await before()) await submit({ operation: 'configure', enabled, max_live_streams: max, allow_owner_profile_access: profiles, allow_owner_live_limit: limit });
    }
</script>
<section aria-labelledby="admin-policy-heading"><h2 id="admin-policy-heading">{m.iptv_manage_assignment()}</h2>
    <div class="metadata"><p>{m.iptv_manage_assignment_id()}: <bdi lang="">{value.assignment_id}</bdi></p><p>{m.iptv_manage_revision()}: {value.revision}</p><p>{m.iptv_manage_generation()}: {value.source_generation}</p></div><p>{m.iptv_manage_configured_help()}</p>
    <form onsubmit={save} oninput={() => ondirty(true)}><fieldset disabled={!writable}>
        <label class="check"><input type="checkbox" bind:checked={enabled} />{m.iptv_manage_enabled()}</label>
        <label for="administrator-live-cap">{m.iptv_manage_admin_cap()}</label><input id="administrator-live-cap" type="text" inputmode="numeric" maxlength="10" bind:value={cap} aria-invalid={error} aria-describedby="administrator-cap-help" /><p id="administrator-cap-help">{error ? m.iptv_manage_cap_error() : m.iptv_manage_cap_help()}</p>
        {#if error}<p role="alert">{m.iptv_manage_cap_error()}</p>{/if}
        <label class="check"><input type="checkbox" bind:checked={profiles} />{m.iptv_manage_delegate_profiles()}</label><label class="check"><input type="checkbox" bind:checked={limit} />{m.iptv_manage_delegate_cap()}</label>
        <button class="primary-action" type="submit">{m.iptv_manage_save_policy()}</button>
    </fieldset></form>
</section>
