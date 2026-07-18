package com.duskcue.tv.api

import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class ProblemDetails(
    val type: String? = null,
    val title: String? = null,
    val status: Int? = null,
    val detail: String? = null,
    val instance: String? = null,
    val trace_id: String? = null,
)

@Serializable
data class DeviceCodeRequest(
    val device_id: String,
    val client_name: String,
    val client_platform: String,
    val client_version: String,
)

@Serializable
data class DeviceCodeResponse(
    val device_code: String,
    val user_code: String,
    val verification_uri: String,
    val verification_uri_complete: String? = null,
    val expires_in: Int,
    val interval: Int,
)

@Serializable
data class DeviceTokenRequest(val device_code: String)

@Serializable
data class AuthenticatedUser(
    val id: String,
    val username: String,
    val display_name: String,
    val role: String,
    val capabilities: List<String>,
    val has_all_library_access: Boolean,
    val active_profile_id: String,
    val profile_selection_required: Boolean,
)

@Serializable
data class DeviceTokenResponse(
    val session_token: String,
    val user: AuthenticatedUser,
)

@Serializable
data class ProfileSummary(
    val id: String,
    val name: String,
    val profile_type: String,
    val is_default: Boolean,
    val max_content_rating: String,
    val allow_search: Boolean,
    val allow_downloads: Boolean,
    val allow_external_links: Boolean,
    val allow_ambient_channels: Boolean,
    val parent_pin_configured: Boolean,
)

@Serializable
data class ProfileListResponse(
    val active_profile_id: String,
    val profile_selection_required: Boolean,
    val remembered_profile_id: String? = null,
    val device_can_remember_profile: Boolean,
    val parent_unlock_required: Boolean,
    val parent_unlock_expires_at: String? = null,
    val items: List<ProfileSummary>,
)

@Serializable
data class SwitchProfileRequest(val remember_on_device: Boolean)

@Serializable
data class SwitchProfileResponse(
    val active_profile: ProfileSummary,
    val profile_selection_required: Boolean,
    val remembered_profile_id: String? = null,
    val device_can_remember_profile: Boolean,
    val parent_unlock_required: Boolean,
    val parent_unlock_expires_at: String? = null,
)

@Serializable
data class ParentUnlockRequest(val pin: String)

@Serializable
data class ParentUnlockResponse(val unlocked_until: String)

@Serializable
data class TvSurface(
    val generated_at: String,
    val platform: String,
    val limit: Int,
    val sections: List<TvSection>,
)

@Serializable
data class TvSection(
    val section_type: String,
    val title: String,
    val empty_reason: String? = null,
    val items: List<TvSurfaceItem>,
)

@Serializable
data class TvSurfaceItem(
    val surface_item_id: String,
    val platform_content_id: String,
    val media_item_id: String,
    val media_type: String,
    val section_type: String,
    val title: String,
    val subtitle: String? = null,
    val description: String? = null,
    val season_number: Int? = null,
    val episode_number: Int? = null,
    val duration_ms: Long? = null,
    val resume_position_ms: Long? = null,
    val progress_percent: Double? = null,
    val last_engaged_at: String? = null,
    val poster_url: String? = null,
    val backdrop_url: String? = null,
    val deep_link: String? = null,
    val web_url: String? = null,
    val availability: String,
    val availability_detail: String? = null,
)

@Serializable
data class TvMediaItem(
    val id: String,
    val library_id: String,
    val type: String,
    val title: String,
    val sort_title: String? = null,
    val overview: String? = null,
    val premiere_date: String? = null,
    val content_rating: String? = null,
    val runtime_seconds: Int? = null,
    val rating_average: Double? = null,
    val series_id: String? = null,
    val season_number: Int? = null,
    val episode_number: Int? = null,
)

@Serializable
data class TvMediaItemPage(
    val items: List<TvMediaItem>,
    val cursor: String? = null,
    val has_more: Boolean = false,
)

@Serializable
data class TvSearchResponse(
    val items: List<TvMediaItem>,
)

@Serializable
data class TvLibrary(
    val id: String,
    val name: String,
    val media_type: String,
    val item_count: Long = 0,
)

@Serializable
data class TvLibraryListResponse(
    val items: List<TvLibrary>,
    val total: Long = 0,
)

@Serializable
data class TvCollection(
    val id: String,
    val name: String,
    val description: String? = null,
    val item_count: Int = 0,
)

@Serializable
data class TvCollectionListResponse(
    val items: List<TvCollection>,
    val total: Long = 0,
)

@Serializable
data class TvSurfaceSettings(
    val tv_publication_enabled: Boolean,
    val enabled_platforms: List<String>,
    val publish_continue_watching: Boolean,
    val publish_next_up: Boolean,
    val publish_new_episodes: Boolean,
    val publish_recommendations: Boolean,
)

