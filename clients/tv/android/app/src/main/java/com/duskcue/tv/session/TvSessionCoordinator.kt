package com.duskcue.tv.session

import com.duskcue.tv.api.AuthenticatedUser
import com.duskcue.tv.api.DeviceTokenResponse
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.SwitchProfileResponse
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.profiles.ProfileGateState
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

interface TvLocalStateCleaner {
    suspend fun clearProfileScope()
    suspend fun clearIdentityScope()
}

class TvSessionCoordinator(
    private val store: TvSessionStore,
    private val tokenProvider: MutableBearerTokenProvider,
    private val cleaner: TvLocalStateCleaner,
    private val nowMillis: () -> Long = System::currentTimeMillis,
) {
    private val scopeMutex = Mutex()
    private var scopeVersion = 0L

    suspend fun sessionSnapshot(): Pair<PersistedAccountSession, Long>? = scopeMutex.withLock {
        store.current().session?.let { it to scopeVersion }
    }

    suspend fun selectServer(origin: ServerOrigin): SecureTvState = scopeMutex.withLock {
        val current = store.current()
        if (current.session?.let { it.origin != origin.value } == true) {
            clearIdentityScope()
            tokenProvider.clear()
        }
        val next = current.copy(
            known_servers = remember(current.known_servers, origin),
            session = if (current.session?.origin == origin.value) current.session else null,
        )
        store.replace(next)
        next
    }

    suspend fun completeDeviceLink(origin: ServerOrigin, response: DeviceTokenResponse): ProfileGateState = scopeMutex.withLock {
        val current = store.current()
        val existing = current.session
        val replacement = existing != null && (existing.origin != origin.value || existing.user_id != response.user.id)
        val profileChanged = existing != null && (
            existing.active_profile_id != response.user.active_profile_id ||
                existing.profile_selection_required != response.user.profile_selection_required
            )
        when {
            replacement -> clearIdentityScope()
            profileChanged || response.user.profile_selection_required -> clearProfileScope()
        }
        scopeVersion += 1
        val session = response.user.toSession(origin, response.session_token)
        store.replace(current.copy(known_servers = remember(current.known_servers, origin), session = session))
        tokenProvider.replace(response.session_token)
        ProfileGateState(profileSelectionRequired = response.user.profile_selection_required)
    }

    suspend fun applyProfileList(response: ProfileListResponse): ProfileGateState = scopeMutex.withLock {
        val current = store.current()
        val session = requireNotNull(current.session)
        val profileChanged = session.active_profile_id != response.active_profile_id ||
            session.profile_selection_required != response.profile_selection_required
        if (profileChanged || response.profile_selection_required) {
            clearProfileScope()
        }
        store.replace(
            current.copy(
                session = session.copy(
                    active_profile_id = response.active_profile_id,
                    profile_selection_required = response.profile_selection_required,
                ),
            ),
        )
        ProfileGateState(
            profileSelectionRequired = response.profile_selection_required,
            parentUnlockRequired = response.parent_unlock_required,
        )
    }

    suspend fun applyProfileSwitch(response: SwitchProfileResponse): ProfileGateState = scopeMutex.withLock {
        val current = store.current()
        val session = requireNotNull(current.session)
        clearProfileScope()
        store.replace(
            current.copy(
                session = session.copy(
                    active_profile_id = response.active_profile.id,
                    profile_selection_required = response.profile_selection_required,
                ),
            ),
        )
        ProfileGateState(
            profileSelectionRequired = response.profile_selection_required,
            parentUnlockRequired = response.parent_unlock_required,
        )
    }

    suspend fun clearIdentity() = scopeMutex.withLock {
        val current = store.current()
        clearIdentityScope()
        tokenProvider.clear()
        store.replace(current.copy(session = null))
    }

    suspend fun restoreToken(): PersistedAccountSession? = scopeMutex.withLock {
        val session = store.current().session
        tokenProvider.replace(session?.token)
        session
    }

    private suspend fun clearProfileScope() {
        scopeVersion += 1
        cleaner.clearProfileScope()
    }

    private suspend fun clearIdentityScope() {
        scopeVersion += 1
        cleaner.clearIdentityScope()
    }

    private fun remember(servers: List<SavedServer>, origin: ServerOrigin): List<SavedServer> =
        (servers.filterNot { it.origin == origin.value } + SavedServer(origin.value, nowMillis()))
            .sortedByDescending(SavedServer::last_used_at)
            .take(10)

    private fun AuthenticatedUser.toSession(origin: ServerOrigin, token: String): PersistedAccountSession =
        PersistedAccountSession(
            origin = origin.value,
            token = token,
            user_id = id,
            username = username,
            display_name = display_name,
            role = role,
            active_profile_id = active_profile_id,
            profile_selection_required = profile_selection_required,
        )
}
