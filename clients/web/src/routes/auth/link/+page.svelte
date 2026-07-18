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
    import { getDeviceLinkingRequest, verifyDeviceCode } from '$lib/api/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let code = $state('');
    let loading = $state(false);
    let request = $state(null);

    let urlCode = $derived($page.url.searchParams.get('code') || $page.url.searchParams.get('user_code') || '');

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

    async function handleReview(e) {
        e.preventDefault();
        const clean = code.toUpperCase().replace(/[^A-Z0-9]/g, '');
        if (clean.length !== 8) {
            notifications.error(m.routes_auth_link_page_please_enter_a_valid_device_code());
            return;
        }
        loading = true;
        try {
            request = await getDeviceLinkingRequest({ user_code: clean });
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_auth_link_page_device_verification_failed());
        } finally {
            loading = false;
        }
    }

    async function handleDecision(approve) {
        if (!request || loading) return;
        loading = true;
        try {
            await verifyDeviceCode({ user_code: request.user_code, approve });
            notifications.success(
                approve
                    ? m.routes_auth_link_page_device_authorized_successfully()
                    : 'Device authorization cancelled',
            );
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

        {#if request}
            <section class="device-review" aria-live="polite">
                <h2>Review device</h2>
                <dl>
                    <div><dt>Code</dt><dd>{request.user_code}</dd></div>
                    <div><dt>App</dt><dd>{request.client_name || 'Unknown app'}</dd></div>
                    <div><dt>Platform</dt><dd>{request.client_platform || 'Unknown platform'}</dd></div>
                    {#if request.client_version}<div><dt>Version</dt><dd>{request.client_version}</dd></div>{/if}
                </dl>
                <p>Confirm that this code matches the one displayed on the device before you continue.</p>
                <div class="review-actions">
                    <button class="btn-secondary" onclick={() => (request = null)} disabled={loading}>Back</button>
                    <button class="btn-secondary deny" onclick={() => handleDecision(false)} disabled={loading}>Deny</button>
                    <button class="btn-primary" onclick={() => handleDecision(true)} disabled={loading}>
                        {loading ? 'Authorizing…' : 'Authorize Device'}
                    </button>
                </div>
            </section>
        {:else}
            <form onsubmit={handleReview} class="auth-form">
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

                <button type="submit" class="btn-primary" disabled={loading || code.length < 8}>
                    {loading ? 'Checking…' : 'Continue'}
                </button>
            </form>
        {/if}

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

    .device-review {
        display: grid;
        gap: 1rem;
    }

    .device-review h2 {
        font-size: 1rem;
        color: var(--color-text-primary);
    }

    .device-review dl {
        display: grid;
        gap: 0.5rem;
        margin: 0;
    }

    .device-review dl div {
        display: flex;
        justify-content: space-between;
        gap: 1rem;
        padding-bottom: 0.5rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .device-review dt {
        color: var(--color-text-muted);
        font-size: 0.8125rem;
    }

    .device-review dd {
        margin: 0;
        color: var(--color-text-primary);
        font-size: 0.8125rem;
        text-align: end;
        overflow-wrap: anywhere;
    }

    .device-review p {
        color: var(--color-text-secondary);
        font-size: 0.8125rem;
        line-height: 1.5;
    }

    .review-actions {
        display: grid;
        grid-template-columns: 1fr 1fr 1.4fr;
        gap: 0.5rem;
    }

    .btn-secondary {
        padding: 0.75rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 0.8125rem;
    }

    .deny {
        color: var(--color-error);
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
