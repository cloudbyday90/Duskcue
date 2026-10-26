<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { textLanguage } from '$lib/live-tv/presentation.js';
    import SuppliedValue from './SuppliedValue.svelte';
    let { label, variants } = $props();
</script>

{#if variants.length}
    <details>
        <summary>{label}</summary>
        <ul>
            {#each variants as variant}
                <li><p><bdi lang={textLanguage(variant.language)}>{variant.text}</bdi></p>
                    <small>{m.live_tv_language()}: <SuppliedValue value={variant.language} /></small></li>
            {/each}
        </ul>
    </details>
{/if}

<style>
    details { min-width: 0; }
    summary { cursor: pointer; padding-block: 0.65rem; }
    ul { padding-inline-start: 1.4rem; display: grid; gap: 0.7rem; }
    p { white-space: pre-wrap; overflow-wrap: anywhere; }
    small { color: var(--color-text-secondary); overflow-wrap: anywhere; }
</style>
