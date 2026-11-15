// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import java.util.concurrent.CompletableFuture

internal class AmbientNotificationProbe(private val context: Context) {
    private data class Captured(val session: String, val action: PendingIntent, val dismissal: PendingIntent)
    private val captured = ArrayList<Captured>(2)
    private val handler = Handler(Looper.getMainLooper())
    private var pending: CompletableFuture<Unit>? = null

    fun capture(start: String, output: String): Int? {
        check(Looper.myLooper() == Looper.getMainLooper() && Build.VERSION.SDK_INT >= 33)
        val owner = AmbientApplication.owner(context)
        synchronized(owner.lock) {
            val status = owner.status()
            check(status["request_id"] == start && status["output_revision"] == output)
            val session = owner.mediaSession() ?: return null
            captured.indexOfFirst { it.session == session.id }.takeIf { it >= 0 }?.let { return it }
            check(captured.size < 2 && pending == null)
            val notification = context.getSystemService(NotificationManager::class.java).activeNotifications
                .singleOrNull { it.packageName == context.packageName && it.id == 48029 && it.notification.channelId == "duskcue_ambient_commands" }
                ?.notification ?: return null
            val token = notification.extras.getParcelable(Notification.EXTRA_MEDIA_SESSION, android.media.session.MediaSession.Token::class.java)
            if (token != session.platformToken) return null
            val action = notification.actions?.singleOrNull()?.actionIntent ?: return null
            val dismissal = notification.deleteIntent ?: return null
            check(action.creatorPackage == context.packageName && dismissal.creatorPackage == context.packageName)
            for (old in captured) check(old.action != action && old.dismissal != dismissal)
            captured.add(Captured(session.id, action, dismissal))
            return captured.lastIndex
        }
    }

    fun send(index: Int, dismiss: Boolean): CompletableFuture<Unit> {
        check(Looper.myLooper() == Looper.getMainLooper() && pending == null && index in captured.indices)
        val actual = CompletableFuture<Unit>()
        val result = CompletableFuture<Unit>()
        pending = actual
        val timeout = Runnable { result.completeExceptionally(AmbientFailure("notification_dispatch_uncertain")) }
        check(handler.postDelayed(timeout, 5000))
        actual.whenComplete { _, error ->
            handler.removeCallbacks(timeout)
            pending = null
            if (error == null) result.complete(Unit) else result.completeExceptionally(error)
        }
        try {
            val token = if (dismiss) captured[index].dismissal else captured[index].action
            token.send(context, 0, null, PendingIntent.OnFinished { _, _, _, _, _ -> actual.complete(Unit) }, handler)
        } catch (_: PendingIntent.CanceledException) {
            actual.completeExceptionally(AmbientFailure("notification_cancelled"))
        }
        return result
    }

    fun clear() {
        check(Looper.myLooper() == Looper.getMainLooper() && pending == null)
        for (entry in captured) { entry.action.cancel(); entry.dismissal.cancel() }
        captured.clear()
    }
}
