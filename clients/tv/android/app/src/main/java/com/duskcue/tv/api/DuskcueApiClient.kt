package com.duskcue.tv.api

import java.net.HttpURLConnection
import java.net.URL
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
    val duration_ms: Long? = null,
    val resume_position_ms: Long? = null,
    val progress_percent: Double? = null,
    val last_engaged_at: String? = null,
    val poster_url: String? = null,
    val backdrop_url: String? = null,
    val deep_link: String? = null,
    val availability: String,
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
    data class Failure(val problem: ProblemDetails, val status: Int) : ApiResult<Nothing>
}

class DuskcueApiClient(
    private val origin: ServerOrigin,
    private val transport: HttpTransport,
    private val tokenProvider: BearerTokenProvider,
    private val etagStore: EtagStore,
    private val json: Json = Json { ignoreUnknownKeys = true },
) {
    fun tvSurface(limit: Int, cacheScope: String): ApiResult<TvSurface> {
        require(limit in 1..100)
        val cacheKey = "$cacheScope:tv-surface:android_tv:$limit"
        val response = transport.execute(
            request(
                path = "/api/v1/users/me/tv-surface?platform=android_tv&limit=$limit",
                etag = etagStore.read(cacheKey),
            ),
        )
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

    fun resolveTvItem(platformContentId: String): ApiResult<TvResolveResponse> {
        val encodedId = java.net.URLEncoder.encode(platformContentId, "UTF-8")
        val response = transport.execute(
            request(path = "/api/v1/tv/resolve/$encodedId?platform=android_tv"),
        )
        if (response.status !in 200..299) {
            return failure(response)
        }
        return ApiResult.Success(json.decodeFromString<TvResolveResponse>(response.body), null)
    }

    private fun request(path: String, etag: String? = null): ApiRequest {
        val headers = buildMap {
            put("Accept", "application/json")
            tokenProvider.currentToken()?.let { put("Authorization", "Bearer $it") }
            etag?.let { put("If-None-Match", it) }
        }
        return ApiRequest(method = "GET", path = "${origin.value}$path", headers = headers)
    }

    private fun failure(response: ApiResponse): ApiResult.Failure {
        val problem = runCatching {
            json.decodeFromString<ProblemDetails>(response.body)
        }.getOrElse {
            ProblemDetails(status = response.status, title = "HTTP_${response.status}")
        }
        return ApiResult.Failure(problem = problem, status = response.status)
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

private fun Map<String, String>.header(name: String): String? =
    entries.firstOrNull { it.key.equals(name, ignoreCase = true) }?.value
