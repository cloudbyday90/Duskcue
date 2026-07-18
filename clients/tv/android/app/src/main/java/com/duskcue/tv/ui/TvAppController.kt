package com.duskcue.tv.ui

import com.duskcue.tv.TvApplicationRuntime
import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.TvCollection
import com.duskcue.tv.api.TvLibrary
import com.duskcue.tv.api.TvMediaItem
import com.duskcue.tv.api.TvResolveResponse
import com.duskcue.tv.api.TvSearchResponse
import com.duskcue.tv.api.TvSurfaceItem
import com.duskcue.tv.api.TvSurfaceSettings
import com.duskcue.tv.api.UpdateTvSurfaceSettingsRequest
import com.duskcue.tv.api.ServerSentEvent
import com.duskcue.tv.home.TvHomeLoadState
import com.duskcue.tv.session.DeviceLinkChallenge
import com.duskcue.tv.session.DeviceLinkPollResult
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

enum class TvAppPhase {
    Launching,
    ServerSetup,
    DeviceLink,
    ProfilePicker,
    SignedIn,
}

enum class TvRoute {
    Home,
    Browse,
    Detail,
    Search,
    Settings,
    Profiles,
}

data class TvDetail(
    val mediaItemId: String,
    val platformContentId: String,
    val title: String,
    val subtitle: String? = null,
    val description: String? = null,
    val availability: String = "playable",
)

data class TvAppState(
    val phase: TvAppPhase = TvAppPhase.Launching,
    val route: TvRoute = TvRoute.Home,
    val originInput: String = "",
    val deviceLink: DeviceLinkChallenge? = null,
    val profiles: ProfileListResponse? = null,
    val rememberProfile: Boolean = false,
    val parentPin: String = "",
    val home: TvHomeLoadState = TvHomeLoadState.Loading,
    val libraries: List<TvLibrary> = emptyList(),
    val collections: List<TvCollection> = emptyList(),
    val browseItems: List<TvMediaItem> = emptyList(),
    val browseTitle: String? = null,
    val searchQuery: String = "",
    val searchResult: TvSearchResponse? = null,
    val detail: TvDetail? = null,
    val prePlayback: ApiResult<TvResolveResponse>? = null,
    val tvSettings: TvSurfaceSettings? = null,
    val busy: Boolean = false,
    val message: String? = null,
)