@Serializable
data class UpdateTvSurfaceSettingsRequest(
    val tv_publication_enabled: Boolean? = null,
    val enabled_platforms: List<String>? = null,
    val publish_continue_watching: Boolean? = null,
    val publish_next_up: Boolean? = null,
    val publish_new_episodes: Boolean? = null,
    val publish_recommendations: Boolean? = null,
)

@Serializable
data class TvResolveResponse(
    val platform_content_id: String,
    val media_item_id: String,
    val media_type: String,
    val resume_position_ms: Long,
    val availability: String,
    val playback_action: String,
    val requires_auth: Boolean,
    val access_revalidated: Boolean,
)

sealed interface ApiResult<out T> {
    data class Success<T>(val value: T, val etag: String?) : ApiResult<T>
    data object NotModified : ApiResult<Nothing>
    data class Failure(
        val problem: ProblemDetails,
        val status: Int,
        val retryAfterSeconds: Int? = null,
    ) : ApiResult<Nothing>
    data object NetworkFailure : ApiResult<Nothing>
}

interface TvSessionApi {
    fun requestDeviceCode(request: DeviceCodeRequest): ApiResult<DeviceCodeResponse>
    fun pollDeviceToken(deviceCode: String): ApiResult<DeviceTokenResponse>
    fun listProfiles(): ApiResult<ProfileListResponse>
    fun switchProfile(profileId: String, rememberOnDevice: Boolean): ApiResult<SwitchProfileResponse>
    fun unlockParentProfile(pin: String): ApiResult<ParentUnlockResponse>
    fun logout(allSessions: Boolean = false): ApiResult<Unit>
}

