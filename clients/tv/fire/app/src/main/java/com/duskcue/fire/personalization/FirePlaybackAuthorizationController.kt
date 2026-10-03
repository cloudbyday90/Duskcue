package com.duskcue.fire.personalization

import com.duskcue.tv.playback.TvInteractivePlayback
import com.duskcue.tv.playback.TvPlaybackObserver
import com.duskcue.tv.playback.TvPlaybackSample
import com.duskcue.tv.playback.TvPlaybackSignal
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class FirePlaybackAuthorizationController(
    private val reporter: FireWatchActivityReporter,
    private val source: FirePlaybackAuthorizationSource,
    private val scope: CoroutineScope,
    private val awaitRefresh: suspend () -> Unit = { delay(30_000) },
) : TvPlaybackObserver {
    private var generation = 0L
    private var sessionId: String? = null
    private var refreshJob: Job? = null

    suspend fun prepare(mediaItemId: String, playbackSessionId: String): Boolean {
        val expected = synchronized(this) {
            clear()
            generation
        }
        val authorization = authorize(mediaItemId, playbackSessionId)
        return synchronized(this) {
            if (generation != expected) false else {
                sessionId = playbackSessionId
                reporter.bind(playbackSessionId, authorization)
                true
            }
        }
    }

    @Synchronized
    override fun started(playback: TvInteractivePlayback) {
        if (sessionId != playback.sessionId) return
        reporter.started(playback)
        val expected = generation
        refreshJob = scope.launch {
            while (true) {
                awaitRefresh()
                val authorization = authorize(playback.mediaItemId, playback.sessionId)
                val keepRunning = synchronized(this@FirePlaybackAuthorizationController) {
                    generation == expected && sessionId == playback.sessionId &&
                        reporter.refresh(playback.sessionId, authorization)
                }
                if (!keepRunning) return@launch
            }
        }
    }

    @Synchronized
    override fun sample(sample: TvPlaybackSample, signal: TvPlaybackSignal) {
        reporter.sample(sample, signal)
    }

    @Synchronized
    override fun exited(sample: TvPlaybackSample?) {
        if (sample?.sessionId != sessionId || sessionId == null) return
        reporter.exited(sample)
        clear()
    }

    @Synchronized
    fun clearIfSession(playbackSessionId: String) {
        if (sessionId == playbackSessionId) clear()
    }

    @Synchronized
    override fun clear() {
        generation += 1
        sessionId = null
        refreshJob?.cancel()
        refreshJob = null
        reporter.clear()
    }

    private suspend fun authorize(mediaItemId: String, sessionId: String): FirePlaybackAuthorization? =
        try {
            source.authorize(mediaItemId, sessionId)
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (_: RuntimeException) {
            null
        }
}
