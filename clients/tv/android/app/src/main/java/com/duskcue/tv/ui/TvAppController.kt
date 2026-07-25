package com.duskcue.tv.ui

import com.duskcue.tv.TvApplicationRuntime
import com.duskcue.tv.TvDeepLink
import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.StartTvPlaybackRequest
import com.duskcue.tv.api.TvCollection
import com.duskcue.tv.api.TvDeviceProfile
import com.duskcue.tv.api.TvLibrary
import com.duskcue.tv.api.TvMediaItem
import com.duskcue.tv.api.TvMediaFile
import com.duskcue.tv.api.TvPlaybackStartResponse
import com.duskcue.tv.api.TvResolveResponse
import com.duskcue.tv.api.TvSearchResponse
import com.duskcue.tv.api.TvSegment
import com.duskcue.tv.api.TvSurfaceItem
import com.duskcue.tv.api.TvSurfaceSettings
import com.duskcue.tv.api.UpdateTvSurfaceSettingsRequest
import com.duskcue.tv.api.ServerSentEvent
import com.duskcue.tv.home.TvHomeLoadState
import com.duskcue.tv.playback.TvPlaybackService
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
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

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
    Player,
}

data class TvDetail(
    val mediaItemId: String,
    val platformContentId: String,
    val title: String,
    val subtitle: String? = null,
    val description: String? = null,
    val availability: String = "playable",
)

