package com.duskcue.tv.session

import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.AuthenticatedUser
import com.duskcue.tv.api.DeviceCodeRequest
import com.duskcue.tv.api.DeviceCodeResponse
import com.duskcue.tv.api.DeviceTokenResponse
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.ParentUnlockResponse
import com.duskcue.tv.api.ProblemDetails
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.SwitchProfileResponse
import com.duskcue.tv.api.TvSessionApi
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TvAuthenticationServiceTest {
    @Test
    fun starts_device_linking_with_the_persisted_installation_id() = runBlocking {
        val store = MemorySessionStore(SecureTvState(device_id = "tv-installation-id"))
        val api = FakeSessionApi()
        val service = service(store, api)

        val result = service.beginDeviceLink(ServerOrigin.parse("https://duskcue.example").getOrThrow())

        val success = result as ApiResult.Success<DeviceLinkChallenge>
        assertEquals("USER-CODE", success.value.userCode)
        assertEquals("tv-installation-id", requireNotNull(api.deviceCodeRequest).device_id)
        assertEquals("android_tv", requireNotNull(api.deviceCodeRequest).client_platform)
    }

    @Test
    fun honors_the_server_slow_down_interval_from_the_auth_fixture() = runBlocking {
        val fixture = fixtureJson.parseToJsonElement(fixture("auth-flow-matrix.json"))
        val expected = fixture.jsonObject.getValue("flows").toString()
        assertTrue(expected.contains("AUTH_024"))
        val api = FakeSessionApi().apply {
            tokenResult = ApiResult.Failure(
                problem = ProblemDetails(title = "AUTH_024", status = 429),
                status = 429,
                retryAfterSeconds = 12,
            )
        }
        val service = service(MemorySessionStore(SecureTvState(device_id = "device")), api)
        val challenge = (service.beginDeviceLink(ServerOrigin.parse("https://duskcue.example").getOrThrow()) as ApiResult.Success).value

        val result = service.pollDeviceLink(challenge)

        assertEquals(DeviceLinkPollResult.Pending(12), result)
    }

    @Test
    fun successful_device_linking_persists_the_new_session_and_logout_clears_it() = runBlocking {
        val store = MemorySessionStore(SecureTvState(device_id = "device"))
        val api = FakeSessionApi()
        val tokenProvider = MutableBearerTokenProvider()
        val cleaner = RecordingCleaner()
        val coordinator = TvSessionCoordinator(store, tokenProvider, cleaner)
        val service = TvAuthenticationService(store, coordinator, tokenProvider) { _, _ -> api }
        val challenge = (service.beginDeviceLink(ServerOrigin.parse("https://duskcue.example").getOrThrow()) as ApiResult.Success).value

        val poll = service.pollDeviceLink(challenge)
        val logout = service.logout(allSessions = true)

        assertTrue(poll is DeviceLinkPollResult.Authorized)
        assertTrue(logout is ApiResult.Success)
        assertTrue(api.logoutAll)
        assertNull(store.value.session)
        assertNull(tokenProvider.currentToken())
        assertEquals(1, cleaner.identityClears)
    }

    @Test
    fun session_kicked_uses_the_same_local_identity_cleanup_as_logout() = runBlocking {
        val store = MemorySessionStore(
            SecureTvState(
                device_id = "device",
                session = PersistedAccountSession(
                    origin = "https://duskcue.example:48027",
                    token = "token",
                    user_id = "user",
                    username = "user",
                    display_name = "User",
                    role = "user",
                    active_profile_id = "profile",
                    profile_selection_required = false,
                ),
            ),
        )
        val tokenProvider = MutableBearerTokenProvider("token")
        val cleaner = RecordingCleaner()
        val service = TvAuthenticationService(
            store = store,
            coordinator = TvSessionCoordinator(store, tokenProvider, cleaner),
            tokenProvider = tokenProvider,
            apiFor = { _, _ -> FakeSessionApi() },
        )

        service.handleSessionKicked()

        assertEquals(1, cleaner.identityClears)
        assertNull(store.value.session)
        assertNull(tokenProvider.currentToken())
    }

    private fun service(store: MemorySessionStore, api: FakeSessionApi): TvAuthenticationService {
        val tokenProvider = MutableBearerTokenProvider()
        return TvAuthenticationService(
            store = store,
            coordinator = TvSessionCoordinator(store, tokenProvider, RecordingCleaner()),
            tokenProvider = tokenProvider,
            apiFor = { _, _ -> api },
        )
    }

    private fun fixture(name: String): String = requireNotNull(javaClass.classLoader?.getResource(name))
        .openStream()
        .bufferedReader()
        .use { it.readText() }

    private class FakeSessionApi : TvSessionApi {
        var deviceCodeRequest: DeviceCodeRequest? = null
        var tokenResult: ApiResult<DeviceTokenResponse> = ApiResult.Success(
            DeviceTokenResponse("session-token", user()),
            null,
        )
        var logoutAll = false

        override fun requestDeviceCode(request: DeviceCodeRequest): ApiResult<DeviceCodeResponse> {
            deviceCodeRequest = request
            return ApiResult.Success(
                DeviceCodeResponse(
                    device_code = "device-code",
                    user_code = "USER-CODE",
                    verification_uri = "https://duskcue.example/auth/link",
                    expires_in = 600,
                    interval = 5,
                ),
                null,
            )
        }

        override fun pollDeviceToken(deviceCode: String): ApiResult<DeviceTokenResponse> = tokenResult

        override fun listProfiles(): ApiResult<ProfileListResponse> = ApiResult.NetworkFailure

        override fun switchProfile(profileId: String, rememberOnDevice: Boolean): ApiResult<SwitchProfileResponse> = ApiResult.NetworkFailure

        override fun unlockParentProfile(pin: String): ApiResult<ParentUnlockResponse> = ApiResult.NetworkFailure

        override fun logout(allSessions: Boolean): ApiResult<Unit> {
            logoutAll = allSessions
            return ApiResult.Success(Unit, null)
        }
    }

    private class MemorySessionStore(initial: SecureTvState) : TvSessionStore {
        var value = initial

        override suspend fun current(): SecureTvState = value

        override suspend fun replace(value: SecureTvState) {
            this.value = value
        }
    }

    private class RecordingCleaner : TvLocalStateCleaner {
        var identityClears = 0

        override suspend fun clearProfileScope() = Unit

        override suspend fun clearIdentityScope() {
            identityClears += 1
        }
    }

    private companion object {
        val fixtureJson = Json { ignoreUnknownKeys = true }

        fun user(): AuthenticatedUser = AuthenticatedUser(
            id = "user-id",
            username = "user",
            display_name = "User",
            role = "user",
            capabilities = emptyList(),
            has_all_library_access = true,
            active_profile_id = "profile-id",
            profile_selection_required = false,
        )
    }
}
