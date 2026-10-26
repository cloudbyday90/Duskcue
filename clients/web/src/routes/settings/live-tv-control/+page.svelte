<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { getContext } from 'svelte';
    import { auth } from '$lib/stores/auth.js';
    import { createOperatorController } from '$lib/live-tv-management/control-controller.js';
    import Frame from '$lib/components/LiveTvManagementFrame.svelte';
    import { m } from '$lib/paraglide/messages.js';
    const profile = getContext('active-profile')?.(); const commands = getContext('live-tv-management-commands');
    const controller = createOperatorController({ auth, commands, profileId: profile?.id });
    let frame; let enabled = $state(false); let dirty = $state(false);
    const savedControl = $derived($controller.data?.control);
    $effect(() => { enabled = savedControl?.body.enabled || false; dirty = false; });
    const writable = $derived(Boolean($controller.data && $controller.session?.grant.enabled && !$controller.blocked && !$controller.busy && !$commands));
    function discard() { dirty = false; controller.erase(); }
    async function save(event) { event.preventDefault(); if (!writable) return; if (!enabled && !await frame.review(m.iptv_manage_disable_question(), m.iptv_manage_save_control())) return; await controller.configure(enabled); }
</script>
<Frame bind:this={frame} title={m.iptv_manage_control_title()} intro={m.iptv_manage_control_intro()} {controller} {commands} standard={profile?.profile_type === 'standard'} {dirty} ondiscard={discard}>
    {#if $controller.data}
        <section aria-labelledby="refresh-control-heading"><h2 id="refresh-control-heading">{m.iptv_manage_control_title()}</h2><p>{m.iptv_manage_control_help()}</p><p>{m.iptv_manage_not_playback()}</p>
            <p>{m.iptv_manage_revision()}: {$controller.data.control.body.revision}</p>
            <form onsubmit={save}><fieldset disabled={!writable}><label class="check"><input type="checkbox" bind:checked={enabled} onchange={() => dirty = true} />{m.iptv_manage_global_enabled()}</label><button class="primary-action" type="submit">{m.iptv_manage_save_control()}</button></fieldset></form>
        </section>
    {/if}
</Frame>
