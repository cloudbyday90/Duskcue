// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "support/retention_cases.rs"]
mod cases;
#[path = "support/admission_commit_proxy.rs"]
mod commit_proxy;
#[path = "support/retention_database.rs"]
mod database;
#[path = "support/playback_admission_fixture.rs"]
mod fixture;
#[path = "support/retention_ownership.rs"]
mod ownership;
#[path = "../src/services/playback_admission/retention/queries.rs"]
mod queries;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an owned disposable PostgreSQL18 fixture"]
async fn bounded_receipts_preserve_replay_accounting_and_owned_cleanup() -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(600), async {
        database::run(cases::migration_and_pruning).await?;
        println!("RETENTION_CASE migration_pruning passed");
        database::run(cases::time_and_guards).await?;
        println!("RETENTION_CASE time_guards passed");
        database::run(cases::quotas).await?;
        println!("RETENTION_CASE quotas passed");
        database::run(cases::quota_races).await?;
        println!("RETENTION_CASE quota_races passed");
        database::run(cases::household_quota).await?;
        println!("RETENTION_CASE household_quota passed");
        database::run(cases::household_cleanup_backlog).await?;
        println!("RETENTION_CASE household_cleanup_backlog passed");
        database::run(ownership::faults).await?;
        println!("RETENTION_CASE ownership_faults passed");
        for before in [true, false] {
            database::run(async |db| ownership::prune_commit_fault(db, before).await).await?;
        }
        println!("RETENTION_CASE prune_commit_faults passed");
        database::run(ownership::old_viewer).await?;
        println!("RETENTION_CASE old_viewer passed");
        Ok::<_, anyhow::Error>(())
    })
    .await?
}
