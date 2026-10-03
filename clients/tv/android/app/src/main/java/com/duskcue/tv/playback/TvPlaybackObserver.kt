package com.duskcue.tv.playback

enum class TvPlaybackSignal { Loaded, StateChanged, Seek, Tick }

data class TvPlaybackSample(
    val sessionId: String,
    val positionMs: Long,
    val durationMs: Long,
    val loaded: Boolean,
    val playWhenReady: Boolean,
    val interstitial: Boolean = false,
)

interface TvPlaybackObserver {
    fun started(playback: TvInteractivePlayback)
    fun sample(sample: TvPlaybackSample, signal: TvPlaybackSignal)
    fun exited(sample: TvPlaybackSample?)
    fun clear()
}
