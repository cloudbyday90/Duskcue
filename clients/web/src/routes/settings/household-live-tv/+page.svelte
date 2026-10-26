<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { getContext } from 'svelte';
    import { auth } from '$lib/stores/auth.js';
    import { createOwnerPolicyController } from '$lib/live-tv-management/owner-controller.js';
    import { parseCap } from '$lib/live-tv-management/schema.js';
    import Frame from '$lib/components/LiveTvManagementFrame.svelte';
    import { m } from '$lib/paraglide/messages.js';
    const profile = getContext('active-profile')?.(); const commands = getContext('live-tv-management-commands');
    const controller = createOwnerPolicyController({ auth, commands, profileId: profile?.id });
    let frame; let cap = $state(''); let dirty = $state(false); let error = $state(false);
    const writable = $derived(Boolean($controller.data && $controller.session?.grant.enabled && !$controller.blocked && !$controller.busy && !$commands));
    const busy = $derived($controller.busy || $commands);
    const savedAssignment = $derived($controller.data?.assignment);
    $effect(() => { const value = savedAssignment?.body.owner_max_live_streams; cap = value == null ? '' : String(value); dirty = false; });
    function discard() { dirty = false; controller.erase(); }
    async function prepare() { if (dirty && !await frame.review(m.iptv_manage_discard_question(), m.iptv_manage_discard())) return false; discard(); return true; }
    async function save(event) { event.preventDefault(); if (!writable) return; error = false; let value; try { value = parseCap(cap); } catch { error = true; return; } await controller.patch({ operation: 'set_live_limit', max_live_streams: value }); }
    async function toggle(person, event) { const enabled = event.target.checked; event.target.checked = person.enabled; if (writable && await prepare()) await controller.patch({ operation: 'set_profile_access', profile_id: person.id, enabled }); }
    async function page(direction) { if (await prepare()) await controller.page(direction); }
</script>
<Frame bind:this={frame} title={m.iptv_manage_owner_title()} intro={m.iptv_manage_owner_intro()} {controller} {commands} standard={profile?.profile_type === 'standard'} {dirty} ondiscard={discard}>
    {#if $controller.data}
        <section aria-labelledby="owner-profiles-heading"><h2 id="owner-profiles-heading">{m.iptv_manage_profiles()}</h2><p>{m.iptv_manage_profile_help()}</p>
            {#if !$controller.data.assignment.body.enabled || !$controller.data.assignment.body.allow_owner_profile_access}<p>{m.iptv_manage_delegation_missing()}</p>{/if}
            {#each $controller.data.profiles.items as person}<label class="check"><input type="checkbox" checked={person.enabled} disabled={!writable || !person.eligible || person.profile_type !== 'standard' || !$controller.data.assignment.body.enabled || !$controller.data.assignment.body.allow_owner_profile_access} aria-label={m.iptv_manage_profile_enabled({ name: person.name })} onchange={(event) => toggle(person, event)} /><bdi lang="">{person.name}</bdi> ({person.profile_type})</label>{/each}
            {#if !$controller.data.profiles.items.length}<p>{m.iptv_manage_no_profiles()}</p>{/if}
            <nav class="actions" aria-label={m.iptv_manage_profile_pages()}><button class="secondary-action" type="button" disabled={busy || $controller.data.position === 0} onclick={() => page('first')}>{m.iptv_manage_first()}</button><button class="secondary-action" type="button" disabled={busy || $controller.data.position === 0} onclick={() => page('previous')}>{m.iptv_manage_previous()}</button><button class="secondary-action" type="button" disabled={busy || !$controller.data.profiles.next_cursor || $controller.data.fullHistory} onclick={() => page('next')}>{m.iptv_manage_next()}</button></nav>
        </section>
        <section aria-labelledby="owner-cap-heading"><h2 id="owner-cap-heading">{m.iptv_manage_owner_cap()}</h2><p id="owner-cap-help">{m.iptv_manage_cap_help()}</p>
            {#if !$controller.data.assignment.body.enabled || !$controller.data.assignment.body.allow_owner_live_limit}<p>{m.iptv_manage_delegation_missing()}</p>{/if}
            <form onsubmit={save}><fieldset disabled={!writable || !$controller.data.assignment.body.enabled || !$controller.data.assignment.body.allow_owner_live_limit}>
                <label for="owner-live-cap">{m.iptv_manage_owner_cap()}</label><input id="owner-live-cap" inputmode="numeric" maxlength="10" bind:value={cap} oninput={() => dirty = true} aria-describedby="owner-cap-help" aria-invalid={error} />
                {#if error}<p role="alert">{m.iptv_manage_cap_error()}</p>{/if}<button class="primary-action" type="submit">{m.iptv_manage_save_owner_cap()}</button>
            </fieldset></form>
        </section>
    {/if}
</Frame>
