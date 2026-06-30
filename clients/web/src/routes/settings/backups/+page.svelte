<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { getBackupStatus, checkWalGStatus, triggerPgDump, verifyBackups } from '$lib/api/backups.js';
    import { listScheduledTasks, triggerScheduledTask, listScheduledTaskRuns } from '$lib/api/settings.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let loading = $state(true);
    let canManage = $state(false);
    let loadError = $state(null);
    let status = $state(null);
    let scheduledTasks = $state([]);
    let recoveryDrillRuns = $state([]);
    let action = $state(null);

    $effect(() => {
        const unsub = hasCapability('can_manage_server').subscribe((value) => (canManage = value));
        return unsub;
    });

    let lastBackupRun = $derived(latestRun(['backup_database']));
    let lastVerificationRun = $derived(latestRun(['backup_verification']));
    let retentionRun = $derived(latestRun(['backup_retention_cleanup']));
    let recoveryDrillTask = $derived(
        scheduledTasks.find((task) => task.task_type === 'backup_recovery_drill' || task.task_type === 'recovery_drill'),
    );
    let lastRecoveryDrill = $derived(recoveryDrillRuns[0] || null);

    onMount(async () => {
        if (!canManage) {
            loading = false;
            return;
        }
        await load();
    });

    async function load() {
        loading = true;
        loadError = null;
        try {
            const [backupStatus, tasks] = await Promise.all([getBackupStatus(), listScheduledTasks()]);
            status = backupStatus;
            scheduledTasks = tasks.items || [];
            await loadRecoveryDrillRuns();
        } catch (err) {
            loadError = err.detail || err.message || m.routes_settings_backups_page_failed_to_load_backup_status();
        } finally {
            loading = false;
        }
    }

    async function loadRecoveryDrillRuns() {
        const task = scheduledTasks.find((item) => item.task_type === 'backup_recovery_drill' || item.task_type === 'recovery_drill');
        if (!task) {
            recoveryDrillRuns = [];
            return;
        }
        try {
            const response = await listScheduledTaskRuns(task.id, { page: 1, page_size: 5 });
            recoveryDrillRuns = response.items || [];
        } catch {
            recoveryDrillRuns = [];
        }
    }

    function latestRun(types) {
        const runs = status?.recent_runs || [];
        return runs.find((run) => types.includes(run.task_type)) || null;
    }

    async function runAction(name, fn, success) {
        action = name;
        try {
            await fn();
            notifications.success(success);
            await load();
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_backups_page_backup_operation_failed());
        } finally {
            action = null;
        }
    }

    function backupTask(type) {
        return status?.tasks?.find((task) => task.task_type === type);
    }

    async function triggerTask(type) {
        const task = backupTask(type) || scheduledTasks.find((item) => item.task_type === type);
        if (!task) {
            notifications.error(m.routes_settings_backups_page_scheduled_task_is_not_registered());
            return;
        }
        await runAction(type, () => triggerScheduledTask(task.id), 'Scheduled task triggered');
    }

    function formatDate(value) {
        if (!value) return 'Not run';
        return new Intl.DateTimeFormat(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
        }).format(new Date(value));
    }

    function formatDuration(ms) {
        if (!ms && ms !== 0) return '—';
        if (ms < 1000) return `${ms} ms`;
        const seconds = Math.round(ms / 1000);
        if (seconds < 60) return `${seconds}s`;
        return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    }

    function formatBytes(bytes) {
        if (!bytes && bytes !== 0) return '—';
        const units = ['B', 'KB', 'MB', 'GB', 'TB'];
        let value = Number(bytes);
        let index = 0;
        while (value >= 1024 && index < units.length - 1) {
            value /= 1024;
            index += 1;
        }
        return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
    }

    function resultClass(result) {
        if (result === 'success') return 'status-ok';
        if (result === 'failure' || result === 'cancelled') return 'status-bad';
        return 'status-warn';
    }
</script>

