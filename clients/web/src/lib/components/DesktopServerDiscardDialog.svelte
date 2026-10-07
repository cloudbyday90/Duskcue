<script>
    import { onDestroy } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';

    const identity = $props.id();
    let dialog;
    let invoker;
    let resolve;

    export function confirm() {
        if (resolve) return Promise.resolve(false);
        return new Promise((complete) => {
            resolve = complete;
            invoker = document.activeElement;
            dialog.returnValue = 'keep';
            dialog.showModal();
            dialog.querySelector('button[value="keep"]')?.focus();
        });
    }

    function finish() {
        const complete = resolve;
        resolve = null;
        if (invoker instanceof HTMLElement && invoker.isConnected) invoker.focus();
        complete?.(dialog.returnValue === 'discard');
    }

    onDestroy(() => { resolve?.(false); resolve = null; });
</script>

<dialog lang={messageLocale('tonight_desktop_server_discard_title')} bind:this={dialog} aria-labelledby={`${identity}-title`} aria-describedby={`${identity}-copy`} onclose={finish}>
    <h2 lang={messageLocale('tonight_desktop_server_discard_title')} id={`${identity}-title`}>{m.tonight_desktop_server_discard_title()}</h2>
    <p lang={messageLocale('tonight_desktop_server_discard_copy')} id={`${identity}-copy`}>{m.tonight_desktop_server_discard_copy()}</p>
    <form method="dialog">
        <button lang={messageLocale('tonight_desktop_server_keep')} class="tonight-button secondary" value="keep">{m.tonight_desktop_server_keep()}</button>
        <button lang={messageLocale('tonight_desktop_server_discard')} class="tonight-button" value="discard">{m.tonight_desktop_server_discard()}</button>
    </form>
</dialog>

<style>
    dialog { margin: auto; max-inline-size: min(30rem, calc(100vw - 2rem)); padding: 1.5rem; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-bg-surface); color: var(--color-text-primary); }
    dialog::backdrop { background: rgba(0, 0, 0, 0.75); }
    h2 { font-size: 1.25rem; margin-block-end: 0.75rem; }
    p { color: var(--color-text-secondary); line-height: 1.6; }
    form { display: flex; flex-wrap: wrap; gap: 0.75rem; margin-block-start: 1.25rem; }
    button:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; }
</style>
