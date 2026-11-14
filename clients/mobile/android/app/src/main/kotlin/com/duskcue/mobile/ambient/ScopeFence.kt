// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import java.util.concurrent.CompletableFuture

internal fun AmbientOwner.fenceScope() {
    synchronized(lock) {
        if (scopeFence?.isDone == false) return
        val completion = CompletableFuture<Unit>()
        scopeFence = completion
        fun reconcile() {
            try {
                val work = cleanup.run(publish = { outcome -> if (outcome.isFailure) quarantined = true }) {
                    val epoch = epochs.get()
                    retireTrackedOutputs()
                    demand(!journal.failed, "journal_unavailable")
                    val rt = runtime
                    if (rt != null) {
                        if (rt.identity.cleanupSame(identity)) rt.phase = "cleanup"
                        else {
                            journal.clear().actual.get()
                            journalPresent = false
                            rt.record = null
                            synchronized(lock) { if (runtime === rt) runtime = null }
                        }
                    }
                    emit()
                    epoch
                }
                work.actual.whenComplete { epoch, failure ->
                    synchronized(lock) {
                        if (failure != null) { quarantined = true; completion.completeExceptionally(failure) }
                        else if (epoch != epochs.get()) reconcile()
                        else completion.complete(Unit)
                    }
                    emit()
                }
            } catch (failure: Throwable) {
                quarantined = true
                completion.completeExceptionally(failure)
                emit()
            }
        }
        CompletableFuture.allOf(commands.drain(), checkpoints.drain(), sampling.drain(), renewal.drain(), http.drain(), journal.drain(), cleanup.drain())
            .whenComplete { _, error ->
                if (error != null) {
                    quarantined = true
                    completion.completeExceptionally(error)
                    emit()
                } else reconcile()
            }
    }
}
