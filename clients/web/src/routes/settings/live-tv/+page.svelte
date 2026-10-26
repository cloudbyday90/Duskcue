<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { getContext } from 'svelte';
    import { auth } from '$lib/stores/auth.js';
    import { createAdministratorController } from '$lib/live-tv-management/admin-controller.js';
    import Frame from '$lib/components/LiveTvManagementFrame.svelte';
    import Source from '$lib/components/LiveTvSourceForm.svelte';
    import Policy from '$lib/components/LiveTvAdministratorPolicy.svelte';
    import Destination from '$lib/components/LiveTvDestinationForm.svelte';
    import Refresh from '$lib/components/LiveTvRefreshPanel.svelte';
    import { m } from '$lib/paraglide/messages.js';
    const profile = getContext('active-profile')?.(); const commands = getContext('live-tv-management-commands');
    const controller = createAdministratorController({ auth, commands, profileId: profile?.id });
    let frame; let marks = $state({ source: false, policy: false, destination: false });
    const dirty = $derived(Object.values(marks).some(Boolean));
    const writable = $derived(Boolean($controller.data && $controller.session?.grant.enabled && !$controller.blocked && !$controller.busy && !$commands));
    const busy = $derived($controller.busy || $commands);
    function discard() { marks = { source: false, policy: false, destination: false }; controller.erase(); }
    async function prepare(own = null) { if (Object.entries(marks).some(([key, value]) => key !== own && value)) { if (!await frame.review(m.iptv_manage_discard_question(), m.iptv_manage_discard())) return false; discard(); } return true; }
    async function select(event) { const value = event.target.value; event.target.value = $controller.data?.target || ''; if (await prepare()) await controller.select(value || null); }
    async function usersPage(page) { if (await prepare()) await controller.usersPage(page); }
    async function source(operation, pair) { if (operation === 'create') await controller.create({ ...pair, enabled: false, max_live_streams: null, allow_owner_profile_access: false, allow_owner_live_limit: false }); else await controller.patch({ operation, ...pair }); }
    async function remove() { if (writable && await prepare() && await frame.review(m.iptv_manage_delete_question(), m.iptv_manage_delete())) await controller.remove(); }
</script>

<Frame bind:this={frame} title={m.iptv_manage_admin_title()} intro={m.iptv_manage_admin_intro()} {controller} {commands} standard={profile?.profile_type === 'standard'} {dirty} ondiscard={discard}>
    {#if $controller.data}
        <section aria-labelledby="households-heading"><h2 id="households-heading">{m.iptv_manage_households()}</h2>
            <label for="household-choice">{m.iptv_manage_choose_household()}</label><select id="household-choice" value={$controller.data.target || ''} disabled={busy} onchange={select}><option value="">{m.iptv_manage_choose_household()}</option>{#each $controller.data.people.items as person}<option value={person.id} lang="">{person.display_name || person.username} ({person.username})</option>{/each}</select>
            {#if !$controller.data.people.items.length}<p>{m.iptv_manage_no_households()}</p>{/if}
            <nav class="actions" aria-label={m.iptv_manage_household_pages()}><button class="secondary-action" type="button" disabled={busy || $controller.data.people.page === 1} onclick={() => usersPage(1)}>{m.iptv_manage_first()}</button><button class="secondary-action" type="button" disabled={busy || $controller.data.people.page === 1} onclick={() => usersPage($controller.data.people.page - 1)}>{m.iptv_manage_previous()}</button><button class="secondary-action" type="button" disabled={busy || $controller.data.people.page >= $controller.data.people.total_pages} onclick={() => usersPage($controller.data.people.page + 1)}>{m.iptv_manage_next()}</button></nav>
            <p>{m.iptv_manage_page({ page: $controller.data.people.page, total: $controller.data.people.total_pages })}</p>
        </section>
        {#if $controller.data.target}
            <p>{m.iptv_manage_selected()}: <bdi lang="">{$controller.data.target}</bdi></p>
            {#if $controller.data.target === $controller.session?.context.user_id}<p>{m.iptv_manage_self_revocation()}</p>{/if}
            {#key $controller.resetEpoch}
                {#if $controller.data.assignment}<Policy value={$controller.data.assignment.body} {writable} submit={controller.patch} before={() => prepare('policy')} ondirty={(value) => marks.policy = value} />{:else}<p>{m.iptv_manage_not_assigned()}</p>{/if}
                <Source saved={Boolean($controller.data.assignment)} {writable} secretEpoch={$controller.secretEpoch} submit={source} review={(...args) => frame.review(...args)} before={() => prepare('source')} ondirty={(value) => marks.source = value} />
                {#if $controller.data.assignment}
                    <Destination value={$controller.data.assignment.body} {writable} secretEpoch={$controller.secretEpoch} submit={controller.patch} review={(...args) => frame.review(...args)} before={() => prepare('destination')} ondirty={(value) => marks.destination = value} />
                    <Refresh resource={$controller.data.refresh.body} observation={$controller.observation} {writable} {busy} {controller} before={() => prepare()} />
                    <button class="secondary-action" type="button" aria-disabled={!writable} onclick={remove}>{m.iptv_manage_delete()}</button>
                {/if}
            {/key}
        {/if}
    {/if}
</Frame>
