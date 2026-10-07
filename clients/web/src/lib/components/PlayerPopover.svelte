<script>
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { onMount, tick } from 'svelte';

    let { id, open = false, label, labelLocale = undefined, children } = $props();
    let panel = $state();
    let focusFrame = null;
    let lastLayout = '';

    function cancelFocusFrame() {
        if (focusFrame !== null) cancelAnimationFrame(focusFrame);
        focusFrame = null;
    }

    function revealFocusedChoice() {
        const target = document.activeElement;
        if (!(target instanceof HTMLElement) || !panel?.contains(target) || target === panel) return;
        const bounds = panel.getBoundingClientRect();
        const box = target.getBoundingClientRect();
        const top = bounds.top + panel.clientTop + 2;
        const bottom = bounds.top + panel.clientTop + panel.clientHeight - 2;
        if (box.height <= bottom - top && box.bottom > bottom) panel.scrollTop += box.bottom - bottom;
        else if (box.top < top) panel.scrollTop += box.top - top;
    }

    function place() {
        if (!open || !panel?.isConnected) return;
        const owner = panel.closest('.player-container');
        const bounds = owner?.getBoundingClientRect();
        const left = Math.max(0, bounds?.left ?? 0) + 8;
        const right = Math.min(document.documentElement.clientWidth, bounds?.right ?? window.innerWidth) - 8;
        const shortHeight = window.innerHeight <= 420;
        const headingBottom = shortHeight ? owner?.querySelector('.player-heading')?.getBoundingClientRect().bottom ?? 0 : 0;
        const top = Math.max(0, bounds?.top ?? 0, headingBottom) + 8;
        const bottom = Math.min(window.innerHeight, bounds?.bottom ?? window.innerHeight) - 8;
        panel.style.transform = '';
        panel.style.insetBlockEnd = shortHeight ? `${window.innerHeight - bottom}px` : '';
        panel.style.maxInlineSize = `${Math.max(0, right - left)}px`;
        const box = panel.getBoundingClientRect();
        panel.style.maxBlockSize = shortHeight
            ? `min(20rem, ${Math.max(0, bottom - top)}px)`
            : `min(20rem, 55dvh, ${Math.max(0, box.bottom - top)}px)`;
        const shift = box.left < left ? left - box.left : box.right > right ? right - box.right : 0;
        panel.style.transform = `translateX(${shift}px)`;
        revealFocusedChoice();
        const finalBounds = panel.getBoundingClientRect();
        const layout = [finalBounds.left, finalBounds.top, finalBounds.width, finalBounds.height].map((value) => value.toFixed(3)).join(' ');
        if (layout !== lastLayout) {
            lastLayout = layout;
            panel.dispatchEvent(new Event('duskcue:player-popover-layout', { bubbles: true }));
        }
    }

    function handleFocus() {
        tick().then(() => {
            if (!open || !panel?.isConnected) return;
            cancelFocusFrame();
            focusFrame = requestAnimationFrame(() => { focusFrame = null; place(); });
        });
    }

    $effect(() => {
        if (!open) return;
        let current = true;
        tick().then(() => { if (current) place(); });
        return () => { current = false; cancelFocusFrame(); };
    });

    onMount(() => {
        const observer = new ResizeObserver(place);
        const owner = panel.closest('.player-container');
        if (owner) observer.observe(owner);
        window.addEventListener('resize', place);
        document.addEventListener('fullscreenchange', place);
        return () => {
            cancelFocusFrame();
            observer.disconnect();
            window.removeEventListener('resize', place);
            document.removeEventListener('fullscreenchange', place);
        };
    });
</script>

<section lang={labelLocale} {id} class="player-popover" bind:this={panel} hidden={!open} aria-label={label} onfocusin={handleFocus}>
    <h2>{label}</h2>
    <div lang={getLocale()}>{@render children()}</div>
</section>

<style>
    .player-popover { position: absolute; inset-block-end: calc(100% + 0.75rem); inset-inline-end: 0; z-index: 12; box-sizing: border-box; inline-size: min(22rem, calc(100vw - 2rem)); max-block-size: min(20rem, 55dvh); padding: 0.75rem; overflow: auto; overscroll-behavior: contain; scrollbar-gutter: stable; background: var(--color-bg-elevated); color: var(--color-text-primary); border: 1px solid var(--color-border); border-radius: var(--radius-md); box-shadow: var(--shadow-elevated); }
    .player-popover[hidden] { display: none; }
    h2 { font-size: 1rem; font-weight: 600; line-height: 1.4; margin-block-end: 0.65rem; }
    @media (prefers-reduced-motion: reduce) { .player-popover { scroll-behavior: auto; } }
    @media (max-height: 420px) { .player-popover { position: fixed; inset-block-end: 8px; inset-inline-end: 8px; max-block-size: calc(100dvh - 16px); } }
    @media (max-height: 420px) and (min-width: 600px) { .player-popover { inline-size: 15rem; } }
</style>
