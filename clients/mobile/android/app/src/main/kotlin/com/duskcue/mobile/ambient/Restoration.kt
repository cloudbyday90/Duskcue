// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import java.util.concurrent.CompletableFuture

internal fun AmbientOwner.restore(force: Boolean = false): CompletableFuture<Unit> {
    synchronized(lock) {
        restoring?.let { if (!it.completion.isDone) return it.result }
        if (runtime != null || identity == null || !force && restoredEpoch == epochs.get()) return CompletableFuture.completedFuture(Unit)
        demand(commands.count() == 0 && cleanup.count() == 0 && !journal.pending(), "ambient_cleanup_pending")
        val captured = identity!!
        val epoch = epochs.get()
        val work = commands.run(publish = { outcome ->
            if (outcome.isFailure && epoch == epochs.get()) lastError = "ambient_reconciliation_required"
            emit()
        }) {
            val record = journal.read().result.get()
            demand(epoch == epochs.get() && captured.same(identity), "scope_changed")
            journalPresent = record != null
            if (record != null) {
                val rt = AmbientRuntime(epoch, captured, "", "")
                rt.protocol = AmbientProtocol(captured, http, clock) { current(rt, it) }
                val actual = rt.protocol.context(cleanup = true)
                demand(epoch == epochs.get() && captured.same(identity), "scope_changed")
                if (actual.matches(record.scope, cleanup = true)) {
                    rt.record = record
                    rt.dispatched = true
                    rt.cleanupOnly = true
                    rt.requiresPlay = true
                    rt.phase = "cleanup"
                    synchronized(lock) {
                        demand(epoch == epochs.get() && captured.same(identity), "scope_changed")
                        runtime = rt
                    }
                } else {
                    journal.clear().result.get()
                    journalPresent = false
                }
            }
            restoredEpoch = epoch
            Unit
        }
        restoring = work
        return work.result
    }
}
