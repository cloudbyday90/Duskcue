<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { goto } from '$app/navigation';
    import { auth, authLoading, authError } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let username = $state('');
    let password = $state('');
    let displayName = $state('');
    let serverName = $state('');

    async function handleSubmit(e) {
        e.preventDefault();
        if (!username.trim() || !password) {
            notifications.error('Username and password are required');
            return;
        }
        try {
            await auth.setup({
                username: username.trim(),
                password,
                display_name: displayName.trim() || username.trim(),
                server_name: serverName.trim() || null,
            });
            notifications.success('Server configured successfully');
            goto('/dashboard');
        } catch (err) {
            notifications.error(err.detail || err.message || 'Setup failed');
        }
    }
</script>

<div class="auth-page">
    <div class="auth-card">
        <div class="auth-header">
            <h1 class="auth-title">Welcome to Duskcue</h1>
            <p class="auth-subtitle">
                Create your owner account to get started. This will be the primary administrator
                for your media server.
            </p>
        </div>

        <form onsubmit={handleSubmit} class="auth-form">
            <label class="field">
                <span class="field-label">Username</span>
                <input
                    type="text"
                    bind:value={username}
                    placeholder="admin"
                    autocomplete="username"
                    required
                />
            </label>

            <label class="field">
                <span class="field-label">Password</span>
                <input
                    type="password"
                    bind:value={password}
                    placeholder="••••••••"
                    autocomplete="new-password"
                    required
                />
            </label>

            <label class="field">
                <span class="field-label">Display Name <span class="field-optional">(optional)</span></span>
                <input
                    type="text"
                    bind:value={displayName}
                    placeholder="Your Name"
                    autocomplete="name"
                />
            </label>

            <label class="field">
                <span class="field-label">Server Name <span class="field-optional">(optional)</span></span>
                <input
                    type="text"
                    bind:value={serverName}
                    placeholder="My Media Server"
                />
            </label>

            {#if $authError}
                <div class="auth-error">{$authError.detail || $authError.message}</div>
            {/if}

            <button type="submit" class="btn-primary" disabled={$authLoading}>
                {$authLoading ? 'Setting up…' : 'Complete Setup'}
            </button>
        </form>
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
        width: 100%;
        max-width: 420px;
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
        margin-bottom: 0.5rem;
    }

    .auth-subtitle {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        line-height: 1.5;
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

    .auth-error {
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
