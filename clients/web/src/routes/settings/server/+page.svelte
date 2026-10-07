<script>
    import { getContext, onMount } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { initializeDesktopSession } from '$lib/desktop/session.js';
    import DesktopServerSelector from '$lib/components/DesktopServerSelector.svelte';

    const lifecycle = getContext('desktop-server-selection');
    let loading = $state(true);
    let session = $state(null);
    let error = $state('');
    let errorLocale = $state('en');
    let active = true;

    async function load() {
        loading = true;
        error = '';
        try {
            const result = await initializeDesktopSession();
            if (active) session = result;
        } catch (failure) {
            if (active) {
                error = typeof failure === 'string' ? failure : failure.message || m.tonight_desktop_server_load_failed();
                errorLocale = typeof failure === 'string' || failure.message ? 'en' : messageLocale('tonight_desktop_server_load_failed');
            }
        } finally {
            if (active) loading = false;
        }
    }

    onMount(() => { load(); return () => { active = false; }; });
</script>

<div class="server-page">
    {#if loading}
        <p lang={messageLocale('tonight_desktop_server_loading')} role="status">{m.tonight_desktop_server_loading()}</p>
    {:else if error}
        <h1 lang={messageLocale('tonight_desktop_server_switch_title')}>{m.tonight_desktop_server_switch_title()}</h1>
        <p lang={errorLocale} role="alert">{error}</p>
        <button lang={messageLocale('tonight_desktop_server_retry')} type="button" class="tonight-button" onclick={load}>{m.tonight_desktop_server_retry()}</button>
    {:else if session?.isDesktop}
        <DesktopServerSelector server={session.server} onbeforeselect={() => lifecycle?.beforeSelect?.() ?? false} onselected={(result) => lifecycle?.selected?.(result)} onsettled={(result) => lifecycle?.settled?.(result)} />
    {:else}
        <h1 lang={messageLocale('tonight_desktop_server_switch_title')}>{m.tonight_desktop_server_switch_title()}</h1>
        <p lang={messageLocale('tonight_desktop_server_browser')}>{m.tonight_desktop_server_browser()}</p>
    {/if}
</div>

<style>
    .server-page { max-inline-size: 42rem; margin-inline: auto; padding-block: 1rem; display: grid; gap: 1.25rem; }
    h1 { font-size: 1.75rem; }
    p { color: var(--color-text-secondary); line-height: 1.6; }
</style>
