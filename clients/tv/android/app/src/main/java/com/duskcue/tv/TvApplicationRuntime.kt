package com.duskcue.tv

import android.content.Context
import com.duskcue.tv.api.DuskcueApiClient
import com.duskcue.tv.api.MemoryEtagStore
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.RetryingTransport
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.TvSurface
import com.duskcue.tv.api.UrlConnectionTransport
import com.duskcue.tv.home.TvHomeLoadState
import com.duskcue.tv.home.TvLivingRoomStore
import com.duskcue.tv.home.TvProfileScope
import com.duskcue.tv.playback.TvInteractivePlayback
import com.duskcue.tv.playback.TvPlaybackService
import com.duskcue.tv.session.SecureSessionStore
import com.duskcue.tv.session.TvAuthenticationService
import com.duskcue.tv.session.TvLocalStateCleaner
import com.duskcue.tv.session.TvSessionCoordinator
import com.duskcue.tv.watchnext.AndroidWatchNextProvider
import com.duskcue.tv.watchnext.WatchNextPublisher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class ActiveTvSession(
    val origin: ServerOrigin,
    val userId: String,
    val profileId: String,
    val profileSelectionRequired: Boolean,
)

class TvApplicationRuntime(context: Context) {
    private val applicationContext = context.applicationContext
    private val tokenProvider = MutableBearerTokenProvider()
    private val etags = MemoryEtagStore()
    private val sessionStore = SecureSessionStore(context)
    private val runtimeScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val watchNext = WatchNextPublisher(
        provider = AndroidWatchNextProvider(applicationContext.contentResolver),
        store = sessionStore,
    )
    val livingRoom = TvLivingRoomStore(etags = etags)
    private val localStateCleaner = object : TvLocalStateCleaner {
        override suspend fun clearProfileScope() {
            TvPlaybackService.stop(applicationContext)
            withContext(Dispatchers.IO) { watchNext.clear() }
            livingRoom.clearProfileScope()
        }

        override suspend fun clearIdentityScope() {
            TvPlaybackService.stop(applicationContext)
            withContext(Dispatchers.IO) { watchNext.clear() }
            livingRoom.clearIdentityScope()
        }
    }
    private val coordinator = TvSessionCoordinator(
        store = sessionStore,
        tokenProvider = tokenProvider,
        cleaner = localStateCleaner,
    )
    val authentication = TvAuthenticationService(
        store = sessionStore,
        coordinator = coordinator,
        tokenProvider = tokenProvider,
        apiFor = { origin, _ -> client(origin) },
    )

    fun client(origin: ServerOrigin): DuskcueApiClient = DuskcueApiClient(
        origin = origin,
        transport = RetryingTransport(UrlConnectionTransport()),
        tokenProvider = tokenProvider,
        etagStore = etags,
    )

    suspend fun activeSession(): ActiveTvSession? {
        val session = sessionStore.current().session ?: return null
        val origin = ServerOrigin.parse(session.origin).getOrNull() ?: return null
        return ActiveTvSession(
            origin = origin,
            userId = session.user_id,
            profileId = session.active_profile_id,
            profileSelectionRequired = session.profile_selection_required,
        )
    }

    suspend fun activeProfileScope(): TvProfileScope? = activeSession()
        ?.takeUnless(ActiveTvSession::profileSelectionRequired)
        ?.let { TvProfileScope(it.origin.value, it.userId, it.profileId) }

    suspend fun syncWatchNext(scope: TvProfileScope, surface: TvSurface) {
        withContext(Dispatchers.IO) {
            try {
                watchNext.sync(scope, surface)
            } catch (_: Exception) {
                Unit
            }
        }
    }

    fun refreshWatchNext() {
        runtimeScope.launch {
            val scope = activeProfileScope() ?: return@launch
            val origin = ServerOrigin.parse(scope.origin).getOrNull() ?: return@launch
            val home = livingRoom.load(client(origin), scope)
            if (home is TvHomeLoadState.Ready && !home.stale) {
                try {
                    watchNext.sync(scope, home.surface)
                } catch (_: Exception) {
                    Unit
                }
            }
        }
    }

    fun handleWatchNextProgramDisabled(programId: Long, onComplete: () -> Unit) {
        runtimeScope.launch {
            try {
                watchNext.handleProgramDisabled(programId)
            } catch (_: Exception) {
                Unit
            } finally {
                onComplete()
            }
        }
    }

    suspend fun startInteractivePlayback(
        sessionId: String,
        streamUrl: String,
        mediaItemId: String,
        title: String,
        startPositionMs: Long,
        qualityMode: String,
        audioLanguage: String?,
        subtitleLanguage: String?,
    ): Boolean {
        val session = sessionStore.current().session ?: return false
        if (session.profile_selection_required) return false
        TvPlaybackService.start(
            applicationContext,
            TvInteractivePlayback(
                serverOrigin = session.origin,
                bearerToken = session.token,
                sessionId = sessionId,
                streamUrl = streamUrl,
                mediaItemId = mediaItemId,
                title = title,
                startPositionMs = startPositionMs,
                qualityMode = qualityMode,
                audioLanguage = audioLanguage,
                subtitleLanguage = subtitleLanguage,
            ),
        )
        return true
    }

    fun pausePlayback() {
        TvPlaybackService.pause()
    }

    fun stopPlayback() {
        TvPlaybackService.stop(applicationContext)
    }
}
