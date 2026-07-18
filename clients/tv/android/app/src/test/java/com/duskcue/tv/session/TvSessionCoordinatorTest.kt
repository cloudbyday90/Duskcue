package com.duskcue.tv.session

import com.duskcue.tv.api.AuthenticatedUser
import com.duskcue.tv.api.DeviceTokenResponse
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.ProfileSummary
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.SwitchProfileResponse
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TvSessionCoordinatorTest {
    @Test
    fun device_link_for_a_different_account_clears_identity_before_replacing_the_token() = runBlocking {
        val store = MemorySessionStore(sessionState(userId = "first-user", origin = "https://first.example:48027"))
        val tokenProvider = MutableBearerTokenProvider("first-token")
        val cleaner = RecordingCleaner()
        val coordinator = TvSessionCoordinator(store, tokenProvider, cleaner, nowMillis = { 100 })

        coordinator.completeDeviceLink(
            ServerOrigin.parse("https://second.example").getOrThrow(),
            DeviceTokenResponse(session_token = "second-token", user = user(id = "second-user", profileId = "second-profile")),
        )

        assertEquals(1, cleaner.identityClears)
        assertEquals(0, cleaner.profileClears)
        assertEquals("second-token", tokenProvider.currentToken())
        assertEquals("second-user", requireNotNull(store.value.session).user_id)
        assertEquals("https://second.example:48027", store.value.known_servers.first().origin)
    }

    @Test
    fun a_profile_switch_clears_profile_state_before_the_new_profile_is_saved() = runBlocking {
        val store = MemorySessionStore(sessionState(userId = "user", profileId = "kids"))
        val cleaner = RecordingCleaner()
        val coordinator = TvSessionCoordinator(store, MutableBearerTokenProvider("token"), cleaner)

        val gate = coordinator.applyProfileSwitch(
            SwitchProfileResponse(
                active_profile = profile(id = "standard", type = "standard"),
                profile_selection_required = false,
                device_can_remember_profile = true,
                parent_unlock_required = false,
            ),
        )

        assertEquals(1, cleaner.profileClears)
        assertTrue(!gate.profileSelectionRequired)
        assertEquals("standard", requireNotNull(store.value.session).active_profile_id)
    }

    @Test
    fun a_required_profile_choice_clears_existing_profile_state_and_blocks_loading() = runBlocking {
        val store = MemorySessionStore(sessionState(userId = "user", profileId = "standard"))
        val cleaner = RecordingCleaner()
        val coordinator = TvSessionCoordinator(store, MutableBearerTokenProvider("token"), cleaner)

        val gate = coordinator.applyProfileList(
            ProfileListResponse(
                active_profile_id = "standard",
                profile_selection_required = true,
                device_can_remember_profile = true,
                parent_unlock_required = false,
                items = listOf(profile(id = "standard", type = "standard")),
            ),
        )

        assertEquals(1, cleaner.profileClears)
        assertTrue(gate.profileSelectionRequired)
    }

    @Test
    fun logout_clears_identity_data_but_keeps_the_nonsecret_server_choice() = runBlocking {
        val store = MemorySessionStore(sessionState(userId = "user", profileId = "standard"))
        val cleaner = RecordingCleaner()
        val tokenProvider = MutableBearerTokenProvider("token")
        val coordinator = TvSessionCoordinator(store, tokenProvider, cleaner)

        coordinator.clearIdentity()

        assertEquals(1, cleaner.identityClears)
        assertNull(tokenProvider.currentToken())
        assertNull(store.value.session)
        assertEquals("https://duskcue.example:48027", store.value.known_servers.single().origin)
    }

    private fun sessionState(
        userId: String,
        origin: String = "https://duskcue.example:48027",
        profileId: String = "standard",
    ): SecureTvState = SecureTvState(
        device_id = "device-id",
        known_servers = listOf(SavedServer(origin, 1)),
        session = PersistedAccountSession(
            origin = origin,
            token = "token",
            user_id = userId,
            username = userId,
            display_name = userId,
            role = "user",
            active_profile_id = profileId,
            profile_selection_required = false,
        ),
    )

    private fun user(id: String, profileId: String): AuthenticatedUser = AuthenticatedUser(
        id = id,
        username = id,
        display_name = id,
        role = "user",
        capabilities = emptyList(),
        has_all_library_access = true,
        active_profile_id = profileId,
        profile_selection_required = false,
    )

    private fun profile(id: String, type: String): ProfileSummary = ProfileSummary(
        id = id,
        name = id,
        profile_type = type,
        is_default = true,
        max_content_rating = "TV-MA",
        allow_search = true,
        allow_downloads = true,
        allow_external_links = true,
        allow_ambient_channels = true,
        parent_pin_configured = type == "kids",
    )

    private class MemorySessionStore(initial: SecureTvState) : TvSessionStore {
        var value = initial

        override suspend fun current(): SecureTvState = value

        override suspend fun replace(value: SecureTvState) {
            this.value = value
        }
    }

    private class RecordingCleaner : TvLocalStateCleaner {
        var profileClears = 0
        var identityClears = 0

        override suspend fun clearProfileScope() {
            profileClears += 1
        }

        override suspend fun clearIdentityScope() {
            identityClears += 1
        }
    }
}
