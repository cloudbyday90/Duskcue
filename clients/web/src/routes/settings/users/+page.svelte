<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { listUsers, deleteUser } from '$lib/api/users.js';
    import { listInvitations, createInvitation as apiCreateInvitation, revokeInvitation as apiRevokeInvitation } from '$lib/api/auth.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let loading = $state(true);
    let users = $state([]);
    let invitations = $state([]);
    let showInviteForm = $state(false);
    let inviteEmail = $state('');
    let inviteRole = $state('member');
    let inviteMaxUses = $state('1');
    let creatingInvite = $state(false);
    let canManage = $state(false);

    $effect(() => {
        const unsub = hasCapability('can_manage_users').subscribe((v) => (canManage = v));
        return unsub;
    });

    onMount(async () => {
        await loadData();
        loading = false;
    });

    async function loadData() {
        try {
            const [usersResp, invitesResp] = await Promise.all([
                listUsers(),
                listInvitations().catch(() => ({ items: [] })),
            ]);
            users = usersResp.items || usersResp || [];
            invitations = invitesResp.items || invitesResp || [];
        } catch (err) {
            notifications.error(err.detail || 'Failed to load users');
        }
    }

    async function handleCreateInvite() {
        creatingInvite = true;
        try {
            const data = { role: inviteRole, max_uses: parseInt(inviteMaxUses) };
            if (inviteEmail.trim()) data.email = inviteEmail.trim();
            const result = await apiCreateInvitation(data);
            notifications.success(`Invite created: ${result.code || result.code_prefix}`);
            invitations = [result, ...invitations];
            showInviteForm = false;
            inviteEmail = '';
        } catch (err) {
            notifications.error(err.detail || err.message || 'Failed to create invitation');
        } finally {
            creatingInvite = false;
        }
    }

    async function handleRevoke(invitationId) {
        try {
            await apiRevokeInvitation(invitationId);
            notifications.success('Invitation revoked');
            invitations = invitations.filter((i) => i.id !== invitationId);
        } catch (err) {
            notifications.error(err.detail || 'Failed to revoke invitation');
        }
    }

    async function handleDeleteUser(userId, displayName) {
        if (!confirm(`Delete user "${displayName}"? This action cannot be undone.`)) return;
        try {
            await deleteUser(userId);
            notifications.success('User deleted');
            users = users.filter((u) => u.id !== userId);
        } catch (err) {
            notifications.error(err.detail || 'Failed to delete user');
        }
    }
</script>

