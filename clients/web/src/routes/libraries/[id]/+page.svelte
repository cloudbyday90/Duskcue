<script>
    import { page } from '$app/stores';
    import BrowseScopeGallery from '$lib/components/BrowseScopeGallery.svelte';
</script>

<BrowseScopeGallery kind="library" itemId={$page.params.id} />
