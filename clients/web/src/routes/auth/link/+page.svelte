<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { verifyDeviceCode } from '$lib/api/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let code = $state('');
    let loading = $state(false);

    let urlCode = $derived($page.url.searchParams.get('code') || '');

    $effect(() => {
        if (urlCode) {
            code = urlCode;
        }
    });

    function formatUserCode(raw) {
        const clean = raw.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 8);
        if (clean.length <= 4) return clean;
        return clean.slice(0, 4) + '-' + clean.slice(4);
    }

    let formattedCode = $derived(formatUserCode(code));

    async function handleVerify(e) {
        e.preventDefault();
        const clean = code.toUpperCase().replace(/[^A-Z0-9]/g, '');
        if (clean.length < 6) {
            notifications.error(m.routes_auth_link_page_please_enter_a_valid_device_code());
            return;
        }
        loading = true;
        try {
            await verifyDeviceCode({ user_code: clean });
            notifications.success(m.routes_auth_link_page_device_authorized_successfully());
            goto('/dashboard');
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_auth_link_page_device_verification_failed());
        } finally {
            loading = false;
        }
    }
</script>

<div class="auth-page">
    <div class="auth-card">
        <div class="auth-header">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="margin: 0 auto 1rem;">
                <rect x="3" y="4" width="18" height="12" rx="2" />
                <line x1="2" y1="20" x2="22" y2="20" />
            </svg>
            <h1 class="auth-title">{m.routes_auth_link_page_link_a_device()}</h1>
            <p class="auth-subtitle">
                Enter the code displayed on your device to authorize it.
            </p>
        </div>

        <form onsubmit={handleVerify} class="auth-form">
            <label class="field">
                <span class="field-label">{m.routes_auth_link_page_device_code()}</span>
                <input
                    type="text"
                    value={formattedCode}
                    oninput={(e) => (code = e.currentTarget.value)}
                    placeholder={m.routes_auth_link_page_abcd_efgh()}
                    autocomplete="off"
                    class="code-input"
                    required
                />
            </label>

            <button type="submit" class="btn-primary" disabled={loading || code.length < 6}>
                {loading ? 'Verifying…' : 'Authorize Device'}
            </button>
        </form>

        <div class="link-info">
            <p>
                Device codes are 8 characters, displayed in two groups of four (e.g., ABCD-EFGH).
                Codes are case-insensitive and expire after 15 minutes.
            </p>
        </div>
    </div>
</div>

<style>
    .auth-page {
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: calc(100vh - 56px);
        padding: 2rem 1rem;
    }

    .auth-card {
        width: min(100%, 420px);
        min-width: 0;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-lg);
        padding: 2.5rem;
    }

    .auth-header {
        text-align: center;
        margin-bottom: 2rem;
    }

    .auth-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-accent);
    }

    .auth-subtitle {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        margin-top: 0.5rem;
    }

    .auth-form {
        display: flex;
        flex-direction: column;
        gap: 1.25rem;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
    }

    .field-label {
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .code-input {
        width: 100%;
        padding: 0.875rem 1rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 1.5rem;
        font-weight: 600;
        font-family: monospace;
        letter-spacing: 0.15em;
        text-align: center;
        text-transform: uppercase;
        transition: border-color var(--transition-fast);
    }

    .code-input:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .code-input::placeholder {
        color: var(--color-text-muted);
        font-weight: 400;
    }

    .btn-primary {
        width: 100%;
        padding: 0.75rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.875rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-primary:hover:not(:disabled) {
        background-color: var(--color-accent-hover);
    }

    .btn-primary:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .link-info {
        margin-top: 1.5rem;
        padding: 0.75rem;
        background-color: var(--color-info-bg);
        border-radius: var(--radius-sm);
    }

    .link-info p {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        line-height: 1.5;
    }

    @media (max-width: 480px) {
        .auth-card {
            padding: 1.5rem;
        }

        .auth-page {
            padding: 1rem 0.75rem;
        }
    }
</style>
