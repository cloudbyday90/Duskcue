<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { goto } from '$app/navigation';
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
        <svg
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
        </svg>
        <input
            bind:this={inputEl}
            type="search"
            class="search-input"
            {placeholder}
            value
            oninput={handleInput}
            aria-label="Search"
            autocomplete="off"
            spellcheck="false"
        />
    </div>
</form>

<style>
    .search-bar {
        width: 100%;
        max-width: 480px;
    }

    .search-input-wrapper {
        position: relative;
        display: flex;
        align-items: center;
    }

    .search-icon {
        position: absolute;
        left: 0.75rem;
        color: var(--color-text-muted);
        pointer-events: none;
        z-index: 1;
    }

    .search-input {
        width: 100%;
        padding: 0.625rem 1rem 0.625rem 2.5rem;
        font-size: 0.9375rem;
        color: var(--color-text-primary);
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
        outline: none;
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
        padding: 0.5rem 0.875rem 0.5rem 2.25rem;
        font-size: 0.875rem;
    }

    .compact .search-icon {
        left: 0.625rem;
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
