<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<div class="coming-soon">
    <a href="/settings" class="back-link">← Settings</a>
    <h1>Storage & Cache</h1>
    <p>Three-tier storage, cache limits, and disk monitoring.</p>
    <span class="phase-badge">Coming in Phase 13</span>
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
    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
        margin-bottom: 1rem;
    }
    .back-link:hover {
        color: var(--color-text-secondary);
    }
    .coming-soon h1 {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }
    .coming-soon p {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        max-width: 400px;
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