<div class="backup-settings">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">{m.routes_settings_backups_page_settings()}</a>
            <h1 class="page-title">{m.routes_settings_backups_page_backup_and_recovery()}</h1>
        </div>
        {#if !loading && canManage}
            <button class="btn-secondary" onclick={load} disabled={!!action}>{m.routes_settings_backups_page_refresh()}</button>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">{m.routes_settings_backups_page_you_do_not_have_permission_to_manage_backups()}</div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>{m.routes_settings_backups_page_retry()}</button>
        </div>
    {:else if status}
        <section class="summary-grid">
            <div class="summary-card">
                <span class="summary-label">{m.routes_settings_backups_page_readiness()}</span>
                <strong class={status.readiness.status === 'ready' ? 'status-ok' : 'status-warn'}>
                    {status.readiness.status}
                </strong>
                {#if status.readiness.issues?.length}
                    <ul class="issue-list">
                        {#each status.readiness.issues as issue}
                            <li>{issue}</li>
                        {/each}
                    </ul>
                {/if}
            </div>
            <div class="summary-card">
                <span class="summary-label">{m.routes_settings_backups_page_last_backup()}</span>
                <strong>{formatDate(lastBackupRun?.completed_at || lastBackupRun?.started_at)}</strong>
                <span class={resultClass(lastBackupRun?.result)}>{lastBackupRun?.result || 'not run'}</span>
            </div>
            <div class="summary-card">
                <span class="summary-label">{m.routes_settings_backups_page_last_verification()}</span>
                <strong>{formatDate(lastVerificationRun?.completed_at || lastVerificationRun?.started_at)}</strong>
                <span class={resultClass(lastVerificationRun?.result)}>{lastVerificationRun?.result || 'not run'}</span>
            </div>
            <div class="summary-card">
                <span class="summary-label">{m.routes_settings_backups_page_recovery_drill()}</span>
                <strong>{formatDate(lastRecoveryDrill?.completed_at || lastRecoveryDrill?.started_at)}</strong>
                <span class={resultClass(lastRecoveryDrill?.result)}>{lastRecoveryDrill?.result || 'not registered'}</span>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_backups_page_backup_configuration()}</h2>
                <a class="inline-link" href="/settings/system">{m.routes_settings_backups_page_edit_server_config_backup()}</a>
            </div>
            <div class="card-body">
                <div class="config-grid">
                    <div><span>{m.routes_settings_backups_page_wal_g()}</span><strong>{status.config.wal_g_enabled ? 'Enabled' : 'Disabled'}</strong></div>
                    <div><span>{m.routes_settings_backups_page_wal_g_storage()}</span><strong>{status.config.wal_g_storage_type}</strong></div>
                    <div><span>{m.routes_settings_backups_page_pg_dump()}</span><strong>{status.config.pg_dump_enabled ? 'Enabled' : 'Disabled'}</strong></div>
                    <div><span>{m.routes_settings_backups_page_verification()}</span><strong>{status.config.verification_enabled ? 'Enabled' : 'Disabled'}</strong></div>
                    <div><span>{m.routes_settings_backups_page_full_retention()}</span><strong>{status.config.wal_g_retention_full} full backups</strong></div>
                    <div><span>{m.routes_settings_backups_page_weekly_retention()}</span><strong>{status.config.wal_g_retention_weekly} weeks</strong></div>
                    <div><span>{m.routes_settings_backups_page_monthly_retention()}</span><strong>{status.config.wal_g_retention_monthly} months</strong></div>
                    <div><span>{m.routes_settings_backups_page_dump_retention()}</span><strong>{status.config.pg_dump_retention_daily} days / {status.config.pg_dump_retention_monthly} months</strong></div>
                </div>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_backups_page_operations()}</h2>
            </div>
            <div class="card-body action-grid">
                <button class="btn-secondary" onclick={() => runAction('wal-g-check', checkWalGStatus, 'WAL-G status check completed')} disabled={!!action}>
                    {action === 'wal-g-check' ? 'Checking…' : 'Check WAL-G'}
                </button>
                <button class="btn-secondary" onclick={() => runAction('pg-dump', () => triggerPgDump({ verify: true }), 'pg_dump backup completed')} disabled={!!action}>
                    {action === 'pg-dump' ? 'Running…' : 'Run pg_dump'}
                </button>
                <button class="btn-secondary" onclick={() => runAction('verify', () => verifyBackups({ verify_wal_g: true, verify_pg_dump: true }), 'Backup verification completed')} disabled={!!action}>
                    {action === 'verify' ? 'Verifying…' : 'Verify Backups'}
                </button>
                <button class="btn-secondary" onclick={() => triggerTask('backup_database')} disabled={!!action}>
                    {action === 'backup_database' ? 'Triggering…' : 'Trigger Scheduled Backup'}
                </button>
                <button class="btn-secondary" onclick={() => triggerTask('backup_verification')} disabled={!!action}>
                    {action === 'backup_verification' ? 'Triggering…' : 'Trigger Verification Task'}
                </button>
                <button class="btn-secondary" onclick={() => triggerTask('backup_retention_cleanup')} disabled={!!action}>
                    {action === 'backup_retention_cleanup' ? 'Triggering…' : 'Run Retention Cleanup'}
                </button>
                <button class="btn-secondary" onclick={() => triggerTask(recoveryDrillTask?.task_type)} disabled={!!action || !recoveryDrillTask}>
                    {action === recoveryDrillTask?.task_type ? 'Triggering…' : 'Trigger Recovery Drill'}
                </button>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_backups_page_scheduled_tasks()}</h2>
            </div>
            <div class="table-wrap">
                <table>
                    <thead>
                        <tr>
                            <th>{m.routes_settings_backups_page_task()}</th>
                            <th>{m.routes_settings_backups_page_state()}</th>
                            <th>{m.routes_settings_backups_page_last_run()}</th>
                            <th>{m.routes_settings_backups_page_result()}</th>
                            <th>{m.routes_settings_backups_page_next_run()}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each status.tasks as task}
                            <tr>
                                <td>
                                    <strong>{task.name}</strong>
                                    <span>{task.task_type}</span>
                                </td>
                                <td>{task.state}</td>
                                <td>{formatDate(task.last_run_at)}</td>
                                <td class={resultClass(task.last_run_result)}>{task.last_run_result || '—'}</td>
                                <td>{formatDate(task.next_run_at)}</td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_backups_page_recent_backup_evidence()}</h2>
            </div>
            <div class="table-wrap">
                <table>
                    <thead>
                        <tr>
                            <th>{m.routes_settings_backups_page_run()}</th>
                            <th>{m.routes_settings_backups_page_started()}</th>
                            <th>{m.routes_settings_backups_page_duration()}</th>
                            <th>{m.routes_settings_backups_page_result()}</th>
                            <th>{m.routes_settings_backups_page_evidence()}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each status.recent_runs as run}
                            <tr>
                                <td>
                                    <strong>{run.task_name}</strong>
                                    <span>{run.trigger_type}</span>
                                </td>
                                <td>{formatDate(run.started_at)}</td>
                                <td>{formatDuration(run.duration_ms)}</td>
                                <td class={resultClass(run.result)}>{run.result || run.state}</td>
                                <td>
                                    {#if run.stats?.pg_dump?.size_bytes}
                                        {formatBytes(run.stats.pg_dump.size_bytes)}
                                    {:else if run.stats?.commands}
                                        {run.stats.commands.length} commands
                                    {:else}
                                        recorded
                                    {/if}
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_backups_page_recovery_drill_evidence()}</h2>
                {#if !recoveryDrillTask}<span class="phase-badge">{m.routes_settings_backups_page_worker_pending()}</span>{/if}
            </div>
            <div class="card-body">
                {#if lastRecoveryDrill}
                    <div class="evidence-grid">
                        <div><span>{m.routes_settings_backups_page_started()}</span><strong>{formatDate(lastRecoveryDrill.started_at)}</strong></div>
                        <div><span>{m.routes_settings_backups_page_completed()}</span><strong>{formatDate(lastRecoveryDrill.completed_at)}</strong></div>
                        <div><span>{m.routes_settings_backups_page_duration()}</span><strong>{formatDuration(lastRecoveryDrill.duration_ms)}</strong></div>
                        <div><span>{m.routes_settings_backups_page_result()}</span><strong class={resultClass(lastRecoveryDrill.result)}>{lastRecoveryDrill.result || lastRecoveryDrill.state}</strong></div>
                    </div>
                    <pre>{JSON.stringify(lastRecoveryDrill.stats || {}, null, 2)}</pre>
                {:else}
                    <p class="section-note">
                        Recovery-drill evidence will appear here after the Phase 13a recovery drill worker registers and records restore evidence in scheduled task history.
                    </p>
                {/if}
            </div>
        </section>
    {/if}
</div>

<style>
    .backup-settings {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1120px;
    }

    .page-header,
    .card-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }

    .back-link,
    .inline-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .inline-link:hover,
    .back-link:hover {
        color: var(--color-text-secondary);
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
        margin-top: 0.25rem;
    }

    .summary-grid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .summary-card,
    .settings-card {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .summary-card {
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
        padding: 1rem;
        min-width: 0;
    }

    .summary-label {
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-muted);
    }

    .summary-card strong {
        font-size: 0.9375rem;
        color: var(--color-text-primary);
        word-break: break-word;
    }

    .settings-card {
        overflow: hidden;
    }

    .card-header {
        padding: 1rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .card-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .card-body {
        padding: 1.25rem;
    }

    .config-grid,
    .evidence-grid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .config-grid div,
    .evidence-grid div {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        padding: 0.75rem;
        background-color: var(--color-bg-deep);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .config-grid span,
    .evidence-grid span {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .config-grid strong,
    .evidence-grid strong {
        font-size: 0.8125rem;
        color: var(--color-text-primary);
    }

    .action-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
        gap: 0.75rem;
    }

    .btn-secondary {
        padding: 0.5rem 1rem;
        background-color: var(--color-bg-elevated);
        color: var(--color-text-secondary);
        font-size: 0.8125rem;
        font-weight: 600;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    .btn-secondary:hover:not(:disabled) {
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .btn-secondary:disabled {
        opacity: 0.5;
    }

    .table-wrap {
        overflow-x: auto;
    }

    table {
        width: 100%;
        border-collapse: collapse;
    }

    th,
    td {
        padding: 0.75rem 1rem;
        border-bottom: 1px solid var(--color-border-subtle);
        text-align: left;
        font-size: 0.8125rem;
        vertical-align: top;
    }

    th {
        color: var(--color-text-muted);
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
    }

    td strong,
    td span {
        display: block;
    }

    td span {
        margin-top: 0.125rem;
        color: var(--color-text-muted);
        font-size: 0.6875rem;
    }

    .status-ok {
        color: var(--color-success);
    }

    .status-warn {
        color: var(--color-warning);
    }

    .status-bad {
        color: var(--color-error);
    }

    .issue-list {
        margin: 0.25rem 0 0 1rem;
        color: var(--color-warning);
        font-size: 0.6875rem;
    }

    .section-note {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 0.8125rem;
    }

    .phase-badge {
        font-size: 0.625rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-warning);
        background-color: var(--color-warning-bg);
        padding: 0.1875rem 0.5rem;
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    pre {
        margin: 1rem 0 0;
        padding: 1rem;
        max-height: 320px;
        overflow: auto;
        background-color: var(--color-bg-deep);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        font-size: 0.75rem;
    }

    .empty-state,
    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: 240px;
        color: var(--color-text-muted);
        font-size: 0.875rem;
    }

    .empty-state {
        flex-direction: column;
        gap: 1rem;
    }

    .error-text {
        color: var(--color-error);
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

    @media (max-width: 980px) {
        .summary-grid,
        .config-grid,
        .evidence-grid {
            grid-template-columns: repeat(2, minmax(0, 1fr));
        }
    }

    @media (max-width: 640px) {
        .page-header,
        .card-header {
            flex-direction: column;
        }

        .summary-grid,
        .config-grid,
        .evidence-grid {
            grid-template-columns: 1fr;
        }
    }
</style>
