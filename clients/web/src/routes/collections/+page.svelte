<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
</script>

<div class="coming-soon">
    <h1>{m.routes_collections_page_collections()}</h1>
    <p>{m.routes_collections_page_dynamic_collections_curated_lists_and_media_grou()}</p>
    <span class="phase-badge">{m.routes_collections_page_coming_in_phase_12()}</span>
</div>

<style>
    .coming-soon {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        text-align: center;
        padding: 4rem 1rem;
        gap: 0.5rem;
    }
    .coming-soon h1 {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }
    .coming-soon p {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
    }
    .phase-badge {
        margin-top: 0.5rem;
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.25rem 0.75rem;
        border-radius: var(--radius-sm);
    }
</style>
