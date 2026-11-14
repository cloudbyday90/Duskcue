// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.DefaultMediaNotificationProvider
import com.duskcue.mobile.ambient.AmbientApplication
import com.duskcue.mobile.ambient.AmbientHost
import com.duskcue.mobile.ambient.AmbientOwner
import com.duskcue.mobile.ambient.AmbientStopIntents

class AmbientPlaybackService : MediaSessionService(), AmbientHost {
    private lateinit var owner: AmbientOwner
    private var registered: MediaSession? = null
    private var stopAction: PendingIntent? = null
    private var latestStartId = 0
    override fun onCreate() {
        super.onCreate()
        owner = AmbientApplication.owner(applicationContext)
        owner.host = this
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Ambient playback", NotificationManager.IMPORTANCE_LOW))
        setMediaNotificationProvider(DefaultMediaNotificationProvider.Builder(this).setNotificationId(NOTIFICATION).setChannelId(CHANNEL).build())
        changed()
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        latestStartId = startId
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            START -> {
                val args = AmbientApplication.take(intent.getLongExtra("ticket", -1))
                if (args != null) try { owner.start(args) } catch (_: Exception) {
                    owner.lastError = "ambient_start_refused"
                    if (owner.runtime == null) stopped() else changed()
                }
                else if (owner.runtime == null && !AmbientApplication.hasPending()) stopped()
            }
            STOP -> synchronized(owner.lock) {
                if (AmbientStopIntents.matches(intent, owner.epochs.get(), AmbientApplication.pendingTicket())) {
                    AmbientApplication.cancelPending()
                    owner.stop()
                } else stopped()
            }
        }
        return START_NOT_STICKY
    }
    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? =
        if (controllerInfo.uid == android.os.Process.myUid() || controllerInfo.isTrusted) owner.mediaSession() else null
    override fun changed() {
        if (owner.legacyMode) return
        val session = owner.mediaSession()
        if (registered !== session) {
            registered?.let { if (sessions.contains(it)) removeSession(it) }
            registered = session
            session?.let { addSession(it) }
        }
        if (session != null) {
            stopAction?.cancel()
            stopAction = null
            triggerNotificationUpdate()
            return
        }
        val pending = AmbientStopIntents.pending(this, owner.epochs.get(), AmbientApplication.pendingTicket())
        if (stopAction != pending) stopAction?.cancel()
        stopAction = pending
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        val open = launch?.let { PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }
        val state = owner.status()["state"] as? String ?: "checking"
        val notification = Notification.Builder(this, CHANNEL).setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle("Duskcue ambient playback").setContentText(if (state == "cleanup") "Playback cleanup needs attention" else "Ambient playback: $state")
            .setContentIntent(open).setOngoing(true).addAction(Notification.Action.Builder(null, "Stop", pending).build()).build()
        startForeground(NOTIFICATION, notification)
        if (state in setOf("uncertain", "cleanup", "unavailable", "settled")) stopForeground(STOP_FOREGROUND_DETACH)
    }
    override fun stopped() {
        if (owner.runtime != null || AmbientApplication.hasPending()) return
        stopAction?.cancel()
        stopAction = null
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelfResult(latestStartId)
    }
    override fun legacy(args: Map<*, *>) { LegacyAmbientPlaybackService.start(applicationContext, args); stopped() }
    override fun onTaskRemoved(rootIntent: Intent?) {
        if (isPlaybackOngoing && owner.backgroundPlaying()) return
        owner.stop()
        super.onTaskRemoved(rootIntent)
    }
    override fun onDestroy() {
        stopAction?.cancel()
        stopAction = null
        if (owner.host === this) {
            owner.host = null
            if (!owner.legacyMode) owner.stop()
        }
        super.onDestroy()
    }
    companion object {
        const val START = "com.duskcue.mobile.ambient.START"
        const val STOP = "com.duskcue.mobile.ambient.STOP"
        private const val CHANNEL = "duskcue_ambient_commands"
        private const val NOTIFICATION = 48029
    }
}
