<!-- Duskcue — Self-hosted media streaming server
Copyright (C) 2026-2026 Duskcue Contributors
Licensed under AGPL-3.0. See LICENSE for details. -->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    let { phase = '', progressUncertain = false, retryAfter = 0 } = $props();
    const messages = {
        preparing: ['tonight_player_command_preparing', () => m.tonight_player_command_preparing()],
        changing: ['tonight_player_command_changing', () => m.tonight_player_command_changing()],
        recovering: ['tonight_player_command_recovering', () => m.tonight_player_command_recovering()],
        uncertain: ['tonight_player_command_uncertain', () => m.tonight_player_command_uncertain()],
        expired: ['tonight_player_command_expired', () => m.tonight_player_command_expired()],
        status_required: ['tonight_player_command_status_required', () => m.tonight_player_command_status_required()],
        withdrawing: ['tonight_player_command_withdrawing', () => m.tonight_player_command_withdrawing()],
        unavailable: ['tonight_player_command_unavailable', () => m.tonight_player_command_unavailable()],
    };
    let selected = $derived(messages[phase]);
</script>

<div class="command-status" role="status" aria-live="polite" aria-atomic="true">
    {#if selected}<p lang={messageLocale(selected[0])}>{selected[1]()}</p>{/if}
    {#if progressUncertain}<p lang={messageLocale('tonight_player_command_progress_uncertain')}>{m.tonight_player_command_progress_uncertain()}</p>{/if}
    {#if retryAfter > 0}<p lang={messageLocale('tonight_player_command_retry_after')}>{m.tonight_player_command_retry_after()}</p>{/if}
</div>

<style>
    .command-status { position: absolute; bottom: 9rem; inset-inline: 1rem; z-index: 3; pointer-events: none; }
    p { max-width: 35rem; padding: .5rem .75rem; background: rgba(0,0,0,.85); color: var(--color-text-primary); font-size: .85rem; }
    @media (max-height: 500px) { .command-status { position: relative; inset: auto; } }
</style>
