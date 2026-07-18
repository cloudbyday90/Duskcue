package com.duskcue.tv

import android.content.Context
import com.duskcue.tv.api.DuskcueApiClient
import com.duskcue.tv.api.MemoryEtagStore
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.RetryingTransport
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.UrlConnectionTransport
import com.duskcue.tv.home.TvLivingRoomStore
import com.duskcue.tv.home.TvProfileScope
import com.duskcue.tv.session.SecureSessionStore
import com.duskcue.tv.session.TvAuthenticationService
import com.duskcue.tv.session.TvSessionCoordinator

data class ActiveTvSession(
    val origin: ServerOrigin,
    val userId: String,
    val profileId: String,
    val profileSelectionRequired: Boolean,
)

class TvApplicationRuntime(context: Context) {
    private val tokenProvider = MutableBearerTokenProvider()
    private val etags = MemoryEtagStore()
    private val sessionStore = SecureSessionStore(context)
    val livingRoom = TvLivingRoomStore(etags = etags)
    private val coordinator = TvSessionCoordinator(
        store = sessionStore,
        tokenProvider = tokenProvider,
        cleaner = livingRoom,
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
}
