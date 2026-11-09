// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.os.Looper
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.TextureView
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.exoplayer.ExoPlayer
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import java.util.concurrent.CompletableFuture

internal interface AmbientControls {
    fun playing(value: Boolean): CompletableFuture<Unit>
    fun stop(): CompletableFuture<Unit>
    fun seek(position: Long): CompletableFuture<Unit>
    fun available(): Boolean
    fun seekable(): Boolean
}

internal class CommandPlayer(
    private val player: ExoPlayer,
    private val ready: AmbientReady,
    private val title: String,
    private val controls: AmbientControls,
) : SimpleBasePlayer(Looper.getMainLooper()) {
    fun changed() { invalidateState() }
    override fun getState(): State {
        val available = controls.available()
        val commands = Player.Commands.Builder().addAll(Player.COMMAND_GET_CURRENT_MEDIA_ITEM, Player.COMMAND_GET_TIMELINE,
            Player.COMMAND_GET_METADATA, Player.COMMAND_GET_VOLUME, Player.COMMAND_SET_VIDEO_SURFACE,
            Player.COMMAND_STOP).apply {
            if (available) add(Player.COMMAND_PLAY_PAUSE)
            if (available && controls.seekable()) add(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM)
        }.build()
        val item = MediaItem.Builder().setMediaId(ready.output)
            .setMediaMetadata(MediaMetadata.Builder().setTitle(title).build()).build()
        val duration = if (player.duration >= 0) (ready.offset + player.duration) * 1000 else C.TIME_UNSET
        val metadata = MediaItemData.Builder(ready.output).setMediaItem(item).setDurationUs(duration)
            .setIsSeekable(available && controls.seekable() && player.isCurrentMediaItemSeekable).setIsDynamic(player.isCurrentMediaItemDynamic).build()
        return State.Builder().setAvailableCommands(commands).setPlaylist(listOf(metadata)).setCurrentMediaItemIndex(0)
            .setPlaybackState(if (available) player.playbackState else Player.STATE_IDLE)
            .setPlayWhenReady(available && player.playWhenReady, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setPlaybackSuppressionReason(player.playbackSuppressionReason)
            .setIsLoading(available && player.playbackState in setOf(Player.STATE_READY, Player.STATE_BUFFERING) && player.isLoading)
            .setContentPositionMs((ready.offset + player.currentPosition.coerceAtLeast(0)).coerceAtMost(2147483647))
            .setContentBufferedPositionMs(PositionSupplier.getConstant(ready.offset + player.bufferedPosition.coerceAtLeast(0)))
            .setVideoSize(player.videoSize).setVolume(player.volume).build()
    }
    override fun getPlaceholderState(suggestedPlaceholderState: State) = getState()
    private fun invoke(action: () -> CompletableFuture<Unit>): ListenableFuture<Unit> {
        val result = SettableFuture.create<Unit>()
        try { action().whenComplete { _, error -> if (error == null) result.set(Unit) else result.setException(AmbientFailure("control_unavailable")) } }
        catch (_: Exception) { result.setException(AmbientFailure("control_unavailable")) }
        return result
    }
    override fun handleSetPlayWhenReady(playWhenReady: Boolean) = invoke { controls.playing(playWhenReady) }
    override fun handleStop() = invoke { controls.stop() }
    override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<Unit> = invoke {
        demand(mediaItemIndex == 0 && positionMs in 0..2147483647)
        controls.seek(positionMs)
    }
    override fun handleRelease(): ListenableFuture<Unit> = Futures.immediateFuture(Unit)
    override fun handleSetVideoOutput(videoOutput: Any): ListenableFuture<Unit> {
        when (videoOutput) {
            is Surface -> player.setVideoSurface(videoOutput)
            is SurfaceHolder -> player.setVideoSurfaceHolder(videoOutput)
            is SurfaceView -> player.setVideoSurfaceView(videoOutput)
            is TextureView -> player.setVideoTextureView(videoOutput)
            else -> throw AmbientFailure("unsupported_surface")
        }
        return Futures.immediateFuture(Unit)
    }
    override fun handleClearVideoOutput(videoOutput: Any?): ListenableFuture<Unit> {
        player.clearVideoSurface()
        return Futures.immediateFuture(Unit)
    }
}
