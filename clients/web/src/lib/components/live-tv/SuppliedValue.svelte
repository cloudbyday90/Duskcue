<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    let { value } = $props();
</script>

{#if value !== null && value !== undefined}<bdi lang="">{value}</bdi>{:else}{m.live_tv_unspecified()}{/if}
