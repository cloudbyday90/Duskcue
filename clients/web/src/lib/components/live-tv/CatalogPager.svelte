<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    let { label, page, loading = false, onchange } = $props();
</script>

<nav aria-label={label} class="catalog-pager">
    <button type="button" aria-disabled={loading || page.index === 0} onclick={() => !loading && page.index > 0 && onchange('first')}>{m.live_tv_first()}</button>
    <button type="button" aria-disabled={loading || page.index === 0} onclick={() => !loading && page.index > 0 && onchange('previous')}>{m.live_tv_previous()}</button>
    <span>{m.live_tv_page({ page: page.index + 1 })}</span>
    <button type="button" aria-disabled={loading || !page.canNext} onclick={() => !loading && page.canNext && onchange('next')}>{m.live_tv_next()}</button>
</nav>
{#if page.atLimit}<p>{m.live_tv_history_limit()}</p>{/if}

<style>
    .catalog-pager { display: flex; align-items: center; flex-wrap: wrap; gap: 0.6rem; }
    button { padding: 0.6rem 0.8rem; border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg-elevated); }
    button[aria-disabled='true'] { color: var(--color-text-muted); cursor: default; }
    p { color: var(--color-text-secondary); }
</style>
