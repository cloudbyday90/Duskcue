/*
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 */

package com.duskcue.mobile

import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.ui.PlayerView
import org.json.JSONObject
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

class AmbientPlaybackService : MediaSessionService() {
    private data class Runtime(
        val serverOrigin: String,
        val bearerToken: String,
        val channelId: String,
        var channelName: String,
        var mediaItemId: String? = null,
        var sessionId: String? = null
    )

    private data class Selection(
        val channelName: String,
        val mediaItemId: String,
        val channelUpdatedAt: String
    )

    private data class StartResponse(
        val sessionId: String,
        val streamUrl: String
    )

    private class HttpFailure(val statusCode: Int, message: String) : Exception(message)

    private val mainHandler = Handler(Looper.getMainLooper())
    private val networkExecutor: ExecutorService = Executors.newSingleThreadExecutor()
    private var player: ExoPlayer? = null
    private var mediaSession: MediaSession? = null
    private var runtime: Runtime? = null
    private var lastError: String? = null
    private var advancing = false

    private val heartbeatRunnable = object : Runnable {
        override fun run() {
            val activeRuntime = runtime
            val activePlayer = player
            val sessionId = activeRuntime?.sessionId
            if (activeRuntime != null && activePlayer != null && sessionId != null) {
                val positionMs = activePlayer.currentPosition.coerceAtLeast(0)
                val isPaused = !activePlayer.isPlaying
                val isBuffering = activePlayer.isLoading
                networkExecutor.execute {
                    try {
                        postJson(
                            activeRuntime,
                            "/api/v1/playback/heartbeat",
                            JSONObject()
                                .put("session_id", sessionId)
                                .put("position_ms", positionMs)
                                .put("state", if (isPaused) "paused" else "playing")
                                .put("is_paused", isPaused)
                                .put("is_buffering", isBuffering)
                        )
                    } catch (_: Exception) {
                    }
                }
                mainHandler.postDelayed(this, HEARTBEAT_INTERVAL_MS)
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        activeService = this
        player = ExoPlayer.Builder(this).build().also { activePlayer ->
            activePlayer.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                    .build(),
                true
            )
            activePlayer.addListener(object : Player.Listener {
                override fun onPlaybackStateChanged(playbackState: Int) {
                    if (playbackState == Player.STATE_ENDED) {
                        advanceToNext()
                    }
                }

                override fun onPlayerError(error: androidx.media3.common.PlaybackException) {
                    lastError = error.message ?: "Ambient playback failed."
                    stopInternal(notifyServer = true, clearError = false)
                }
            })
        }
        mediaSession = MediaSession.Builder(this, requireNotNull(player)).build()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_START) {
            val serverOrigin = intent.getStringExtra(EXTRA_SERVER_ORIGIN)
            val bearerToken = intent.getStringExtra(EXTRA_BEARER_TOKEN)
            val channelId = intent.getStringExtra(EXTRA_CHANNEL_ID)
            val channelName = intent.getStringExtra(EXTRA_CHANNEL_NAME)
            if (serverOrigin.isNullOrBlank() || bearerToken.isNullOrBlank() || channelId.isNullOrBlank() || channelName.isNullOrBlank()) {
                lastError = "Ambient playback could not start."
                stopSelf(startId)
            } else {
                stopInternal(notifyServer = true)
                runtime = Runtime(serverOrigin, bearerToken, channelId, channelName)
                lastError = null
                advancing = false
                loadNext(null)
            }
        } else if (intent?.action == ACTION_STOP) {
            stopInternal(notifyServer = true)
            stopSelf(startId)
        }
        return Service.START_NOT_STICKY
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onDestroy() {
        stopInternal(notifyServer = true)
        mediaSession?.release()
        mediaSession = null
        player?.release()
        player = null
        activeService = null
        networkExecutor.shutdownNow()
        super.onDestroy()
    }

    private fun loadNext(afterMediaItemId: String?, staleRetries: Int = 0) {
        val activeRuntime = runtime ?: return
        networkExecutor.execute {
            try {
                val selection = requestNext(activeRuntime, afterMediaItemId)
                val start = requestPlaybackStart(activeRuntime, selection)
                mainHandler.post {
                    if (runtime !== activeRuntime) {
                        networkExecutor.execute {
                            try {
                                postJson(
                                    activeRuntime,
                                    "/api/v1/playback/stop",
                                    JSONObject().put("session_id", start.sessionId).put("position_ms", 0)
                                )
                            } catch (_: Exception) {
                            }
                        }
                        return@post
                    }
                    activeRuntime.channelName = selection.channelName
                    activeRuntime.mediaItemId = selection.mediaItemId
                    activeRuntime.sessionId = start.sessionId
                    playSelection(activeRuntime, start)
                }
            } catch (error: HttpFailure) {
                if (error.statusCode == 409 && staleRetries < MAX_STALE_RETRIES) {
                    mainHandler.post { loadNext(null, staleRetries + 1) }
                } else {
                    mainHandler.post {
                        lastError = error.message ?: "Ambient channel could not be resolved."
                        stopInternal(notifyServer = false, clearError = false)
                    }
                }
            } catch (error: Exception) {
                mainHandler.post {
                    lastError = error.message ?: "Ambient channel could not be resolved."
                    stopInternal(notifyServer = false, clearError = false)
                }
            }
        }
    }

    private fun playSelection(activeRuntime: Runtime, start: StartResponse) {
        val activePlayer = player ?: return
        val headers = mapOf("Authorization" to "Bearer ${activeRuntime.bearerToken}")
        val dataSource = DefaultDataSource.Factory(
            this,
            DefaultHttpDataSource.Factory().setDefaultRequestProperties(headers)
        )
        val mediaItem = MediaItem.Builder()
            .setUri(resolveUrl(activeRuntime.serverOrigin, start.streamUrl))
            .setMediaMetadata(MediaMetadata.Builder().setTitle(activeRuntime.channelName).build())
            .build()
        activePlayer.setMediaSource(DefaultMediaSourceFactory(dataSource).createMediaSource(mediaItem))
        activePlayer.prepare()
        activePlayer.play()
        advancing = false
        attachPlayerToView(activePlayer)
        mainHandler.removeCallbacks(heartbeatRunnable)
        mainHandler.postDelayed(heartbeatRunnable, HEARTBEAT_INTERVAL_MS)
    }

    private fun stopInternal(notifyServer: Boolean, clearError: Boolean = true) {
        mainHandler.removeCallbacks(heartbeatRunnable)
        advancing = false
        val activeRuntime = runtime
        val activePlayer = player
        val sessionId = activeRuntime?.sessionId
        val positionMs = activePlayer?.currentPosition?.coerceAtLeast(0) ?: 0
        runtime = null
        activePlayer?.stop()
        activePlayer?.clearMediaItems()
        detachPlayerFromView()
        if (clearError) lastError = null
        if (notifyServer && activeRuntime != null && sessionId != null) {
            networkExecutor.execute {
                try {
                    postJson(
                        activeRuntime,
                        "/api/v1/playback/stop",
                        JSONObject().put("session_id", sessionId).put("position_ms", positionMs)
                    )
                } catch (_: Exception) {
                }
            }
        }
    }

    private fun advanceToNext() {
        if (advancing) return
        val activeRuntime = runtime ?: return
        val completedSessionId = activeRuntime.sessionId
        val completedMediaItemId = activeRuntime.mediaItemId
        val completedPositionMs = player?.currentPosition?.coerceAtLeast(0) ?: 0
        advancing = true
        activeRuntime.sessionId = null
        mainHandler.removeCallbacks(heartbeatRunnable)
        networkExecutor.execute {
            if (completedSessionId != null) {
                try {
                    postJson(
                        activeRuntime,
                        "/api/v1/playback/stop",
                        JSONObject()
                            .put("session_id", completedSessionId)
                            .put("position_ms", completedPositionMs)
                    )
                } catch (_: Exception) {
                }
            }
            mainHandler.post {
                if (runtime === activeRuntime) {
                    loadNext(completedMediaItemId)
                }
            }
        }
    }

    private fun requestNext(activeRuntime: Runtime, afterMediaItemId: String?): Selection {
        val body = JSONObject().put("after_media_item_id", afterMediaItemId)
        val response = postJson(activeRuntime, "/api/v1/ambient-channels/${activeRuntime.channelId}/next", body)
        return Selection(
            response.getString("channel_name"),
            response.getString("media_item_id"),
            response.getString("channel_updated_at")
        )
    }

    private fun requestPlaybackStart(activeRuntime: Runtime, selection: Selection): StartResponse {
        val body = JSONObject()
            .put("media_item_id", selection.mediaItemId)
            .put("playback_mode", "ambient")
            .put("ambient_channel_id", activeRuntime.channelId)
            .put("ambient_channel_updated_at", selection.channelUpdatedAt)
            .put("device_profile", JSONObject()
                .put("client", "duskcue_mobile")
                .put("platform", "android_native_ambient")
                .put("video_codecs", org.json.JSONArray().put("h264"))
                .put("audio_codecs", org.json.JSONArray().put("aac").put("mp3").put("opus"))
                .put("subtitle_formats", org.json.JSONArray().put("webvtt").put("srt"))
                .put("max_resolution", "1080p")
                .put("hls_supported", true)
                .put("hdr_supported", false)
            )
        val response = postJson(activeRuntime, "/api/v1/playback/start", body)
        return StartResponse(response.getString("session_id"), response.getString("stream_url"))
    }

    private fun postJson(activeRuntime: Runtime, path: String, body: JSONObject): JSONObject {
        val connection = (URL(resolveUrl(activeRuntime.serverOrigin, path)).openConnection() as HttpURLConnection)
        connection.requestMethod = "POST"
        connection.connectTimeout = NETWORK_TIMEOUT_MS
        connection.readTimeout = NETWORK_TIMEOUT_MS
        connection.doOutput = true
        connection.setRequestProperty("Authorization", "Bearer ${activeRuntime.bearerToken}")
        connection.setRequestProperty("Content-Type", "application/json")
        OutputStreamWriter(connection.outputStream, Charsets.UTF_8).use { it.write(body.toString()) }
        val status = connection.responseCode
        val response = (if (status in 200..299) connection.inputStream else connection.errorStream)
            ?.bufferedReader(Charsets.UTF_8)
            ?.use { it.readText() }
            .orEmpty()
        connection.disconnect()
        if (status !in 200..299) {
            val detail = runCatching { JSONObject(response).optString("detail") }.getOrDefault("")
            throw HttpFailure(status, detail.ifBlank { "Ambient playback request failed ($status)." })
        }
        return JSONObject(response)
    }

    private fun resolveUrl(origin: String, value: String): String {
        val uri = Uri.parse(value)
        if (uri.isAbsolute) return value
        return origin.trimEnd('/') + "/" + value.trimStart('/')
    }

    private fun statusMap(): Map<String, Any?> {
        val activeRuntime = runtime
        val activePlayer = player
        return mapOf(
            "is_active" to (activeRuntime != null),
            "channel_id" to activeRuntime?.channelId,
            "channel_name" to activeRuntime?.channelName,
            "media_item_id" to activeRuntime?.mediaItemId,
            "position_ms" to (activePlayer?.currentPosition?.coerceAtLeast(0) ?: 0),
            "is_playing" to (activePlayer?.isPlaying == true),
            "error" to lastError
        )
    }

    private fun attachPlayerToView(activePlayer: ExoPlayer) {
        attachedPlayerView?.player = activePlayer
    }

    private fun detachPlayerFromView() {
        attachedPlayerView?.player = null
    }

    companion object {
        private const val ACTION_START = "com.duskcue.mobile.action.START_AMBIENT"
        private const val ACTION_STOP = "com.duskcue.mobile.action.STOP_AMBIENT"
        private const val EXTRA_SERVER_ORIGIN = "server_origin"
        private const val EXTRA_BEARER_TOKEN = "bearer_token"
        private const val EXTRA_CHANNEL_ID = "channel_id"
        private const val EXTRA_CHANNEL_NAME = "channel_name"
        private const val HEARTBEAT_INTERVAL_MS = 15_000L
        private const val NETWORK_TIMEOUT_MS = 30_000
        private const val MAX_STALE_RETRIES = 1

        @Volatile private var activeService: AmbientPlaybackService? = null
        @Volatile private var attachedPlayerView: PlayerView? = null

        fun start(context: Context, args: Map<*, *>) {
            val intent = Intent(context, AmbientPlaybackService::class.java).apply {
                action = ACTION_START
                putExtra(EXTRA_SERVER_ORIGIN, args[EXTRA_SERVER_ORIGIN] as? String)
                putExtra(EXTRA_BEARER_TOKEN, args[EXTRA_BEARER_TOKEN] as? String)
                putExtra(EXTRA_CHANNEL_ID, args[EXTRA_CHANNEL_ID] as? String)
                putExtra(EXTRA_CHANNEL_NAME, args[EXTRA_CHANNEL_NAME] as? String)
            }
            context.startService(intent)
        }

        fun stop(context: Context) {
            val service = activeService
            if (service != null) {
                service.mainHandler.post {
                    service.stopInternal(notifyServer = true)
                    service.stopSelf()
                }
            } else {
                context.stopService(Intent(context, AmbientPlaybackService::class.java))
            }
        }

        fun status(): Map<String, Any?> {
            return activeService?.statusMap() ?: mapOf(
                "is_active" to false,
                "position_ms" to 0,
                "is_playing" to false,
                "error" to null
            )
        }

        fun attach(view: PlayerView) {
            attachedPlayerView = view
            activeService?.player?.let { view.player = it }
        }

        fun detach(view: PlayerView) {
            if (attachedPlayerView === view) {
                view.player = null
                attachedPlayerView = null
            }
        }
    }
}