<div class="users-page">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">← Settings</a>
            <h1 class="page-title">User Management</h1>
        </div>
        {#if canManage}
            <button class="btn-primary" onclick={() => (showInviteForm = !showInviteForm)}>
                {showInviteForm ? 'Cancel' : 'Create Invite'}
            </button>
        {/if}
    </div>

    {#if showInviteForm}
        <div class="invite-form">
            <h3 class="form-title">Create Invitation</h3>
            <div class="form-row">
                <label class="field">
                    <span class="field-label">Email <span class="opt">(optional)</span></span>
                    <input type="email" bind:value={inviteEmail} placeholder="user@example.com" />
                </label>
                <label class="field">
                    <span class="field-label">Role</span>
                    <select bind:value={inviteRole}>
                        <option value="member">Member</option>
                        <option value="admin">Admin</option>
                    </select>
                </label>
                <label class="field">
                    <span class="field-label">Max Uses</span>
                    <input type="number" bind:value={inviteMaxUses} min="1" max="100" />
                </label>
            </div>
            <button class="btn-primary" onclick={handleCreateInvite} disabled={creatingInvite}>
                {creatingInvite ? 'Creating…' : 'Generate Invite Code'}
            </button>
        </div>
    {/if}

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
        </div>
    {:else}
        <section class="users-section">
            <h2 class="section-title">Users ({users.length})</h2>
            <div class="users-table">
                <div class="table-header">
                    <div class="col-name">Name</div>
                    <div class="col-username">Username</div>
                    <div class="col-role">Role</div>
                    <div class="col-status">Status</div>
                    <div class="col-actions"></div>
                </div>
                {#each users as user (user.id)}
                    <div class="table-row">
                        <div class="col-name">
                            <div class="user-avatar-sm">
                                {user.display_name?.[0]?.toUpperCase() || 'U'}
                            </div>
                            {user.display_name || user.username}
                        </div>
                        <div class="col-username">{user.username}</div>
                        <div class="col-role">
                            <span class="role-badge role-{user.role}">{user.role}</span>
                        </div>
                        <div class="col-status">
                            {#if user.is_active !== false}
                                <span class="status-active">Active</span>
                            {:else}
                                <span class="status-inactive">Disabled</span>
                            {/if}
                        </div>
                        <div class="col-actions">
                            {#if canManage && user.role !== 'owner'}
                                <button
                                    class="btn-danger-sm"
                                    onclick={() => handleDeleteUser(user.id, user.display_name || user.username)}
                                >
                                    Delete
                                </button>
                            {/if}
                        </div>
                    </div>
                {/each}
            </div>
        </section>

        {#if canManage}
            <section class="invitations-section">
                <h2 class="section-title">Pending Invitations ({invitations.length})</h2>
                {#if invitations.length > 0}
                    <div class="invitations-list">
                        {#each invitations as inv (inv.id)}
                            <div class="invitation-row">
                                <div class="inv-info">
                                    <span class="inv-prefix">{inv.code_prefix || '—'}</span>
                                    {#if inv.email}<span class="inv-email">{inv.email}</span>{/if}
                                </div>
                                <div class="inv-meta">
                                    <span class="inv-role role-badge role-{inv.role}">{inv.role}</span>
                                    <span class="inv-uses">{inv.use_count || 0}/{inv.max_uses || 1} used</span>
                                </div>
                                <button class="btn-danger-sm" onclick={() => handleRevoke(inv.id)}>
                                    Revoke
                                </button>
                            </div>
                        {/each}
                    </div>
                {:else}
                    <p class="empty-text">No pending invitations.</p>
                {/if}
            </section>
        {/if}
    {/if}
</div>

<style>
    .users-page {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1000px;
    }

    .page-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .back-link:hover {
        color: var(--color-text-secondary);
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
        margin-top: 0.25rem;
    }

    .invite-form {
        display: flex;
        flex-direction: column;
        gap: 1rem;
        padding: 1.5rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .form-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .form-row {
        display: flex;
        gap: 1rem;
        flex-wrap: wrap;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        flex: 1;
        min-width: 160px;
    }

    .field-label {
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .opt {
        color: var(--color-text-muted);
    }

    input, select {
        padding: 0.5rem 0.625rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 0.8125rem;
    }

    input:focus, select:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .btn-primary {
        align-self: flex-start;
        padding: 0.625rem 1.25rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.8125rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-primary:hover:not(:disabled) {
        background-color: var(--color-accent-hover);
    }

    .btn-primary:disabled {
        opacity: 0.5;
    }

    .section-title {
        font-size: 0.75rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-muted);
        margin-bottom: 0.75rem;
    }

    .users-table {
        display: flex;
        flex-direction: column;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .table-header,
    .table-row {
        display: grid;
        grid-template-columns: 2fr 1.5fr 1fr 1fr auto;
        align-items: center;
        gap: 0.75rem;
        padding: 0.625rem 1rem;
    }

    .table-header {
        background-color: var(--color-bg-elevated);
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-muted);
    }

    .table-row {
        border-top: 1px solid var(--color-border-subtle);
        font-size: 0.8125rem;
    }

    .col-name {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .col-username {
        color: var(--color-text-muted);
        font-family: monospace;
        font-size: 0.75rem;
    }

    .user-avatar-sm {
        width: 24px;
        height: 24px;
        border-radius: 50%;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.625rem;
        font-weight: 700;
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
    }

    .role-badge {
        display: inline-block;
        font-size: 0.625rem;
        font-weight: 600;
        text-transform: uppercase;
        padding: 0.125rem 0.5rem;
        border-radius: var(--radius-sm);
    }

    .role-owner {
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
    }

    .role-admin {
        color: var(--color-warning);
        background-color: var(--color-warning-bg);
    }

    .role-member {
        color: var(--color-text-secondary);
        background-color: var(--color-info-bg);
    }

    .status-active {
        color: var(--color-success);
        font-size: 0.75rem;
    }

    .status-inactive {
        color: var(--color-error);
        font-size: 0.75rem;
    }

    .btn-danger-sm {
        padding: 0.25rem 0.625rem;
        font-size: 0.6875rem;
        color: var(--color-error);
        background-color: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: all var(--transition-fast);
    }

    .btn-danger-sm:hover {
        background-color: var(--color-error-bg);
        border-color: var(--color-error);
    }

    .invitations-list {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .invitation-row {
        display: flex;
        align-items: center;
        gap: 1rem;
        padding: 0.625rem 1rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .inv-info {
        flex: 1;
        display: flex;
        gap: 0.75rem;
        align-items: center;
    }

    .inv-prefix {
        font-family: monospace;
        font-size: 0.8125rem;
        color: var(--color-accent);
    }

    .inv-email {
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .inv-meta {
        display: flex;
        gap: 0.75rem;
        align-items: center;
    }

    .inv-uses {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .empty-text {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 4rem 0;
    }

    .loading-spinner {
        width: 32px;
        height: 32px;
        border: 3px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }
</style>
