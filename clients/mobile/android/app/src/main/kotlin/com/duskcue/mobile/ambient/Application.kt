// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import com.duskcue.mobile.AmbientPlaybackService
import com.duskcue.mobile.LegacyAmbientPlaybackService
import java.util.concurrent.atomic.AtomicLong

internal object AmbientApplication {
    private var instance: AmbientOwner? = null
    private val tickets = AtomicLong()
    private var pending: Pair<Long, Map<*, *>>? = null
    @Synchronized fun owner(context: Context): AmbientOwner = instance ?: AmbientOwner(context).also { instance = it }
    @Synchronized fun start(context: Context, args: Map<*, *>) {
        val owner = owner(context)
        AmbientIdentity(args)
        demand(pending == null && owner.scopeFence?.isDone != false && owner.commands.count() == 0 && owner.cleanup.count() == 0 && !owner.quarantined && !owner.http.failed &&
            !owner.journal.pending() && !owner.journal.failed, "ambient_cleanup_pending")
        val ticket = tickets.incrementAndGet()
        pending = ticket to args.toMap()
        try {
            ContextCompat.startForegroundService(context, Intent(context, AmbientPlaybackService::class.java)
                .setAction(AmbientPlaybackService.START).putExtra("ticket", ticket))
        } catch (error: Exception) { pending = null; throw error }
    }
    @Synchronized fun take(ticket: Long): Map<*, *>? {
        val captured = pending
        if (captured?.first != ticket) return null
        pending = null
        return captured.second
    }
    @Synchronized fun fence(context: Context, args: Map<*, *>?): java.util.concurrent.CompletableFuture<Unit> {
        pending = null
        val owner = owner(context)
        if (owner.legacyMode) LegacyAmbientPlaybackService.stop(context)
        return owner.bind(args)
    }
    @Synchronized fun cancelPending() { pending = null }
    @Synchronized fun hasPending() = pending != null
    @Synchronized fun pausePending() { pending = pending?.let { it.first to (it.second + mapOf("start_paused" to true)) } }
}
