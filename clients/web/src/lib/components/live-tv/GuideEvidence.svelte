<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import TextEvidence from './TextEvidence.svelte';
    let { guide } = $props();
</script>

<div class="guide-evidence">
    {#if !guide}<p>{m.live_tv_no_guide()}</p>
    {:else}
        {#if guide.undeclared}<p>{m.live_tv_undeclared()}</p>{/if}
        {#if guide.ambiguous}<p>{m.live_tv_ambiguous()}</p>{/if}
        {#each guide.declarations as declaration}
            <TextEvidence label={m.live_tv_guide_names()} variants={declaration.display_names} />
        {/each}
    {/if}
</div>

<style>.guide-evidence { color: var(--color-text-secondary); overflow-wrap: anywhere; }</style>
