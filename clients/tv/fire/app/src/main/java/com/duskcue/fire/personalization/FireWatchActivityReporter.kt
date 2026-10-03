package com.duskcue.fire.personalization

import com.duskcue.tv.playback.TvInteractivePlayback
import com.duskcue.tv.playback.TvPlaybackObserver
import com.duskcue.tv.playback.TvPlaybackSample
import com.duskcue.tv.playback.TvPlaybackSignal

data class FirePlaybackAuthorization(
    val catalogContentId: String,
    val opaqueProfileKey: String,
    val expiresAtMs: Long,
    val integrationEnabled: Boolean,
    val customerOptedIn: Boolean,
    val profileType: String,
    val profileSelectionRequired: Boolean,
    val playbackMode: String,
    val accessRevalidated: Boolean,
) {
    fun eligible(nowMs: Long): Boolean = integrationEnabled && customerOptedIn &&
        profileType == "standard" && !profileSelectionRequired && playbackMode == "interactive" &&
        accessRevalidated && expiresAtMs > nowMs &&
        catalogContentId.isNotBlank() && catalogContentId.length <= 512 &&
        catalogContentId == catalogContentId.trim() &&
        catalogContentId.none { it.isISOControl() } &&
        !catalogContentId.startsWith("duskcue:", ignoreCase = true) &&
        opaqueProfileKey.matches(Regex("[A-Za-z0-9_-]{43}"))
}

enum class FirePlaybackState { PLAYING, PAUSED, INTERSTITIAL, EXIT }

data class FirePlaybackEvent(
    val catalogContentId: String,
    val opaqueProfileKey: String,
    val durationMs: Long,
    val positionMs: Long,
    val state: FirePlaybackState,
)

interface FirePlaybackSink {
    fun available(): Boolean
    fun sendActive(event: FirePlaybackEvent): Boolean
}

fun interface FirePlaybackAuthorizationSource {
    suspend fun authorize(mediaItemId: String, sessionId: String): FirePlaybackAuthorization?
}

class FireWatchActivityReporter(
    private val sink: FirePlaybackSink,
    private val featureEnabled: () -> Boolean,
    private val nowMs: () -> Long,
    private val elapsedMs: () -> Long,
) : TvPlaybackObserver {
    private var boundSessionId: String? = null
    private var activeSessionId: String? = null
    private var authorization: FirePlaybackAuthorization? = null
    private var lastState: FirePlaybackState? = null
    private var lastSentAtMs: Long? = null

    @Synchronized
    fun bind(sessionId: String, authorization: FirePlaybackAuthorization?) {
        clear()
        if (authorization != null && authorization.eligible(nowMs())) {
            boundSessionId = sessionId
            this.authorization = authorization
        }
    }

    @Synchronized
    fun refresh(sessionId: String, authorization: FirePlaybackAuthorization?): Boolean {
        val previous = this.authorization
        if (sessionId != boundSessionId || sessionId != activeSessionId || previous == null ||
            authorization == null || !authorization.eligible(nowMs()) ||
            authorization.catalogContentId != previous.catalogContentId ||
            authorization.opaqueProfileKey != previous.opaqueProfileKey
        ) {
            clear()
            return false
        }
        this.authorization = authorization
        return true
    }

    @Synchronized
    override fun started(playback: TvInteractivePlayback) {
        if (playback.sessionId != boundSessionId) {
            clear()
            return
        }
        activeSessionId = playback.sessionId
    }

    @Synchronized
    override fun sample(sample: TvPlaybackSample, signal: TvPlaybackSignal) {
        val state = when {
            sample.interstitial -> FirePlaybackState.INTERSTITIAL
            sample.playWhenReady -> FirePlaybackState.PLAYING
            else -> FirePlaybackState.PAUSED
        }
        val lastSent = lastSentAtMs
        val elapsed = elapsedMs()
        val due = lastSent == null || elapsed - lastSent >= 60_000
        if (signal == TvPlaybackSignal.Seek || lastState != state || due) {
            send(sample, state, elapsed)
        }
    }

    @Synchronized
    override fun exited(sample: TvPlaybackSample?) {
        val finalSample = sample ?: return
        if (finalSample.sessionId != activeSessionId || activeSessionId == null) return
        send(finalSample, FirePlaybackState.EXIT, elapsedMs())
        clear()
    }

    @Synchronized
    override fun clear() {
        boundSessionId = null
        activeSessionId = null
        authorization = null
        lastState = null
        lastSentAtMs = null
    }

    private fun send(sample: TvPlaybackSample, state: FirePlaybackState, elapsed: Long) {
        val authorization = authorization ?: return
        if (!authorization.eligible(nowMs())) {
            clear()
            return
        }
        if (sample.sessionId != activeSessionId || sample.sessionId != boundSessionId ||
            !sample.loaded || sample.durationMs <= 0 || sample.positionMs !in 0..sample.durationMs ||
            !featureEnabled()
        ) return
        try {
            if (!sink.available()) return
            if (sink.sendActive(FirePlaybackEvent(
                    catalogContentId = authorization.catalogContentId,
                    opaqueProfileKey = authorization.opaqueProfileKey,
                    durationMs = sample.durationMs,
                    positionMs = sample.positionMs,
                    state = state,
                ))
            ) {
                lastState = state
                lastSentAtMs = elapsed
            } else {
                clear()
            }
        } catch (_: RuntimeException) {
            clear()
        } catch (_: LinkageError) {
            clear()
        }
    }
}
