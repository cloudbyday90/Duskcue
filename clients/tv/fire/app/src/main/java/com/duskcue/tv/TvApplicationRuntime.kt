package com.duskcue.tv

import android.content.Context
import android.os.SystemClock
import com.duskcue.fire.FireTvDeviceCapabilityCollector
import com.duskcue.fire.FireTvTargetPolicy
import com.duskcue.fire.personalization.FireDevicePlaybackSink
import com.duskcue.fire.personalization.FirePlaybackAuthorizationSource
import com.duskcue.fire.personalization.FirePlaybackAuthorizationController
import com.duskcue.fire.personalization.FireWatchActivityReporter
import com.duskcue.fire.personalization.ServerFirePlaybackAuthorizationSource
import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.DuskcueApiClient
import com.duskcue.tv.api.MemoryEtagStore
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.RetryingTransport
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.TvDeviceProfile
import com.duskcue.tv.api.TvPlatform
import com.duskcue.tv.api.TvSurface
import com.duskcue.tv.api.UrlConnectionTransport
import com.duskcue.tv.diagnostics.TvDiagnostics
import com.duskcue.tv.home.TvLivingRoomStore
import com.duskcue.tv.home.TvProfileScope
import com.duskcue.tv.playback.TvInteractivePlayback
import com.duskcue.tv.playback.TvPlaybackService
import com.duskcue.tv.session.SecureSessionStore
import com.duskcue.tv.session.TvAuthenticationService
import com.duskcue.tv.session.TvLocalStateCleaner
import com.duskcue.tv.session.TvSessionCoordinator
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.withContext

data class ActiveTvSession(
    val origin: ServerOrigin,
    val userId: String,
    val profileId: String,
    val profileSelectionRequired: Boolean,
    val scopeVersion: Long,
)

class TvApplicationRuntime(
    context: Context,
    fireAuthorizationSource: FirePlaybackAuthorizationSource? = null,
    customerOptedIn: () -> Boolean = { false },
) {
    private val applicationContext = context.applicationContext
    private val tokenProvider = MutableBearerTokenProvider()
    private val etags = MemoryEtagStore()
    private val sessionStore = SecureSessionStore(context)
    private val fireWatchActivity = FireWatchActivityReporter(
        sink = FireDevicePlaybackSink(applicationContext),
        featureEnabled = { BuildConfig.FIRE_WATCH_ACTIVITY_ENABLED },
        nowMs = System::currentTimeMillis,
        elapsedMs = SystemClock::elapsedRealtime,
    )
    private val fireAuthorization = FirePlaybackAuthorizationController(
        reporter = fireWatchActivity,
        scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        source = fireAuthorizationSource ?: ServerFirePlaybackAuthorizationSource(
            customerOptedIn = { BuildConfig.FIRE_WATCH_ACTIVITY_ENABLED && customerOptedIn() },
            request = { playbackSessionId ->
                val session = sessionStore.current().session
                val origin = session?.let { ServerOrigin.parse(it.origin).getOrNull() }
                if (session == null || origin == null || session.profile_selection_required) null else {
                    val response = withContext(Dispatchers.IO) { client(origin).firePlaybackAuthorization(playbackSessionId) }
                    if (sessionStore.current().session != session) null
                    else (response as? ApiResult.Success)?.value
                }
            },
        ),
    )
    val diagnostics = TvDiagnostics(
        clientVersion = BuildConfig.VERSION_NAME,
        platform = FireTvTargetPolicy.platform,
        capabilityReportProvider = { route ->
            FireTvDeviceCapabilityCollector.collect(applicationContext, BuildConfig.VERSION_NAME, route)
        },
    )
    val livingRoom = TvLivingRoomStore(etags = etags, platform = TvPlatform.FireTv)
    private val localStateCleaner = object : TvLocalStateCleaner {
        override suspend fun clearProfileScope() {
            fireAuthorization.clear()
            diagnostics.clear()
            TvPlaybackService.stop(applicationContext)
            livingRoom.clearProfileScope()
        }

        override suspend fun clearIdentityScope() {
            fireAuthorization.clear()
            diagnostics.clear()
            TvPlaybackService.stop(applicationContext)
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

    init {
        TvPlaybackService.configureDiagnostics(diagnostics)
        TvPlaybackService.configurePlaybackObserver(fireAuthorization)
    }

    fun client(origin: ServerOrigin): DuskcueApiClient = DuskcueApiClient(
        origin = origin,
        transport = RetryingTransport(UrlConnectionTransport()),
        tokenProvider = tokenProvider,
        etagStore = etags,
        diagnostics = diagnostics,
    )

    fun deviceProfile(): TvDeviceProfile = TvDeviceProfile.fireTv()

    fun platform(): TvPlatform = TvPlatform.FireTv

    fun resolveTvItem(origin: ServerOrigin, platformContentId: String) =
        client(origin).resolveTvItem(platformContentId, platform = platform())

    suspend fun activeSession(): ActiveTvSession? {
        val (session, version) = coordinator.sessionSnapshot() ?: return null
        val origin = ServerOrigin.parse(session.origin).getOrNull() ?: return null
        return ActiveTvSession(
            origin = origin,
            userId = session.user_id,
            profileId = session.active_profile_id,
            profileSelectionRequired = session.profile_selection_required,
            scopeVersion = version,
        )
    }

    suspend fun activeProfileScope(): TvProfileScope? = activeSession()
        ?.takeUnless(ActiveTvSession::profileSelectionRequired)
        ?.let { TvProfileScope(it.origin.value, it.userId, it.profileId) }

    suspend fun refreshPlatformSurface(scope: TvProfileScope, surface: TvSurface) = Unit

    fun refreshPlatformSurface() = Unit

    suspend fun startInteractivePlayback(
        sessionId: String,
        streamUrl: String,
        mediaItemId: String,
        title: String,
        startPositionMs: Long,
        qualityMode: String,
        streamDecision: String,
        audioLanguage: String?,
        subtitleLanguage: String?,
        expectedSession: ActiveTvSession? = null,
        requestIsCurrent: () -> Boolean = { true },
    ): Boolean {
        if (!requestIsCurrent() || (expectedSession != null && activeSession() != expectedSession)) return false
        val session = sessionStore.current().session ?: return false
        if (session.profile_selection_required) return false
        if (!fireAuthorization.prepare(mediaItemId, sessionId)) return false
        if (sessionStore.current().session != session) return false
        if (!requestIsCurrent() || (expectedSession != null && activeSession() != expectedSession)) {
            fireAuthorization.clearIfSession(sessionId)
            return false
        }
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
                streamDecision = streamDecision,
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

    fun exportDiagnosticsBundle(): String = diagnostics.exportBundleJson()

    fun isSupportedFireOs(apiLevel: Int): Boolean =
        FireTvTargetPolicy.platform == "fire_tv" && FireTvTargetPolicy.supportsApi(apiLevel)
}