class DuskcueApiClient(
    private val origin: ServerOrigin,
    private val transport: HttpTransport,
    private val tokenProvider: BearerTokenProvider,
    private val etagStore: EtagStore,
    private val json: Json = Json { ignoreUnknownKeys = true },
) : TvSessionApi {
    fun tvSurface(
        limit: Int,
        cacheScope: String,
        platform: TvPlatform = TvPlatform.AndroidTv,
    ): ApiResult<TvSurface> {
        require(limit in 1..100)
        val cacheKey = "$cacheScope:tv-surface:${platform.apiValue}:$limit"
        val response = execute(
            request(
                path = "/api/v1/users/me/tv-surface?platform=${platform.apiValue}&limit=$limit",
                etag = etagStore.read(cacheKey),
            ),
        ) ?: return ApiResult.NetworkFailure
        if (response.status == HttpURLConnection.HTTP_NOT_MODIFIED) {
            return ApiResult.NotModified
        }
        if (response.status !in 200..299) {
            return failure(response)
        }
        val etag = response.headers.header("etag")
        if (etag != null) {
            etagStore.write(cacheKey, etag)
        }
        return ApiResult.Success(json.decodeFromString<TvSurface>(response.body), etag)
    }

    fun resolveTvItem(
        platformContentId: String,
        platform: TvPlatform = TvPlatform.AndroidTv,
    ): ApiResult<TvResolveResponse> {
        val encodedId = java.net.URLEncoder.encode(platformContentId, "UTF-8")
        val response = execute(request(path = "/api/v1/tv/resolve/$encodedId?platform=${platform.apiValue}"))
            ?: return ApiResult.NetworkFailure
        if (response.status !in 200..299) {
            return failure(response)
        }
        return ApiResult.Success(json.decodeFromString<TvResolveResponse>(response.body), null)
    }

    fun libraries(): ApiResult<TvLibraryListResponse> = executeJson(
        request(path = "/api/v1/libraries"),
    )

    fun libraryItems(libraryId: String, page: CursorRequest = CursorRequest(limit = 24)): ApiResult<TvMediaItemPage> =
        executeJson(request(path = "/api/v1/libraries/${pathSegment(libraryId)}/items?${page.queryString()}"))

    fun collections(): ApiResult<TvCollectionListResponse> = executeJson(
        request(path = "/api/v1/collections?limit=24"),
    )

    fun collectionItems(collectionId: String, page: CursorRequest = CursorRequest(limit = 24)): ApiResult<TvMediaItemPage> =
        executeJson(request(path = "/api/v1/collections/${pathSegment(collectionId)}/items?${page.queryString()}"))

    fun mediaItem(mediaItemId: String): ApiResult<TvMediaItem> = executeJson(
        request(path = "/api/v1/media-items/${pathSegment(mediaItemId)}"),
    )

    fun search(query: String, limit: Int = 24): ApiResult<TvSearchResponse> {
        require(limit in 1..100)
        val normalized = query.trim()
        require(normalized.isNotEmpty())
        return executeJson(
            request(path = "/api/v1/search?q=${URLEncoder.encode(normalized, "UTF-8")}&limit=$limit"),
        )
    }

    fun tvSettings(): ApiResult<TvSurfaceSettings> = executeJson(
        request(path = "/api/v1/tv/settings"),
    )

    fun updateTvSettings(update: UpdateTvSurfaceSettingsRequest): ApiResult<TvSurfaceSettings> = executeJson(
        request(
            method = "PUT",
            path = "/api/v1/tv/settings",
            body = json.encodeToString(UpdateTvSurfaceSettingsRequest.serializer(), update),
        ),
    )

    override fun requestDeviceCode(request: DeviceCodeRequest): ApiResult<DeviceCodeResponse> = executeJson(
        request(
            method = "POST",
            path = "/api/v1/device/code",
            body = json.encodeToString(DeviceCodeRequest.serializer(), request),
            authenticated = false,
        ),
    )

    override fun pollDeviceToken(deviceCode: String): ApiResult<DeviceTokenResponse> = executeJson(
        request(
            method = "POST",
            path = "/api/v1/device/token",
            body = json.encodeToString(DeviceTokenRequest.serializer(), DeviceTokenRequest(deviceCode)),
            authenticated = false,
        ),
    )

    override fun listProfiles(): ApiResult<ProfileListResponse> = executeJson(
        request(path = "/api/v1/profiles"),
    )

    override fun switchProfile(profileId: String, rememberOnDevice: Boolean): ApiResult<SwitchProfileResponse> = executeJson(
        request(
            method = "POST",
            path = "/api/v1/profiles/$profileId/switch",
            body = json.encodeToString(SwitchProfileRequest.serializer(), SwitchProfileRequest(rememberOnDevice)),
        ),
    )

    override fun unlockParentProfile(pin: String): ApiResult<ParentUnlockResponse> = executeJson(
        request(
            method = "POST",
            path = "/api/v1/profiles/parent-unlock",
            body = json.encodeToString(ParentUnlockRequest.serializer(), ParentUnlockRequest(pin)),
        ),
    )

    override fun logout(allSessions: Boolean): ApiResult<Unit> {
        val path = if (allSessions) "/api/v1/auth/logout-all" else "/api/v1/auth/logout"
        val response = execute(request(method = "POST", path = path)) ?: return ApiResult.NetworkFailure
        if (response.status !in 200..299) {
            return failure(response)
        }
        return ApiResult.Success(Unit, null)
    }

    private inline fun <reified T> executeJson(request: ApiRequest): ApiResult<T> {
        val response = execute(request) ?: return ApiResult.NetworkFailure
        if (response.status !in 200..299) {
            return failure(response)
        }
        return ApiResult.Success(json.decodeFromString<T>(response.body), response.headers.header("etag"))
    }

    private fun request(
        method: String = "GET",
        path: String,
        body: String? = null,
        etag: String? = null,
        authenticated: Boolean = true,
    ): ApiRequest {
        val headers = buildMap {
            put("Accept", "application/json")
            if (body != null) {
                put("Content-Type", "application/json; charset=utf-8")
            }
            if (authenticated) {
                tokenProvider.currentToken()?.let { put("Authorization", "Bearer $it") }
            }
            etag?.let { put("If-None-Match", it) }
        }
        return ApiRequest(method = method, path = "${origin.value}$path", headers = headers, body = body)
    }

    private fun pathSegment(value: String): String = URLEncoder.encode(value, "UTF-8")

    private fun failure(response: ApiResponse): ApiResult.Failure {
        val problem = runCatching {
            json.decodeFromString<ProblemDetails>(response.body)
        }.getOrElse {
            ProblemDetails(status = response.status, title = "HTTP_${response.status}")
        }
        return ApiResult.Failure(
            problem = problem,
            status = response.status,
            retryAfterSeconds = response.headers.header("Retry-After")?.toIntOrNull(),
        )
    }

    private fun execute(request: ApiRequest): ApiResponse? = try {
        transport.execute(request)
    } catch (_: java.io.IOException) {
        null
    }
}

class UrlConnectionTransport(
    private val connectTimeoutMs: Int = 10_000,
    private val readTimeoutMs: Int = 20_000,
) : HttpTransport {
    override fun execute(request: ApiRequest): ApiResponse {
        val connection = URL(request.path).openConnection() as HttpURLConnection
        connection.requestMethod = request.method
        connection.connectTimeout = connectTimeoutMs
        connection.readTimeout = readTimeoutMs
        connection.instanceFollowRedirects = false
        request.headers.forEach(connection::setRequestProperty)
        return try {
            request.body?.let { body ->
                connection.doOutput = true
                connection.outputStream.bufferedWriter(Charsets.UTF_8).use { writer -> writer.write(body) }
            }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val body = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            ApiResponse(
                status = status,
                headers = connection.headerFields.mapNotNull { (key, values) ->
                    key?.let { it to values.firstOrNull().orEmpty() }
                }.toMap(),
                body = body,
            )
        } finally {
            connection.disconnect()
        }
    }
}
