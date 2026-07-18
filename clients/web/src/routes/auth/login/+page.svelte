<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { goto } from '$app/navigation';
    import { page } from '$app/stores';
    import { auth, authLoading, authError } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let mode = $state('invite');
    let inviteCode = $state('');
    let username = $state('');
    let password = $state('');
    let deviceName = $state('');

    let postLoginDestination = $derived.by(() => {
        const candidate = $page.url.searchParams.get('return_to');
        return candidate?.startsWith('/') && !candidate.startsWith('//') ? candidate : '/dashboard';
    });

    function switchMode(newMode) {
        mode = newMode;
        auth.clearError();
    }

    async function handleInviteLogin(e) {
        e.preventDefault();
        if (!inviteCode.trim()) {
            notifications.error(m.routes_auth_login_page_invite_code_is_required());
            return;
        }
        try {
            await auth.loginWithInvite({
                code: inviteCode.trim(),
                device_name: deviceName.trim() || 'Web Browser',
            });
            notifications.success(m.routes_auth_login_page_welcome_to_duskcue());
            goto(postLoginDestination);
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_auth_login_page_login_failed());
        }
    }

    async function handlePasswordLogin(e) {
        e.preventDefault();
        if (!username.trim() || !password) {
            notifications.error(m.routes_auth_login_page_username_and_password_are_required());
            return;
        }
        try {
            await auth.loginWithPassword({
                username: username.trim(),
                password,
                device_name: deviceName.trim() || 'Web Browser',
            });
            notifications.success(m.routes_auth_login_page_welcome_back());
            goto(postLoginDestination);
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_auth_login_page_login_failed());
        }
    }

    async function handlePasskeyLogin() {
        try {
            await auth.loginWithPasskey(async (options) => {
                if (!navigator.credentials) {
                    throw new Error('WebAuthn is not supported by this browser');
                }
                return await navigator.credentials.get({ publicKey: options });
            });
            notifications.success(m.routes_auth_login_page_welcome_back());
            goto(postLoginDestination);
        } catch (err) {
            if (err.name === 'NotAllowedError') return;
            notifications.error(err.detail || err.message || m.routes_auth_login_page_passkey_authentication_failed());
        }
    }
</script>

<div class="auth-page">
    <div class="auth-card">
        <div class="auth-header">
            <h1 class="auth-title">{m.routes_auth_login_page_sign_in()}</h1>
            <p class="auth-subtitle">{m.routes_auth_login_page_access_your_duskcue_media_server()}</p>
        </div>

        <div class="mode-tabs">
            <button
                class="mode-tab"
                class:active={mode === 'invite'}
                onclick={() => switchMode('invite')}
            >
                Invite Code
            </button>
            <button
                class="mode-tab"
                class:active={mode === 'password'}
                onclick={() => switchMode('password')}
            >
                Password
            </button>
        </div>

        {#if mode === 'invite'}
            <form onsubmit={handleInviteLogin} class="auth-form">
                <label class="field">
                    <span class="field-label">{m.routes_auth_login_page_invite_code()}</span>
                    <input
                        type="text"
                        bind:value={inviteCode}
                        placeholder={m.routes_auth_login_page_mv_invite_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx()}
                        autocomplete="off"
                        required
                    />
                </label>
                <label class="field">
                    <span class="field-label">{m.routes_auth_login_page_device_name()} <span class="field-optional">{m.routes_auth_login_page_optional()}</span></span>
                    <input
                        type="text"
                        bind:value={deviceName}
                        placeholder={m.routes_auth_login_page_web_browser()}
                    />
                </label>
                <button type="submit" class="btn-primary" disabled={$authLoading}>
                    {$authLoading ? 'Signing in…' : 'Sign In'}
                </button>
            </form>
        {:else}
            <form onsubmit={handlePasswordLogin} class="auth-form">
                <label class="field">
                    <span class="field-label">{m.routes_auth_login_page_username_52zi1f()}</span>
                    <input
                        type="text"
                        bind:value={username}
                        placeholder={m.routes_auth_login_page_username()}
                        autocomplete="username"
                        required
                    />
                </label>
                <label class="field">
                    <span class="field-label">{m.routes_auth_login_page_password()}</span>
                    <input
                        type="password"
                        bind:value={password}
                        placeholder="••••••••"
                        autocomplete="current-password"
                        required
                    />
                </label>
                <label class="field">
                    <span class="field-label">{m.routes_auth_login_page_device_name()} <span class="field-optional">{m.routes_auth_login_page_optional()}</span></span>
                    <input
                        type="text"
                        bind:value={deviceName}
                        placeholder={m.routes_auth_login_page_web_browser()}
                    />
                </label>
                <button type="submit" class="btn-primary" disabled={$authLoading}>
                    {$authLoading ? 'Signing in…' : 'Sign In'}
                </button>
            </form>
        {/if}

        <div class="divider">
            <span>{m.routes_auth_login_page_or()}</span>
        </div>

        <button class="btn-secondary" onclick={handlePasskeyLogin} disabled={$authLoading}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M2 12a5 5 0 0 1 5-5 5 5 0 0 1 5 5v3H8.5v3h-3v3H2z" />
                <circle cx="6.5" cy="9.5" r="1.5" fill="currentColor" stroke="none" />
            </svg>
            Sign in with Passkey
        </button>

        {#if $authError}
            <div class="auth-error">{$authError.detail || $authError.message}</div>
        {/if}
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
        margin-bottom: 1.5rem;
    }

    .auth-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-accent);
    }

    .auth-subtitle {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        margin-top: 0.25rem;
    }

    .mode-tabs {
        display: flex;
        gap: 0.25rem;
        background-color: var(--color-bg-elevated);
        border-radius: var(--radius-sm);
        padding: 3px;
        margin-bottom: 1.5rem;
    }

    .mode-tab {
        flex: 1;
        min-width: 0;
        padding: 0.5rem;
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
        white-space: nowrap;
    }

    .mode-tab.active {
        background-color: var(--color-bg-hover);
        color: var(--color-text-primary);
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

    .field-optional {
        color: var(--color-text-muted);
        font-weight: 400;
    }

    input {
        width: 100%;
        padding: 0.625rem 0.75rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 0.875rem;
        transition: border-color var(--transition-fast);
    }

    input:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    input::placeholder {
        color: var(--color-text-muted);
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

    .divider {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        margin: 1.5rem 0;
        color: var(--color-text-muted);
        font-size: 0.75rem;
    }

    .divider::before,
    .divider::after {
        content: '';
        flex: 1;
        height: 1px;
        background-color: var(--color-border);
    }

    .btn-secondary {
        width: 100%;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 0.5rem;
        padding: 0.75rem;
        background-color: var(--color-bg-elevated);
        color: var(--color-text-primary);
        font-size: 0.875rem;
        font-weight: 500;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .btn-secondary:hover:not(:disabled) {
        border-color: var(--color-accent);
        background-color: var(--color-bg-hover);
    }

    .btn-secondary:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .auth-error {
        margin-top: 1rem;
        padding: 0.625rem 0.75rem;
        background-color: var(--color-error-bg);
        color: var(--color-error);
        font-size: 0.8125rem;
        border-radius: var(--radius-sm);
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
