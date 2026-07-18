package com.duskcue.tv.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.viewinterop.AndroidView
import androidx.activity.compose.BackHandler
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import com.duskcue.tv.R
import com.duskcue.tv.TvDeepLinkRequest
import com.duskcue.tv.TvApplicationRuntime
import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.ProfileSummary
import com.duskcue.tv.api.TvMediaItem
import com.duskcue.tv.api.TvSection
import com.duskcue.tv.api.TvSurfaceItem
import com.duskcue.tv.home.TvHomeLoadState
import com.duskcue.tv.playback.TvPlaybackService
import androidx.media3.ui.PlayerView

private val TvBackground = Color(0xFF0E0F13)
private val TvSurface = Color(0xFF16181F)
private val TvElevated = Color(0xFF1E2129)
private val TvHover = Color(0xFF262A35)
private val TvText = Color(0xFFE8E4DC)
private val TvSecondary = Color(0xFF9B9BA4)
private val TvAccent = Color(0xFFC8965A)
private val TvError = Color(0xFFC95C5C)
private val TvSuccess = Color(0xFF6ABF69)

@Composable
fun DuskcueTvApp(runtime: TvApplicationRuntime, deepLinkRequest: TvDeepLinkRequest) {
    val scope = rememberCoroutineScope()
    val controller = remember(runtime) { TvAppController(runtime, scope) }
    val state by controller.state.collectAsState()

    LaunchedEffect(controller) {
        controller.bootstrap()
    }

    LaunchedEffect(deepLinkRequest.sequence) {
        controller.handleDeepLink(deepLinkRequest.uri)
    }

    MaterialTheme {
        Surface(modifier = Modifier.fillMaxSize()) {
            when (state.phase) {
                TvAppPhase.Launching -> TvStatusPage(stringResource(R.string.tv_restoring_session))
                TvAppPhase.ServerSetup -> ServerSetupPage(state, controller)
                TvAppPhase.DeviceLink -> DeviceLinkPage(state, controller)
                TvAppPhase.ProfilePicker -> ProfilePickerPage(state, controller, allowBack = false)
                TvAppPhase.SignedIn -> SignedInPage(state, controller)
            }
        }
    }
}

