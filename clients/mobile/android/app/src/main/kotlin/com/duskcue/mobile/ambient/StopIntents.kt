// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import com.duskcue.mobile.AmbientPlaybackService
import java.security.SecureRandom

internal object AmbientStopIntents {
    private val process = ByteArray(16).also { SecureRandom().nextBytes(it) }.joinToString("") { "%02x".format(it.toInt() and 255) }
    fun intent(context: Context, epoch: Long, ticket: Long, incarnation: String = process): Intent {
        demand(epoch >= 0 && ticket >= -1 && Regex("[a-f0-9]{32}").matches(incarnation), "invalid_stop")
        return Intent(context, AmbientPlaybackService::class.java).setAction(AmbientPlaybackService.STOP)
            .setData(Uri.parse("duskcue-ambient-stop:$incarnation/$epoch/$ticket"))
            .putExtra("ambient_epoch", epoch).putExtra("ambient_ticket", ticket).putExtra("ambient_incarnation", incarnation)
    }
    fun pending(context: Context, epoch: Long, ticket: Long, incarnation: String = process): PendingIntent = PendingIntent.getService(context, 0,
        intent(context, epoch, ticket, incarnation), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    fun matches(intent: Intent, epoch: Long, ticket: Long) = intent.action == AmbientPlaybackService.STOP &&
        intent.getLongExtra("ambient_epoch", Long.MIN_VALUE) == epoch && intent.getLongExtra("ambient_ticket", Long.MIN_VALUE) == ticket &&
        intent.getStringExtra("ambient_incarnation") == process && intent.data == Uri.parse("duskcue-ambient-stop:$process/$epoch/$ticket")
}