data class TvTrackOption(
    val index: Int,
    val label: String,
    val language: String? = null,
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
    val playbackFileId: String? = null,
    val audioTracks: List<TvTrackOption> = emptyList(),
    val subtitleTracks: List<TvTrackOption> = emptyList(),
    val segments: List<TvSegment> = emptyList(),
    val selectedAudioTrackIndex: Int? = null,
    val selectedSubtitleTrackIndex: Int? = null,
    val prePlayback: ApiResult<TvResolveResponse>? = null,
    val playback: TvPlaybackStartResponse? = null,
    val qualityMode: String = "auto",
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
    private var pendingDeepLink: TvDeepLink.Playback? = null
    private var pendingDeepLinkFailure = false

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

    fun handleDeepLink(rawUri: String?) {
        when (val deepLink = TvDeepLink.parse(rawUri)) {
            TvDeepLink.Absent -> Unit
            TvDeepLink.Invalid -> {
                pendingDeepLink = null
                pendingDeepLinkFailure = true
                openPendingDeepLink()
            }
            is TvDeepLink.Playback -> {
                pendingDeepLink = deepLink
                pendingDeepLinkFailure = false
                openPendingDeepLink()
            }
        }
    }

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

    fun startPlayback() {
        val detail = state.value.detail ?: return
        scope.launch {
            val playbackState = state.value
            mutableState.update { it.copy(busy = true, prePlayback = null, message = null) }
            val session = runtime.activeSession() ?: return@launch showServerSetup("Sign in again to continue.")
            val resolve = withContext(Dispatchers.IO) { runtime.client(session.origin).resolveTvItem(detail.platformContentId) }
            if (expireIfUnauthorized(resolve)) {
                return@launch
            }
            val resolved = (resolve as? ApiResult.Success)?.value ?: run {
                mutableState.update { it.copy(busy = false, message = resultMessage(resolve) ?: "This title is unavailable.") }
                return@launch
            }
            if (!resolved.access_revalidated || resolved.availability != "playable" || resolved.playback_action != "start_playback") {
                mutableState.update { it.copy(busy = false, message = "This title is unavailable.") }
                return@launch
            }
            val start = withContext(Dispatchers.IO) {
                runtime.client(session.origin).startTvPlayback(
                    StartTvPlaybackRequest(
                        media_item_id = resolved.media_item_id,
                        media_file_id = playbackState.playbackFileId,
                        audio_stream_index = playbackState.selectedAudioTrackIndex,
                        subtitle_stream_index = playbackState.selectedSubtitleTrackIndex,
                        device_profile = TvDeviceProfile.androidTv(),
                        quality_mode = playbackState.qualityMode,
                    ),
                )
            }
            if (expireIfUnauthorized(start)) {
                return@launch
            }
            val playback = (start as? ApiResult.Success)?.value ?: run {
                mutableState.update { it.copy(busy = false, message = resultMessage(start) ?: "Playback could not start.") }
                return@launch
            }
            val started = runtime.startInteractivePlayback(
                sessionId = playback.session_id,
                streamUrl = playback.stream_url,
                mediaItemId = playback.media_item_id,
                title = detail.title,
                startPositionMs = resolved.resume_position_ms,
                qualityMode = playbackState.qualityMode,
                audioLanguage = playbackState.audioTracks.find { it.index == playbackState.selectedAudioTrackIndex }?.language,
                subtitleLanguage = playbackState.subtitleTracks.find { it.index == playbackState.selectedSubtitleTrackIndex }?.language,
            )
            if (!started) {
                mutableState.update { it.copy(busy = false, message = "Choose a profile before playback starts.") }
                return@launch
            }
            mutableState.update { it.copy(route = TvRoute.Player, playback = playback, busy = false) }
        }
    }

    fun exitPlayback() {
        runtime.stopPlayback()
        TvPlaybackService.clearPlaybackUi()
        mutableState.update { it.copy(route = TvRoute.Detail, playback = null) }
        refreshWatchNextAfterPlayback()
    }

    fun onPlaybackCompleted() {
        TvPlaybackService.clearPlaybackUi()
        mutableState.update { current ->
            current.copy(
                route = if (current.detail == null) TvRoute.Home else TvRoute.Detail,
                playback = null,
                message = null,
            )
        }
        refreshWatchNextAfterPlayback()
    }

    fun onPlaybackPausedTooLong() {
        runtime.refreshWatchNext()
    }

    fun cycleQualityMode() {
        mutableState.update {
            val next = when (it.qualityMode) {
                "auto" -> "maximum"
                "maximum" -> "manual"
                else -> "auto"
            }
            it.copy(qualityMode = next)
        }
    }

    fun cycleAudioTrack() {
        mutableState.update { current ->
            val next = nextTrackIndex(current.audioTracks, current.selectedAudioTrackIndex)
            current.copy(selectedAudioTrackIndex = next)
        }
    }

    fun cycleSubtitleTrack() {
        mutableState.update { current ->
            val next = nextTrackIndex(current.subtitleTracks, current.selectedSubtitleTrackIndex)
            current.copy(selectedSubtitleTrackIndex = next)
        }
    }

    fun skipSegment(segment: TvSegment) {
        TvPlaybackService.seekTo(segment.skip_to_ms)
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
            if (!openPendingDeepLink()) {
                refreshHome()
            }
        }
    }

    private fun openPendingDeepLink(): Boolean {
        if (state.value.phase != TvAppPhase.SignedIn) return false
        if (pendingDeepLinkFailure) {
            pendingDeepLinkFailure = false
            showDeepLinkUnavailable()
            return true
        }
        val deepLink = pendingDeepLink ?: return false
        scope.launch {
            val session = runtime.activeSession() ?: return@launch showServerSetup("Link this TV to continue.")
            if (session.profileSelectionRequired) return@launch openProfiles()
            mutableState.update { it.copy(busy = true, message = null, prePlayback = null) }
            val resolve = withContext(Dispatchers.IO) {
                runtime.client(session.origin).resolveTvItem(deepLink.platformContentId)
            }
            if (resolve is ApiResult.Failure && resolve.status == 401) {
                withContext(Dispatchers.IO) { runtime.authentication.handleSessionKicked() }
                showServerSetup("Link this TV to continue.")
                return@launch
            }
            val resolved = (resolve as? ApiResult.Success)?.value
            if (resolved == null ||
                !resolved.access_revalidated ||
                resolved.availability != "playable" ||
                resolved.playback_action != "start_playback"
            ) {
                pendingDeepLink = null
                showDeepLinkUnavailable()
                return@launch
            }
            val start = withContext(Dispatchers.IO) {
                runtime.client(session.origin).startTvPlayback(
                    StartTvPlaybackRequest(
                        media_item_id = resolved.media_item_id,
                        device_profile = TvDeviceProfile.androidTv(),
                    ),
                )
            }
            if (start is ApiResult.Failure && start.status == 401) {
                withContext(Dispatchers.IO) { runtime.authentication.handleSessionKicked() }
                showServerSetup("Link this TV to continue.")
                return@launch
            }
            val playback = (start as? ApiResult.Success)?.value
            if (playback == null) {
                pendingDeepLink = null
                showDeepLinkUnavailable()
                return@launch
            }
            val started = runtime.startInteractivePlayback(
                sessionId = playback.session_id,
                streamUrl = playback.stream_url,
                mediaItemId = playback.media_item_id,
                title = "Duskcue",
                startPositionMs = resolved.resume_position_ms,
                qualityMode = "auto",
                audioLanguage = null,
                subtitleLanguage = null,
            )
            if (!started) {
                mutableState.update { it.copy(busy = false, message = "Choose a profile before playback starts.") }
                return@launch
            }
            pendingDeepLink = null
            mutableState.update {
                it.copy(
                    route = TvRoute.Player,
                    detail = null,
                    playback = playback,
                    busy = false,
                    message = null,
                )
            }
        }
        return true
    }

    private fun showDeepLinkUnavailable() {
        if (state.value.route == TvRoute.Player) {
            runtime.stopPlayback()
            TvPlaybackService.clearPlaybackUi()
        }
        mutableState.update { it.copy(route = TvRoute.Home, busy = false, message = "This item is unavailable.") }
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
            if (home is TvHomeLoadState.Ready && !home.stale) {
                runtime.syncWatchNext(scope, home.surface)
            }
            mutableState.update { it.copy(home = home, busy = false) }
        }
    }

    private fun refreshWatchNextAfterPlayback() {
        scope.launch {
            delay(750)
            refreshHome()
        }
    }

    private fun showDetail(detail: TvDetail) {
        mutableState.update {
            it.copy(
                route = TvRoute.Detail,
                detail = detail,
                playbackFileId = null,
                audioTracks = emptyList(),
                subtitleTracks = emptyList(),
                segments = emptyList(),
                selectedAudioTrackIndex = null,
                selectedSubtitleTrackIndex = null,
                prePlayback = null,
                message = null,
            )
        }
        scope.launch {
            val session = runtime.activeSession() ?: return@launch
            val files = withContext(Dispatchers.IO) { runtime.client(session.origin).mediaFiles(detail.mediaItemId) }
            if (expireIfUnauthorized(files)) return@launch
            val segments = withContext(Dispatchers.IO) { runtime.client(session.origin).segments(detail.mediaItemId) }
            if (expireIfUnauthorized(segments)) return@launch
            val file = (files as? ApiResult.Success)?.value?.firstOrNull()
            val audioTracks = file?.trackOptions("audio").orEmpty()
            val subtitleTracks = file?.trackOptions("subtitles").orEmpty()
            val itemSegments = (segments as? ApiResult.Success)?.value?.segments.orEmpty()
            mutableState.update { current ->
                if (current.detail?.mediaItemId != detail.mediaItemId) current else current.copy(
                    playbackFileId = file?.id,
                    audioTracks = audioTracks,
                    subtitleTracks = subtitleTracks,
                    segments = itemSegments,
                )
            }
        }
    }

    private fun nextTrackIndex(tracks: List<TvTrackOption>, currentIndex: Int?): Int? {
        if (tracks.isEmpty()) return null
        if (currentIndex == null) return tracks.first().index
        val currentPosition = tracks.indexOfFirst { it.index == currentIndex }
        return if (currentPosition == -1 || currentPosition == tracks.lastIndex) null else tracks[currentPosition + 1].index
    }

    private fun TvMediaFile.trackOptions(category: String): List<TvTrackOption> = additional_streams
        .get(category)
        ?.jsonArray
        ?.mapNotNull { value ->
            val stream = value.jsonObject
            val index = stream["index"]?.jsonPrimitive?.intOrNull ?: return@mapNotNull null
            val language = stream["language"]?.jsonPrimitive?.contentOrNull
            val codec = stream["codec"]?.jsonPrimitive?.contentOrNull
                ?: stream["codec_name"]?.jsonPrimitive?.contentOrNull
            val channels = stream["channels"]?.jsonPrimitive?.intOrNull?.let { "${it}ch" }
            val title = stream["title"]?.jsonPrimitive?.contentOrNull
            TvTrackOption(
                index = index,
                label = listOfNotNull(title, language, codec, channels).joinToString(" · ").ifBlank { "Track $index" },
                language = language,
            )
        }
        .orEmpty()

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