class TvAppController(
    private val runtime: TvApplicationRuntime,
    private val scope: CoroutineScope,
) {
    private val mutableState = MutableStateFlow(TvAppState())
    val state: StateFlow<TvAppState> = mutableState.asStateFlow()

    fun bootstrap() {
        scope.launch {
            val restored = withContext(Dispatchers.IO) { runtime.authentication.restore() }
            when (restored) {
                null -> showServerSetup()
                is ApiResult.Success -> {
                    val origin = withContext(Dispatchers.IO) { runtime.activeSession()?.origin?.value }
                    mutableState.update { it.copy(originInput = origin ?: it.originInput) }
                    enterAuthenticated(restored.value)
                }
                is ApiResult.Failure -> showServerSetup(restored.problem.title ?: "Sign in again to continue.")
                ApiResult.NetworkFailure, ApiResult.NotModified -> showServerSetup("Duskcue could not restore this session.")
            }
        }
    }

    fun updateOrigin(value: String) = mutableState.update { it.copy(originInput = value, message = null) }

    fun beginDeviceLink() {
        scope.launch {
            val origin = ServerOrigin.parse(state.value.originInput).getOrElse {
                mutableState.update { current -> current.copy(message = "Enter a server URL using http or https on port 48027.") }
                return@launch
            }
            mutableState.update { it.copy(busy = true, message = null) }
            when (val result = withContext(Dispatchers.IO) { runtime.authentication.beginDeviceLink(origin) }) {
                is ApiResult.Success -> {
                    mutableState.update { it.copy(phase = TvAppPhase.DeviceLink, deviceLink = result.value, busy = false) }
                    pollDeviceLink(result.value)
                }
                is ApiResult.Failure -> mutableState.update {
                    it.copy(busy = false, message = result.problem.title ?: "Duskcue could not start device linking.")
                }
                ApiResult.NetworkFailure, ApiResult.NotModified -> mutableState.update {
                    it.copy(busy = false, message = "Duskcue could not reach this server.")
                }
            }
        }
    }

    fun setRememberProfile(value: Boolean) = mutableState.update { it.copy(rememberProfile = value) }

    fun updateParentPin(value: String) = mutableState.update { it.copy(parentPin = value.filter(Char::isDigit).take(12), message = null) }

    fun unlockParentProfile() {
        scope.launch {
            val pin = state.value.parentPin
            if (pin.length !in 4..12) {
                mutableState.update { it.copy(message = "Enter the 4 to 12 digit parent PIN.") }
                return@launch
            }
            mutableState.update { it.copy(busy = true, message = null) }
            when (val result = withContext(Dispatchers.IO) { runtime.authentication.unlockParentProfile(pin) }) {
                is ApiResult.Success -> refreshProfilesAfterUnlock()
                is ApiResult.Failure -> mutableState.update { it.copy(busy = false, message = result.problem.title ?: "Parent access is unavailable.") }
                ApiResult.NetworkFailure, ApiResult.NotModified -> mutableState.update {
                    it.copy(busy = false, message = "Duskcue could not verify parent access.")
                }
            }
        }
    }

    fun selectProfile(profileId: String) {
        scope.launch {
            mutableState.update { it.copy(busy = true, message = null) }
            when (val result = withContext(Dispatchers.IO) {
                runtime.authentication.switchProfile(profileId, state.value.rememberProfile)
            }) {
                is ApiResult.Success -> refreshProfilesAfterSwitch()
                is ApiResult.Failure -> mutableState.update {
                    it.copy(busy = false, message = result.problem.title ?: "This profile is unavailable on this TV.")
                }
                ApiResult.NetworkFailure, ApiResult.NotModified -> mutableState.update {
                    it.copy(busy = false, message = "Duskcue could not switch this profile.")
                }
            }
        }
    }

    fun goHome() {
        mutableState.update { it.copy(route = TvRoute.Home, detail = null, prePlayback = null, message = null) }
        refreshHome()
    }

    fun openBrowse() {
        mutableState.update { it.copy(route = TvRoute.Browse, busy = true, message = null, browseItems = emptyList(), browseTitle = null) }
        scope.launch {
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val results = withContext(Dispatchers.IO) {
                val client = runtime.client(session.origin)
                client.libraries() to client.collections()
            }
            if (expireIfUnauthorized(results.first) || expireIfUnauthorized(results.second)) {
                return@launch
            }
            val libraries = (results.first as? ApiResult.Success)?.value?.items.orEmpty()
            val collections = (results.second as? ApiResult.Success)?.value?.items.orEmpty()
            val message = resultMessage(results.first) ?: resultMessage(results.second)
            mutableState.update { it.copy(libraries = libraries, collections = collections, busy = false, message = message) }
        }
    }

    fun openLibrary(library: TvLibrary) = openBrowseItems(library.name) { client -> client.libraryItems(library.id) }

    fun openCollection(collection: TvCollection) = openBrowseItems(collection.name) { client -> client.collectionItems(collection.id) }

    fun updateSearchQuery(value: String) = mutableState.update { it.copy(searchQuery = value, message = null) }

    fun openSearch() = mutableState.update { it.copy(route = TvRoute.Search, message = null) }

    fun search() {
        scope.launch {
            val query = state.value.searchQuery.trim()
            if (query.isEmpty()) {
                mutableState.update { it.copy(searchResult = TvSearchResponse(emptyList()), message = "Enter a title to search.") }
                return@launch
            }
            mutableState.update { it.copy(busy = true, message = null) }
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val result = withContext(Dispatchers.IO) { runtime.client(session.origin).search(query) }
            if (expireIfUnauthorized(result)) {
                return@launch
            }
            when (result) {
                is ApiResult.Success -> mutableState.update { it.copy(searchResult = result.value, busy = false) }
                else -> mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "Search is unavailable.") }
            }
        }
    }

    fun showDetail(item: TvSurfaceItem) = showDetail(
        TvDetail(
            mediaItemId = item.media_item_id,
            platformContentId = item.platform_content_id,
            title = item.title,
            subtitle = item.subtitle,
            description = item.description,
            availability = item.availability,
        ),
    )

    fun showDetail(item: TvMediaItem) = showDetail(
        TvDetail(
            mediaItemId = item.id,
            platformContentId = "duskcue:${item.type}:${item.id}",
            title = item.title,
            subtitle = item.type.replaceFirstChar(Char::titlecase),
            description = item.overview,
        ),
    )

    fun preparePlayback() {
        val detail = state.value.detail ?: return
        scope.launch {
            mutableState.update { it.copy(busy = true, prePlayback = null, message = null) }
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val result = withContext(Dispatchers.IO) { runtime.client(session.origin).resolveTvItem(detail.platformContentId) }
            if (result is ApiResult.Failure && result.status == 401) {
                withContext(Dispatchers.IO) { runtime.authentication.handleSessionKicked() }
                showServerSetup("Sign in again to continue.")
                return@launch
            }
            mutableState.update { it.copy(prePlayback = result, busy = false, message = resultMessage(result)) }
        }
    }

    fun openSettings() {
        mutableState.update { it.copy(route = TvRoute.Settings, busy = true, message = null) }
        scope.launch {
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val result = withContext(Dispatchers.IO) { runtime.client(session.origin).tvSettings() }
            if (expireIfUnauthorized(result)) {
                return@launch
            }
            when (result) {
                is ApiResult.Success -> mutableState.update { it.copy(tvSettings = result.value, busy = false) }
                else -> mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "TV settings are unavailable.") }
            }
        }
    }

    fun setTvPublication(enabled: Boolean) {
        scope.launch {
            mutableState.update { it.copy(busy = true, message = null) }
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val result = withContext(Dispatchers.IO) {
                runtime.client(session.origin).updateTvSettings(UpdateTvSurfaceSettingsRequest(tv_publication_enabled = enabled))
            }
            if (expireIfUnauthorized(result)) {
                return@launch
            }
            when (result) {
                is ApiResult.Success -> mutableState.update { it.copy(tvSettings = result.value, busy = false) }
                else -> mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "TV publication could not be updated.") }
            }
        }
    }

    fun openProfiles() {
        scope.launch {
            mutableState.update { it.copy(route = TvRoute.Profiles, busy = true, message = null) }
            val result = withContext(Dispatchers.IO) { runtime.authentication.refreshProfiles() }
            if (expireIfUnauthorized(result)) {
                return@launch
            }
            when (result) {
                is ApiResult.Success -> mutableState.update { it.copy(profiles = result.value, busy = false) }
                else -> mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "Profiles are unavailable.") }
            }
        }
    }

    fun changeServer() {
        mutableState.value = TvAppState(
            phase = TvAppPhase.ServerSetup,
            originInput = state.value.originInput,
        )
    }

    fun logout(allSessions: Boolean = false) {
        scope.launch {
            mutableState.update { it.copy(busy = true, message = null) }
            withContext(Dispatchers.IO) { runtime.authentication.logout(allSessions) }
            showServerSetup()
        }
    }

    fun onForegroundSurfaceEvent(event: ServerSentEvent) {
        scope.launch {
            val scope = runtime.activeProfileScope() ?: return@launch
            if (runtime.livingRoom.shouldRefresh(event, scope)) {
                refreshHome()
            }
        }
    }

    private fun pollDeviceLink(challenge: DeviceLinkChallenge) {
        scope.launch {
            val deadline = System.currentTimeMillis() + challenge.expiresInSeconds * 1_000L
            var delaySeconds = challenge.pollIntervalSeconds
            while (System.currentTimeMillis() < deadline && state.value.deviceLink == challenge) {
                delay(delaySeconds * 1_000L)
                when (val result = withContext(Dispatchers.IO) { runtime.authentication.pollDeviceLink(challenge) }) {
                    is DeviceLinkPollResult.Authorized -> {
                        val profiles = withContext(Dispatchers.IO) { runtime.authentication.refreshProfiles() }
                        if (profiles is ApiResult.Success) {
                            enterAuthenticated(profiles.value)
                        } else {
                            showServerSetup("Duskcue linked this TV but could not load profiles.")
                        }
                        return@launch
                    }
                    is DeviceLinkPollResult.Pending -> delaySeconds = result.nextPollAfterSeconds
                    is DeviceLinkPollResult.TerminalFailure -> {
                        showServerSetup(result.failure.problem.title ?: "Device linking expired or was denied.")
                        return@launch
                    }
                    DeviceLinkPollResult.NetworkFailure -> {
                        mutableState.update { it.copy(message = "Waiting for Duskcue to reconnect.") }
                    }
                }
            }
            if (state.value.deviceLink == challenge) {
                showServerSetup("Device linking expired. Start again for a new code.")
            }
        }
    }

    private fun refreshProfilesAfterUnlock() {
        scope.launch {
            val result = withContext(Dispatchers.IO) { runtime.authentication.refreshProfiles() }
            if (result is ApiResult.Success) {
                mutableState.update { it.copy(profiles = result.value, parentPin = "", busy = false) }
            } else {
                mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "Profiles are unavailable.") }
            }
        }
    }

    private fun refreshProfilesAfterSwitch() {
        scope.launch {
            val result = withContext(Dispatchers.IO) { runtime.authentication.refreshProfiles() }
            if (result is ApiResult.Success) {
                enterAuthenticated(result.value)
            } else {
                mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "Profiles are unavailable.") }
            }
        }
    }

    private fun openBrowseItems(title: String, request: (com.duskcue.tv.api.DuskcueApiClient) -> ApiResult<com.duskcue.tv.api.TvMediaItemPage>) {
        scope.launch {
            mutableState.update { it.copy(busy = true, message = null, browseTitle = title) }
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val result = withContext(Dispatchers.IO) { request(runtime.client(session.origin)) }
            if (expireIfUnauthorized(result)) {
                return@launch
            }
            when (result) {
                is ApiResult.Success -> mutableState.update { it.copy(browseItems = result.value.items, busy = false) }
                else -> mutableState.update { it.copy(busy = false, message = resultMessage(result) ?: "This collection is unavailable.") }
            }
        }
    }

    private fun enterAuthenticated(profiles: ProfileListResponse) {
        if (profiles.profile_selection_required) {
            mutableState.value = TvAppState(
                phase = TvAppPhase.ProfilePicker,
                originInput = state.value.originInput,
                profiles = profiles,
                rememberProfile = profiles.remembered_profile_id != null,
            )
        } else {
            mutableState.value = TvAppState(
                phase = TvAppPhase.SignedIn,
                route = TvRoute.Home,
                originInput = state.value.originInput,
                profiles = profiles,
                rememberProfile = profiles.remembered_profile_id != null,
            )
            refreshHome()
        }
    }

    private fun refreshHome() {
        scope.launch {
            val scope = runtime.activeProfileScope() ?: return@launch openProfiles()
            mutableState.update { it.copy(home = TvHomeLoadState.Loading, busy = true) }
            val home = withContext(Dispatchers.IO) { runtime.livingRoom.load(runtime.client(ServerOrigin.parse(scope.origin).getOrThrow()), scope) }
            if (home == TvHomeLoadState.SessionExpired) {
                withContext(Dispatchers.IO) { runtime.authentication.handleSessionKicked() }
                showServerSetup("Sign in again to continue.")
                return@launch
            }
            mutableState.update { it.copy(home = home, busy = false) }
        }
    }

    private fun showDetail(detail: TvDetail) {
        mutableState.update { it.copy(route = TvRoute.Detail, detail = detail, prePlayback = null, message = null) }
    }

    private fun showServerSetup(message: String? = null) {
        mutableState.value = TvAppState(
            phase = TvAppPhase.ServerSetup,
            originInput = state.value.originInput,
            message = message,
        )
    }

    private suspend fun expireIfUnauthorized(result: ApiResult<*>): Boolean {
        if (result is ApiResult.Failure && result.status == 401) {
            withContext(Dispatchers.IO) { runtime.authentication.handleSessionKicked() }
            showServerSetup("Sign in again to continue.")
            return true
        }
        return false
    }

    private fun resultMessage(result: ApiResult<*>): String? = when (result) {
        is ApiResult.Failure -> result.problem.title ?: result.problem.detail
        ApiResult.NetworkFailure -> "Duskcue could not reach this server."
        else -> null
    }
}