@Composable
private fun ServerSetupPage(state: TvAppState, controller: TvAppController) {
    TvPage {
        Text(stringResource(R.string.tv_connect_heading), fontSize = 42.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        Spacer(Modifier.height(18.dp))
        Text(stringResource(R.string.tv_connect_description), fontSize = 24.sp, color = TvSecondary)
        Spacer(Modifier.height(34.dp))
        Text(stringResource(R.string.tv_server_url), fontSize = 21.sp, color = TvText)
        Spacer(Modifier.height(10.dp))
        TvTextInput(
            value = state.originInput,
            placeholder = stringResource(R.string.tv_server_url_placeholder),
            onValueChange = controller::updateOrigin,
            keyboardType = KeyboardType.Uri,
        )
        Spacer(Modifier.height(22.dp))
        TvAction(stringResource(R.string.tv_link_this_tv), enabled = !state.busy, onClick = controller::beginDeviceLink)
        TvMessage(state.message)
    }
}

@Composable
private fun DeviceLinkPage(state: TvAppState, controller: TvAppController) {
    val challenge = state.deviceLink
    TvPage {
        Text(stringResource(R.string.tv_link_heading), fontSize = 42.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        Spacer(Modifier.height(18.dp))
        Text(stringResource(R.string.tv_link_description), fontSize = 24.sp, color = TvSecondary)
        Spacer(Modifier.height(26.dp))
        Text(challenge?.verificationUri ?: stringResource(R.string.tv_waiting), fontSize = 22.sp, color = TvText)
        Spacer(Modifier.height(16.dp))
        Text(challenge?.userCode.orEmpty(), fontSize = 44.sp, fontWeight = FontWeight.Bold, color = TvAccent)
        Spacer(Modifier.height(18.dp))
        Text(stringResource(R.string.tv_link_waiting), fontSize = 20.sp, color = TvSecondary)
        TvMessage(state.message)
        Spacer(Modifier.height(28.dp))
        TvAction(stringResource(R.string.tv_cancel), onClick = controller::changeServer)
    }
}

@Composable
private fun SignedInPage(state: TvAppState, controller: TvAppController) {
    when (state.route) {
        TvRoute.Home -> HomePage(state, controller)
        TvRoute.Browse -> BrowsePage(state, controller)
        TvRoute.Detail -> DetailPage(state, controller)
        TvRoute.Search -> SearchPage(state, controller)
        TvRoute.Settings -> SettingsPage(state, controller)
        TvRoute.Profiles -> ProfilePickerPage(state, controller, allowBack = true)
        TvRoute.Player -> PlayerPage(state, controller)
    }
}

@Composable
private fun HomePage(state: TvAppState, controller: TvAppController) {
    LazyColumn(
        modifier = Modifier.fillMaxSize().background(TvBackground),
        contentPadding = PaddingValues(horizontal = 48.dp, vertical = 30.dp),
        verticalArrangement = Arrangement.spacedBy(28.dp),
    ) {
        item { TvNavigation(state, controller) }
        when (val home = state.home) {
            TvHomeLoadState.Loading -> item { TvStatusPage(stringResource(R.string.tv_loading_home), compact = true) }
            is TvHomeLoadState.Failure -> item { TvErrorPanel(home.title, controller::goHome) }
            TvHomeLoadState.SessionExpired -> item { TvErrorPanel(stringResource(R.string.tv_sign_in_again), controller::changeServer) }
            is TvHomeLoadState.Ready -> {
                if (home.stale) {
                    item { Text(stringResource(R.string.tv_showing_cached_home), fontSize = 18.sp, color = TvSecondary) }
                }
                items(home.surface.sections, key = TvSection::section_type) { section ->
                    TvSurfaceRow(section, controller)
                }
            }
        }
        item { TvMessage(state.message) }
    }
}

@Composable
private fun TvSurfaceRow(section: TvSection, controller: TvAppController) {
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Text(section.title, fontSize = 28.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        if (section.items.isEmpty()) {
            Text(section.empty_reason?.replace('_', ' ') ?: stringResource(R.string.tv_nothing_here), fontSize = 21.sp, color = TvSecondary)
        } else {
            LazyRow(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                items(section.items, key = TvSurfaceItem::platform_content_id) { item ->
                    TvMediaCard(
                        title = item.title,
                        subtitle = item.subtitle,
                        progress = item.progress_percent,
                        availability = item.availability,
                        onClick = { controller.showDetail(item) },
                    )
                }
            }
        }
    }
}

@Composable
private fun BrowsePage(state: TvAppState, controller: TvAppController) {
    LazyColumn(
        modifier = Modifier.fillMaxSize().background(TvBackground),
        contentPadding = PaddingValues(horizontal = 48.dp, vertical = 30.dp),
        verticalArrangement = Arrangement.spacedBy(26.dp),
    ) {
        item { TvNavigation(state, controller) }
        if (state.busy && state.libraries.isEmpty() && state.collections.isEmpty()) {
            item { TvStatusPage(stringResource(R.string.tv_loading_browse), compact = true) }
        } else {
            item {
                Text(state.browseTitle ?: stringResource(R.string.tv_browse_heading), fontSize = 34.sp, fontWeight = FontWeight.SemiBold, color = TvText)
            }
            if (state.browseTitle == null) {
                item {
                    TvBrowseChoices(
                        stringResource(R.string.tv_libraries),
                        state.libraries.map { it.name },
                        state.libraries.map { library -> { controller.openLibrary(library) } },
                    )
                }
                item {
                    TvBrowseChoices(
                        stringResource(R.string.tv_collections),
                        state.collections.map { it.name },
                        state.collections.map { collection -> { controller.openCollection(collection) } },
                    )
                }
            } else if (state.browseItems.isEmpty()) {
                item { Text(stringResource(R.string.tv_nothing_here), fontSize = 21.sp, color = TvSecondary) }
            } else {
                items(state.browseItems, key = TvMediaItem::id) { item ->
                    TvResultRow(item, controller::showDetail)
                }
            }
        }
        item { TvMessage(state.message) }
    }
}

@Composable
private fun TvBrowseChoices(title: String, labels: List<String>, actions: List<() -> Unit>) {
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Text(title, fontSize = 28.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        if (labels.isEmpty()) {
            Text(stringResource(R.string.tv_nothing_here), fontSize = 21.sp, color = TvSecondary)
        } else {
            LazyRow(horizontalArrangement = Arrangement.spacedBy(18.dp)) {
                itemsIndexed(labels) { index, label ->
                    TvAction(label, onClick = actions[index])
                }
            }
        }
    }
}

@Composable
private fun SearchPage(state: TvAppState, controller: TvAppController) {
    LazyColumn(
        modifier = Modifier.fillMaxSize().background(TvBackground),
        contentPadding = PaddingValues(horizontal = 48.dp, vertical = 30.dp),
        verticalArrangement = Arrangement.spacedBy(22.dp),
    ) {
        item { TvNavigation(state, controller) }
        item {
            Text(stringResource(R.string.tv_search_heading), fontSize = 34.sp, fontWeight = FontWeight.SemiBold, color = TvText)
            Spacer(Modifier.height(16.dp))
            TvTextInput(
                value = state.searchQuery,
                placeholder = stringResource(R.string.tv_search_placeholder),
                onValueChange = controller::updateSearchQuery,
            )
            Spacer(Modifier.height(16.dp))
            TvAction(stringResource(R.string.tv_search_action), enabled = !state.busy, onClick = controller::search)
        }
        val results = state.searchResult?.items.orEmpty()
        if (state.searchResult != null && results.isEmpty()) {
            item { Text(stringResource(R.string.tv_no_results), fontSize = 21.sp, color = TvSecondary) }
        }
        items(results, key = TvMediaItem::id) { item ->
            TvResultRow(item, controller::showDetail)
        }
        item { TvMessage(state.message) }
    }
}

@Composable
private fun DetailPage(state: TvAppState, controller: TvAppController) {
    val detail = state.detail
    TvPage {
        TvNavigation(state, controller)
        Spacer(Modifier.height(36.dp))
        if (detail == null) {
            Text(stringResource(R.string.tv_nothing_here), fontSize = 22.sp, color = TvSecondary)
            return@TvPage
        }
        Text(detail.title, fontSize = 42.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        detail.subtitle?.let { Text(it, fontSize = 24.sp, color = TvSecondary) }
        Spacer(Modifier.height(20.dp))
        Text(detail.availability.replace('_', ' '), fontSize = 20.sp, color = if (detail.availability == "playable") TvSuccess else TvError)
        detail.description?.let {
            Spacer(Modifier.height(20.dp))
            Text(it, fontSize = 23.sp, color = TvText)
        }
        Spacer(Modifier.height(30.dp))
        TvAction(stringResource(R.string.tv_play), enabled = !state.busy, onClick = controller::startPlayback)
        Spacer(Modifier.height(14.dp))
        if (state.audioTracks.isNotEmpty()) {
            TvAction(
                stringResource(
                    R.string.tv_audio_track,
                    state.audioTracks.find { it.index == state.selectedAudioTrackIndex }?.label
                        ?: stringResource(R.string.tv_track_default),
                ),
                enabled = !state.busy,
                onClick = controller::cycleAudioTrack,
            )
            Spacer(Modifier.height(14.dp))
        }
        if (state.subtitleTracks.isNotEmpty()) {
            TvAction(
                stringResource(
                    R.string.tv_subtitle_track,
                    state.subtitleTracks.find { it.index == state.selectedSubtitleTrackIndex }?.label
                        ?: stringResource(R.string.tv_captions_off),
                ),
                enabled = !state.busy,
                onClick = controller::cycleSubtitleTrack,
            )
            Spacer(Modifier.height(14.dp))
        }
        TvAction(
            stringResource(R.string.tv_quality_mode, state.qualityMode.replaceFirstChar(Char::titlecase)),
            enabled = !state.busy,
            onClick = controller::cycleQualityMode,
        )
        Spacer(Modifier.height(14.dp))
        TvAction(stringResource(R.string.tv_check_availability), enabled = !state.busy, onClick = controller::preparePlayback)
        when (val readiness = state.prePlayback) {
            is ApiResult.Success -> Text(
                stringResource(R.string.tv_ready_to_play, readiness.value.availability.replace('_', ' ')),
                fontSize = 21.sp,
                color = TvSuccess,
            )
            is ApiResult.Failure -> Text(readiness.problem.title ?: stringResource(R.string.tv_unavailable), fontSize = 21.sp, color = TvError)
            ApiResult.NetworkFailure -> Text(stringResource(R.string.tv_server_unreachable), fontSize = 21.sp, color = TvError)
            else -> Unit
        }
        TvMessage(state.message)
    }
}

@Composable
private fun PlayerPage(state: TvAppState, controller: TvAppController) {
    var playerView: PlayerView? by remember { mutableStateOf(null) }
    val playbackUi by TvPlaybackService.playbackUi.collectAsState()
    val activeSegment = state.segments.firstOrNull { segment ->
        playbackUi.positionMs >= segment.start_ms && playbackUi.positionMs < segment.end_ms
    }
    BackHandler(onBack = controller::exitPlayback)
    DisposableEffect(Unit) {
        onDispose {
            playerView?.let(TvPlaybackService::detach)
        }
    }
    Box(modifier = Modifier.fillMaxSize().background(Color.Black)) {
        AndroidView(
            factory = { context ->
                PlayerView(context).also { view ->
                    playerView = view
                    TvPlaybackService.attach(view)
                }
            },
            update = TvPlaybackService::attach,
            modifier = Modifier.fillMaxSize(),
        )
        state.detail?.title?.let { title ->
            Text(
                title,
                modifier = Modifier.align(Alignment.TopStart).padding(36.dp),
                fontSize = 23.sp,
                color = TvText,
            )
        }
        state.playback?.let { playback ->
            Text(
                stringResource(
                    R.string.tv_playback_status,
                    "${playback.stream_decision.replace('_', ' ')} · ${state.qualityMode}",
                ),
                modifier = Modifier.align(Alignment.TopEnd).padding(36.dp),
                fontSize = 18.sp,
                color = TvSecondary,
            )
        }
        activeSegment?.let { segment ->
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(36.dp),
            ) {
                TvAction(
                    stringResource(
                        R.string.tv_skip_segment,
                        segment.segment_type.replaceFirstChar(Char::titlecase),
                    ),
                    compact = true,
                    onClick = { controller.skipSegment(segment) },
                )
            }
        }
        playbackUi.errorCode?.let { errorCode ->
            Column(
                modifier = Modifier
                    .align(Alignment.Center)
                    .width(420.dp)
                    .padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(18.dp),
            ) {
                Text(stringResource(R.string.tv_playback_error, errorCode), fontSize = 23.sp, color = TvError)
                TvAction(stringResource(R.string.tv_return_to_details), compact = true, onClick = controller::exitPlayback)
            }
        }
    }
}

@Composable
private fun SettingsPage(state: TvAppState, controller: TvAppController) {
    TvPage {
        TvNavigation(state, controller)
        Spacer(Modifier.height(36.dp))
        Text(stringResource(R.string.tv_settings_heading), fontSize = 34.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        Spacer(Modifier.height(22.dp))
        Text(stringResource(R.string.tv_current_server, state.originInput), fontSize = 21.sp, color = TvSecondary)
        Spacer(Modifier.height(16.dp))
        TvAction(stringResource(R.string.tv_change_server), onClick = controller::changeServer)
        Spacer(Modifier.height(30.dp))
        val settings = state.tvSettings
        if (settings == null && state.busy) {
            Text(stringResource(R.string.tv_loading_settings), fontSize = 21.sp, color = TvSecondary)
        } else if (settings != null) {
            Text(stringResource(R.string.tv_home_publication), fontSize = 26.sp, fontWeight = FontWeight.SemiBold, color = TvText)
            Spacer(Modifier.height(12.dp))
            Text(
                if (settings.tv_publication_enabled) stringResource(R.string.tv_publication_enabled) else stringResource(R.string.tv_publication_disabled),
                fontSize = 21.sp,
                color = TvSecondary,
            )
            Spacer(Modifier.height(16.dp))
            TvAction(
                if (settings.tv_publication_enabled) stringResource(R.string.tv_disable_publication) else stringResource(R.string.tv_enable_publication),
                enabled = !state.busy,
                onClick = { controller.setTvPublication(!settings.tv_publication_enabled) },
            )
        }
        Spacer(Modifier.height(30.dp))
        TvAction(stringResource(R.string.tv_sign_out), enabled = !state.busy, onClick = { controller.logout() })
        TvMessage(state.message)
    }
}

@Composable
private fun ProfilePickerPage(state: TvAppState, controller: TvAppController, allowBack: Boolean) {
    val profiles = state.profiles
    TvPage {
        if (allowBack) {
            TvNavigation(state, controller)
            Spacer(Modifier.height(36.dp))
        }
        Text(stringResource(R.string.tv_choose_profile), fontSize = 38.sp, fontWeight = FontWeight.SemiBold, color = TvText)
        Spacer(Modifier.height(12.dp))
        Text(stringResource(R.string.tv_choose_profile_description), fontSize = 22.sp, color = TvSecondary)
        Spacer(Modifier.height(24.dp))
        TvAction(
            if (state.rememberProfile) stringResource(R.string.tv_remember_profile_on) else stringResource(R.string.tv_remember_profile_off),
            enabled = !state.busy,
            onClick = { controller.setRememberProfile(!state.rememberProfile) },
        )
        Spacer(Modifier.height(24.dp))
        if (profiles?.parent_unlock_required == true) {
            Text(stringResource(R.string.tv_parent_access), fontSize = 25.sp, fontWeight = FontWeight.SemiBold, color = TvText)
            Spacer(Modifier.height(10.dp))
            TvTextInput(
                value = state.parentPin,
                placeholder = stringResource(R.string.tv_parent_pin),
                onValueChange = controller::updateParentPin,
                keyboardType = KeyboardType.NumberPassword,
            )
            Spacer(Modifier.height(14.dp))
            TvAction(stringResource(R.string.tv_unlock_parent_access), enabled = !state.busy, onClick = controller::unlockParentProfile)
            Spacer(Modifier.height(24.dp))
        }
        profiles?.items?.forEach { profile ->
            TvProfileAction(profile, profiles.active_profile_id == profile.id, state.busy) { controller.selectProfile(profile.id) }
            Spacer(Modifier.height(14.dp))
        }
        TvMessage(state.message)
    }
}

@Composable
private fun TvProfileAction(profile: ProfileSummary, active: Boolean, busy: Boolean, onClick: () -> Unit) {
    TvAction(
        label = if (active) stringResource(R.string.tv_active_profile, profile.name) else profile.name,
        supporting = "${profile.profile_type} · ${profile.max_content_rating}",
        enabled = !busy,
        onClick = onClick,
    )
}

@Composable
private fun TvNavigation(state: TvAppState, controller: TvAppController) {
    Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        TvAction(stringResource(R.string.tv_home), compact = true, onClick = controller::goHome)
        TvAction(stringResource(R.string.tv_browse), compact = true, onClick = controller::openBrowse)
        TvAction(stringResource(R.string.tv_search), compact = true, onClick = controller::openSearch)
        TvAction(stringResource(R.string.tv_profiles), compact = true, onClick = controller::openProfiles)
        TvAction(stringResource(R.string.tv_settings), compact = true, onClick = controller::openSettings)
    }
}

@Composable
private fun TvResultRow(item: TvMediaItem, onClick: (TvMediaItem) -> Unit) {
    TvAction(
        label = item.title,
        supporting = listOfNotNull(item.type, item.content_rating).joinToString(" · "),
        onClick = { onClick(item) },
    )
}

@Composable
private fun TvMediaCard(title: String, subtitle: String?, progress: Double?, availability: String, onClick: () -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val cardShape = RoundedCornerShape(10.dp)
    Column(
        modifier = Modifier
            .width(260.dp)
            .clip(cardShape)
            .background(if (focused) TvHover else TvSurface)
            .border(if (focused) 3.dp else 1.dp, if (focused) TvAccent else TvElevated, cardShape)
            .onFocusChanged { focused = it.isFocused }
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onClick)
            .semantics { contentDescription = listOfNotNull(title, subtitle, availability.replace('_', ' ')).joinToString(", ") }
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(128.dp)
                .clip(RoundedCornerShape(6.dp))
                .background(Brush.linearGradient(listOf(Color(0xFF303747), Color(0xFF171A22)))),
        )
        Text(title, fontSize = 20.sp, fontWeight = FontWeight.SemiBold, color = TvText, maxLines = 1, overflow = TextOverflow.Ellipsis)
        subtitle?.let { Text(it, fontSize = 16.sp, color = TvSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis) }
        if (progress != null && progress > 0.0) {
            Text("${progress.toInt()}%", fontSize = 15.sp, color = TvAccent)
        }
    }
}

