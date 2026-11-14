// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.content.Context
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.Process
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.PlaybackException
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.hls.HlsMediaSource
import androidx.media3.exoplayer.upstream.DefaultLoadErrorHandlingPolicy
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy
import androidx.media3.session.MediaSession
import androidx.media3.ui.PlayerView
import java.util.concurrent.CompletableFuture
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

internal data class AmbientSample(val position: Long, val duration: Long, val playing: Boolean, val buffering: Boolean, val frames: Long)

internal fun <T> onMain(action: () -> T): CompletableFuture<T> {
    val result = CompletableFuture<T>()
    val work = Runnable { try { result.complete(action()) } catch (error: Throwable) { result.completeExceptionally(error) } }
    if (Looper.myLooper() == Looper.getMainLooper()) work.run() else {
        if (!Handler(Looper.getMainLooper()).post(work)) result.completeExceptionally(AmbientFailure("native_unavailable"))
    }
    return result
}

internal class AmbientOutput(
    private val context: Context,
    private val start: String,
    val ready: AmbientReady,
    private val identity: AmbientIdentity,
    private val scope: AmbientScope,
    private val title: String,
    private val timers: AmbientTimers,
    private val http: AmbientTransport,
    private val controls: AmbientControls,
    private val current: () -> Boolean,
    private val playIntent: () -> Boolean,
    private val ended: () -> Unit,
    private val failed: (Throwable) -> Unit,
) {
    private val alive = AtomicBoolean(true)
    private val releaseFailed = AtomicBoolean()
    private val frames = AtomicLong()
    private val loaders = AmbientLoaders()
    private val outputPrefix = commandPath(start, "/outputs/${ready.output}/hls/")
    private val source = AmbientMediaSource(identity, scope, outputPrefix, timers, http.cancellers) { alive.get() && current() }
    private var worker: HandlerThread? = null
    private var player: ExoPlayer? = null
    var facade: CommandPlayer? = null
        private set
    var session: MediaSession? = null
        private set
    private val prepared = CompletableFuture<Unit>()
    private val created = CompletableFuture<Unit>()
    private var retiring: CompletableFuture<Unit>? = null
    fun attach(): CompletableFuture<Unit> {
        onMain {
            try {
                demand(alive.get() && current(), "stale_output")
                val thread = HandlerThread("ambient-playback").also { worker = it; it.start() }
                val native = ExoPlayer.Builder(context).setPlaybackLooper(thread.looper)
                    .setLoadControl(DefaultLoadControl.Builder().setBufferDurationsMs(5000, 15000, 500, 1000)
                        .setTargetBufferBytes(8 * 1024 * 1024).setPrioritizeTimeOverSizeThresholds(false).build()).build()
                player = native
                native.setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(), true)
                native.setHandleAudioBecomingNoisy(true)
                val view = CommandPlayer(native, ready, title, controls).also { facade = it }
                session = MediaSession.Builder(context, view).setCallback(object : MediaSession.Callback {
                    override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
                        return if (controller.uid == Process.myUid() || controller.isTrusted)
                            MediaSession.ConnectionResult.AcceptedResultBuilder(session).build()
                        else MediaSession.ConnectionResult.reject()
                    }
                }).build()
                native.setVideoFrameMetadataListener { _, _, _, _ -> frames.incrementAndGet() }
                native.addListener(object : Player.Listener {
                    override fun onEvents(player: Player, events: Player.Events) { view.changed() }
                    override fun onPlaybackStateChanged(state: Int) {
                        if (state == Player.STATE_READY && !prepared.isDone) {
                            if (alive.get() && current()) { if (playIntent()) native.play(); prepared.complete(Unit) }
                            else prepared.completeExceptionally(AmbientFailure("stale_output"))
                        }
                        if (state == Player.STATE_ENDED && alive.get() && current()) ended()
                    }
                    override fun onPlayerError(error: PlaybackException) {
                        if (error.errorCode == PlaybackException.ERROR_CODE_TIMEOUT) releaseFailed.set(true)
                        prepared.completeExceptionally(AmbientFailure("native_playback_failed"))
                        if (alive.get() && current()) failed(error)
                    }
                    override fun onTracksChanged(tracks: androidx.media3.common.Tracks) {
                        for (group in tracks.groups) for (index in 0 until group.length) if (group.isTrackSelected(index)) {
                            val format = group.getTrackFormat(index)
                            val valid = when (group.type) {
                                C.TRACK_TYPE_VIDEO -> format.sampleMimeType == "video/avc" && format.height in 1..1080 && format.width in 1..1920 &&
                                    (format.colorInfo == null || format.colorInfo?.colorTransfer in setOf(C.COLOR_TRANSFER_SDR, Format.NO_VALUE))
                                C.TRACK_TYPE_AUDIO -> format.sampleMimeType == "audio/mp4a-latm" && format.channelCount in 1..2
                                else -> false
                            }
                            if (!valid) { prepared.completeExceptionally(AmbientFailure("unqualified_output")); if (current()) failed(AmbientFailure("unqualified_output")) }
                        }
                    }
                })
                val media = HlsMediaSource.Factory(source).setDownloadExecutor { loaders.create() }
                    .setLoadErrorHandlingPolicy(object : DefaultLoadErrorHandlingPolicy(0) {
                        override fun getRetryDelayMsFor(loadErrorInfo: LoadErrorHandlingPolicy.LoadErrorInfo): Long = C.TIME_UNSET
                        override fun getFallbackSelectionFor(options: LoadErrorHandlingPolicy.FallbackOptions, info: LoadErrorHandlingPolicy.LoadErrorInfo): LoadErrorHandlingPolicy.FallbackSelection? = null
                    }).createMediaSource(MediaItem.fromUri(identity.origin + outputPrefix + "manifest.m3u8"))
                native.setMediaSource(media)
                native.seekTo((ready.position - ready.offset).coerceAtLeast(0))
                native.prepare()
                created.complete(Unit)
            } catch (error: Throwable) { created.completeExceptionally(error); prepared.completeExceptionally(error) }
        }.whenComplete { _, error -> if (error != null) { created.completeExceptionally(error); prepared.completeExceptionally(error) } }
        return prepared
    }
    fun setPlaying(value: Boolean) = onMain {
        demand(alive.get() && current(), "stale_output")
        if (value) player?.play() else player?.pause()
        facade?.changed()
        Unit
    }
    fun sample() = onMain {
        val value = player ?: throw AmbientFailure("output_retired")
        AmbientSample((ready.offset + value.currentPosition.coerceAtLeast(0)).coerceAtMost(2147483647),
            value.duration, value.isPlaying, value.playbackState == Player.STATE_BUFFERING, frames.get())
    }
    fun attachView(view: PlayerView) = onMain { view.player = facade; Unit }
    fun detachView(view: PlayerView) = onMain { if (view.player === facade) view.player = null; Unit }
    fun backgroundPlaying(): Boolean {
        val native = player ?: return false
        return alive.get() && current() && playIntent() && native.playWhenReady &&
            native.playbackState in setOf(Player.STATE_READY, Player.STATE_BUFFERING)
    }
    @Synchronized fun retire(lane: NativeLane, publish: (Result<Unit>) -> Unit): CompletableFuture<Unit> {
        retiring?.let { return it }
        alive.set(false)
        source.cancel()
        prepared.completeExceptionally(AmbientFailure("output_retired"))
        val work = lane.run(timeoutMs = 30000, publish = publish) {
            try { created.get() } catch (_: Exception) { }
            onMain {
                try { session?.release(); facade?.release(); player?.release() }
                catch (_: Throwable) { releaseFailed.set(true) }
                finally { session = null; facade = null; player = null; worker?.quitSafely() }
            }.get()
            source.drain().get()
            loaders.close().get()
            worker?.join()
            demand(!releaseFailed.get(), "native_cleanup_failed")
            Unit
        }
        retiring = work.actual
        return retiring!!
    }
}
