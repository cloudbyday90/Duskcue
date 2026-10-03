package com.duskcue.tv.playback

import com.duskcue.tv.ActiveTvSession
import com.duskcue.tv.TvDeepLink
import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.TvPlaybackStartResponse
import com.duskcue.tv.api.TvResolveResponse
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

sealed interface TvDeepLinkPlaybackResult {
    data class Started(val playback: TvPlaybackStartResponse) : TvDeepLinkPlaybackResult
    data object AuthenticationRequired : TvDeepLinkPlaybackResult
    data object ProfileRequired : TvDeepLinkPlaybackResult
    data object SessionExpired : TvDeepLinkPlaybackResult
    data object Unavailable : TvDeepLinkPlaybackResult
    data object Superseded : TvDeepLinkPlaybackResult
}

class TvAuthenticatedDeepLinkPlayer(
    private val currentSession: suspend () -> ActiveTvSession?,
    private val resolve: suspend (ActiveTvSession, String) -> ApiResult<TvResolveResponse>,
    private val create: suspend (ActiveTvSession, String) -> ApiResult<TvPlaybackStartResponse>,
    private val start: suspend (ActiveTvSession, TvResolveResponse, TvPlaybackStartResponse, () -> Boolean) -> Boolean,
    private val discard: suspend (ActiveTvSession, TvPlaybackStartResponse, Long) -> Unit,
) {
    private val generation = AtomicLong()

    fun invalidate() { generation.incrementAndGet() }

    suspend fun launch(link: TvDeepLink.Playback): TvDeepLinkPlaybackResult {
        val expected = generation.incrementAndGet()
        val session = currentSession() ?: return TvDeepLinkPlaybackResult.AuthenticationRequired
        if (session.profileSelectionRequired) return TvDeepLinkPlaybackResult.ProfileRequired
        val isCurrent = { generation.get() == expected }
        suspend fun scopeIsCurrent() = isCurrent() && currentSession() == session
        val resolution = resolve(session, link.platformContentId)
        if (!scopeIsCurrent()) return TvDeepLinkPlaybackResult.Superseded
        if (resolution is ApiResult.Failure && resolution.status == 401) return TvDeepLinkPlaybackResult.SessionExpired
        val resolved = (resolution as? ApiResult.Success)?.value
            ?: return TvDeepLinkPlaybackResult.Unavailable
        if (!resolved.access_revalidated || resolved.availability != "playable" ||
            resolved.playback_action != "start_playback" || resolved.resume_position_ms < 0 ||
            resolved.platform_content_id != link.platformContentId || resolved.media_item_id != link.mediaItemId ||
            resolved.media_type != link.mediaType
        ) return TvDeepLinkPlaybackResult.Unavailable
        val response = create(session, resolved.media_item_id)
        val playback = (response as? ApiResult.Success)?.value
        if (playback == null) {
            if (!scopeIsCurrent()) return TvDeepLinkPlaybackResult.Superseded
            if (response is ApiResult.Failure && response.status == 401) return TvDeepLinkPlaybackResult.SessionExpired
            return TvDeepLinkPlaybackResult.Unavailable
        }
        var started = false
        try {
            if (!scopeIsCurrent()) return TvDeepLinkPlaybackResult.Superseded
            if (playback.media_item_id != resolved.media_item_id || playback.playback_mode != "interactive") {
                return TvDeepLinkPlaybackResult.Unavailable
            }
            if (!start(session, resolved, playback, isCurrent)) {
                return if (scopeIsCurrent()) TvDeepLinkPlaybackResult.Unavailable else TvDeepLinkPlaybackResult.Superseded
            }
            started = true
            return TvDeepLinkPlaybackResult.Started(playback)
        } finally {
            if (!started) withContext(NonCancellable) { discard(session, playback, resolved.resume_position_ms) }
        }
    }
}
