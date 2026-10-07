<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { goto } from '$app/navigation';
    import { onDestroy } from 'svelte';
    import { SEARCH_DEBOUNCE_MS } from '../utils/constants.js';

    let {
        value = $bindable(''),
        placeholder = 'Search movies, shows...',
        compact = false,
        autofocus = false,
        onsearch = null,
        oninput = null,
        navigate = true,
    } = $props();

    let debounceTimer = null;
    let inputEl = null;

    onDestroy(() => clearTimeout(debounceTimer));

    $effect(() => {
        if (autofocus && inputEl) {
            inputEl.focus();
        }
    });

    function handleInput(event) {
        value = event.target.value;

        if (oninput) {
            if (debounceTimer) clearTimeout(debounceTimer);
            debounceTimer = setTimeout(() => {
                oninput(value);
            }, SEARCH_DEBOUNCE_MS);
        }
    }

    function handleSubmit(event) {
        event.preventDefault();
        const query = value.trim();
        if (!query) return;

        if (debounceTimer) {
            clearTimeout(debounceTimer);
            debounceTimer = null;
        }

        if (onsearch) {
            onsearch(query);
        }

        if (navigate) {
            goto(`/search?q=${encodeURIComponent(query)}`);
        }
    }
</script>

<form class="search-bar" class:compact onsubmit={handleSubmit} role="search">
    <div class="search-input-wrapper">
        <button type="submit" class="search-submit" aria-label={m.lib_components_searchbar_search()}><svg
            class="search-icon"
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <circle cx="11" cy="11" r="8" />
            <path d="M21 21l-4.35-4.35" />
        </svg></button>
        <input
            bind:this={inputEl}
            type="search"
            class="search-input"
            {placeholder}
            value={value}
            oninput={handleInput}
            aria-label={m.lib_components_searchbar_search()}
            autocomplete="off"
            spellcheck="false"
        />
    </div>
</form>

<style>
    .search-submit { flex: 0 0 44px; width: 44px; min-height: 44px; padding: 0; display: grid; place-items: center; border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg-surface); }
    .search-bar {
        width: 100%;
        max-width: 480px;
    }

    .search-input-wrapper {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .search-icon {
        color: var(--color-text-muted);
        pointer-events: none;
    }

    .search-input {
        width: 100%;
        flex: 1;
        min-width: 0;
        min-height: 44px;
        padding-block: 0.625rem;
        padding-inline: 1rem;
        font-size: 0.9375rem;
        color: var(--color-text-primary);
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .search-input::placeholder {
        color: var(--color-text-muted);
    }

    .search-input:focus {
        border-color: var(--color-accent);
        background-color: var(--color-bg-elevated);
    }

    .search-input::-webkit-search-cancel-button {
        appearance: none;
        width: 16px;
        height: 16px;
        cursor: pointer;
        background: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='%236b6c75' stroke-width='2' stroke-linecap='round'%3E%3Cpath d='M18 6L6 18M6 6l12 12'/%3E%3C/svg%3E") center no-repeat;
    }

    .compact {
        max-width: 240px;
    }

    .compact .search-input {
        padding-block: 0.5rem;
        padding-inline: 0.875rem;
        font-size: 0.875rem;
    }

    .compact .search-submit .search-icon {
        width: 16px;
        height: 16px;
    }

    @media (max-width: 768px) {
        .search-bar {
            max-width: none;
        }

        .compact {
            max-width: none;
        }
    }
</style>
