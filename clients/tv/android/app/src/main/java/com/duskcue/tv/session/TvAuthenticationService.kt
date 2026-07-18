package com.duskcue.tv.session

import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.DeviceCodeRequest
import com.duskcue.tv.api.DeviceCodeResponse
import com.duskcue.tv.api.DeviceTokenResponse
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.ParentUnlockResponse
import com.duskcue.tv.api.ProfileListResponse
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.SwitchProfileResponse
import com.duskcue.tv.api.TvSessionApi
import com.duskcue.tv.profiles.ProfileGateState

data class DeviceLinkChallenge(
    val origin: ServerOrigin,
    val deviceCode: String,
    val userCode: String,
    val verificationUri: String,
    val verificationUriComplete: String?,
    val expiresInSeconds: Int,
    val pollIntervalSeconds: Int,
)

sealed interface DeviceLinkPollResult {
    data class Authorized(val response: DeviceTokenResponse, val gate: ProfileGateState) : DeviceLinkPollResult
    data class Pending(val nextPollAfterSeconds: Int) : DeviceLinkPollResult
    data class TerminalFailure(val failure: ApiResult.Failure) : DeviceLinkPollResult
    data object NetworkFailure : DeviceLinkPollResult
}

class TvAuthenticationService(
    private val store: TvSessionStore,
    private val coordinator: TvSessionCoordinator,
    private val tokenProvider: MutableBearerTokenProvider,
    private val apiFor: (ServerOrigin, MutableBearerTokenProvider) -> TvSessionApi,
) {
    suspend fun restore(): ApiResult<ProfileListResponse>? {
        val session = coordinator.restoreToken() ?: return null
        val origin = ServerOrigin.parse(session.origin).getOrElse {
            coordinator.clearIdentity()
            return ApiResult.NetworkFailure
        }
        val result = apiFor(origin, tokenProvider).listProfiles()
        if (result is ApiResult.Success) {
            coordinator.applyProfileList(result.value)
        } else if (result is ApiResult.Failure && result.status == 401) {
            coordinator.clearIdentity()
        }
        return result
    }

    suspend fun beginDeviceLink(origin: ServerOrigin): ApiResult<DeviceLinkChallenge> {
        val state = coordinator.selectServer(origin)
        val response = apiFor(origin, tokenProvider).requestDeviceCode(
            DeviceCodeRequest(
                device_id = state.device_id,
                client_name = "Duskcue Android TV",
                client_platform = "android_tv",
                client_version = "0.1.0",
            ),
        )
        return when (response) {
            is ApiResult.Success -> ApiResult.Success(response.value.toChallenge(origin), response.etag)
            ApiResult.NetworkFailure -> ApiResult.NetworkFailure
            ApiResult.NotModified -> ApiResult.NetworkFailure
            is ApiResult.Failure -> response
        }
    }

    suspend fun pollDeviceLink(challenge: DeviceLinkChallenge): DeviceLinkPollResult = when (
        val response = apiFor(challenge.origin, tokenProvider).pollDeviceToken(challenge.deviceCode)
    ) {
        is ApiResult.Success -> DeviceLinkPollResult.Authorized(
            response = response.value,
            gate = coordinator.completeDeviceLink(challenge.origin, response.value),
        )
        ApiResult.NetworkFailure -> DeviceLinkPollResult.NetworkFailure
        ApiResult.NotModified -> DeviceLinkPollResult.NetworkFailure
        is ApiResult.Failure -> when (response.problem.title) {
            "AUTH_023" -> DeviceLinkPollResult.Pending(challenge.pollIntervalSeconds)
            "AUTH_024" -> DeviceLinkPollResult.Pending(
                maxOf(challenge.pollIntervalSeconds + 5, response.retryAfterSeconds ?: 0).coerceAtMost(60),
            )
            else -> DeviceLinkPollResult.TerminalFailure(response)
        }
    }

    suspend fun refreshProfiles(): ApiResult<ProfileListResponse> {
        val session = coordinator.restoreToken() ?: return ApiResult.NetworkFailure
        val origin = ServerOrigin.parse(session.origin).getOrElse { return ApiResult.NetworkFailure }
        val result = apiFor(origin, tokenProvider).listProfiles()
        if (result is ApiResult.Success) {
            coordinator.applyProfileList(result.value)
        } else if (result is ApiResult.Failure && result.status == 401) {
            coordinator.clearIdentity()
        }
        return result
    }

    suspend fun switchProfile(profileId: String, rememberOnDevice: Boolean): ApiResult<SwitchProfileResponse> {
        val session = coordinator.restoreToken() ?: return ApiResult.NetworkFailure
        val origin = ServerOrigin.parse(session.origin).getOrElse { return ApiResult.NetworkFailure }
        val result = apiFor(origin, tokenProvider).switchProfile(profileId, rememberOnDevice)
        if (result is ApiResult.Success) {
            coordinator.applyProfileSwitch(result.value)
        } else if (result is ApiResult.Failure && result.status == 401) {
            coordinator.clearIdentity()
        }
        return result
    }

    suspend fun unlockParentProfile(pin: String): ApiResult<ParentUnlockResponse> {
        val session = coordinator.restoreToken() ?: return ApiResult.NetworkFailure
        val origin = ServerOrigin.parse(session.origin).getOrElse { return ApiResult.NetworkFailure }
        return apiFor(origin, tokenProvider).unlockParentProfile(pin)
    }

    suspend fun logout(allSessions: Boolean): ApiResult<Unit> {
        val session = coordinator.restoreToken()
        val result = if (session == null) {
            ApiResult.Success(Unit, null)
        } else {
            ServerOrigin.parse(session.origin)
                .map { apiFor(it, tokenProvider).logout(allSessions) }
                .getOrElse { ApiResult.NetworkFailure }
        }
        coordinator.clearIdentity()
        return result
    }

    suspend fun handleSessionKicked() {
        coordinator.clearIdentity()
    }

    private fun DeviceCodeResponse.toChallenge(origin: ServerOrigin): DeviceLinkChallenge = DeviceLinkChallenge(
        origin = origin,
        deviceCode = device_code,
        userCode = user_code,
        verificationUri = verification_uri,
        verificationUriComplete = verification_uri_complete,
        expiresInSeconds = expires_in,
        pollIntervalSeconds = interval.coerceIn(1, 60),
    )
}