@Composable
private fun TvAction(
    label: String,
    supporting: String? = null,
    enabled: Boolean = true,
    compact: Boolean = false,
    onClick: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(8.dp)
    Column(
        modifier = Modifier
            .then(if (compact) Modifier else Modifier.fillMaxWidth())
            .clip(shape)
            .background(if (focused && enabled) TvHover else TvElevated)
            .border(if (focused && enabled) 3.dp else 1.dp, if (focused && enabled) TvAccent else TvSurface, shape)
            .onFocusChanged { focused = it.isFocused }
            .clickable(
                enabled = enabled,
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
            )
            .semantics { contentDescription = listOfNotNull(label, supporting).joinToString(", ") }
            .padding(horizontal = if (compact) 18.dp else 22.dp, vertical = if (compact) 11.dp else 18.dp),
    ) {
        Text(label, fontSize = if (compact) 18.sp else 22.sp, fontWeight = FontWeight.SemiBold, color = if (enabled) TvText else TvSecondary)
        supporting?.takeIf { it.isNotBlank() }?.let { Text(it, fontSize = 17.sp, color = TvSecondary) }
    }
}

@Composable
private fun TvTextInput(value: String, placeholder: String, onValueChange: (String) -> Unit, keyboardType: KeyboardType = KeyboardType.Text) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(8.dp)
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
        textStyle = androidx.compose.ui.text.TextStyle(color = TvText, fontSize = 22.sp),
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(TvSurface)
            .border(if (focused) 3.dp else 1.dp, if (focused) TvAccent else TvElevated, shape)
            .onFocusChanged { focused = it.isFocused }
            .semantics { contentDescription = placeholder }
            .padding(18.dp),
        decorationBox = { innerTextField ->
            if (value.isEmpty()) {
                Text(placeholder, fontSize = 22.sp, color = TvSecondary)
            }
            innerTextField()
        },
    )
}

@Composable
private fun TvPage(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(TvBackground)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 48.dp, vertical = 30.dp),
        verticalArrangement = Arrangement.spacedBy(0.dp),
        content = content,
    )
}

@Composable
private fun TvStatusPage(message: String, compact: Boolean = false) {
    Box(
        modifier = if (compact) Modifier.fillMaxWidth().background(TvBackground).padding(vertical = 34.dp) else Modifier.fillMaxSize().background(TvBackground),
        contentAlignment = Alignment.Center,
    ) {
        Text(message, fontSize = if (compact) 24.sp else 30.sp, color = TvSecondary)
    }
}

@Composable
private fun TvErrorPanel(message: String, onRetry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Text(message, fontSize = 24.sp, color = TvError)
        TvAction(stringResource(R.string.tv_retry), onClick = onRetry)
    }
}

@Composable
private fun TvMessage(message: String?) {
    message?.let {
        Spacer(Modifier.height(20.dp))
        Text(it, fontSize = 19.sp, color = TvSecondary)
    }
}
