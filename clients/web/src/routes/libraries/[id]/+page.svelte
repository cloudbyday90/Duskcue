<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { page } from '$app/stores';
    import BrowseScopeGallery from '$lib/components/BrowseScopeGallery.svelte';
</script>

<BrowseScopeGallery kind="library" itemId={$page.params.id} />
