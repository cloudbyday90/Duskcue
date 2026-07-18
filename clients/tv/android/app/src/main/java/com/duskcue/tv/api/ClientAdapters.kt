package com.duskcue.tv.api

import java.io.IOException
import java.net.URI
import java.net.URLEncoder
import kotlin.math.min
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

enum class TvPlatform(val apiValue: String) {
    AndroidTv("android_tv"),
    FireTv("fire_tv"),
}

data class CursorRequest(
    val limit: Int,
    val cursor: String? = null,
) {
    init {
        require(limit in 1..100)
    }

    fun queryString(): String = buildList {
        add("limit=$limit")
        cursor?.let { add("cursor=${URLEncoder.encode(it, "UTF-8")}") }
    }.joinToString("&")
}

@Serializable
data class CursorPage<T>(
    val items: List<T>,
    val next_cursor: String? = null,
    val total_count: Int? = null,
)

data class RetryPolicy(
    val maxAttempts: Int = 3,
    val initialDelayMs: Long = 250,
    val maxDelayMs: Long = 2_000,
) {
    init {
        require(maxAttempts >= 1)
        require(initialDelayMs >= 0)
        require(maxDelayMs >= initialDelayMs)
    }

    fun retryDelayMs(request: ApiRequest, response: ApiResponse, attempt: Int): Long? {
        if (attempt >= maxAttempts || request.method.uppercase() !in retryableMethods) {
            return null
        }
        if (response.status !in retryableStatusCodes) {
            return null
        }
        val retryAfterMs = response.headers.header("Retry-After")
            ?.toLongOrNull()
            ?.times(1_000)
            ?.coerceAtMost(maxDelayMs)
        return retryAfterMs ?: min(initialDelayMs * (1L shl (attempt - 1)), maxDelayMs)
    }

    private companion object {
        val retryableMethods = setOf("GET", "HEAD", "OPTIONS")
        val retryableStatusCodes = setOf(408, 425, 429, 500, 502, 503, 504)
    }
}

fun interface RetrySleeper {
    fun sleep(delayMs: Long)
}

class RetryingTransport(
    private val delegate: HttpTransport,
    private val policy: RetryPolicy = RetryPolicy(),
    private val sleeper: RetrySleeper = RetrySleeper(Thread::sleep),
) : HttpTransport {
    override fun execute(request: ApiRequest): ApiResponse {
        var attempt = 1
        while (true) {
            val response = try {
                delegate.execute(request)
            } catch (error: IOException) {
                if (attempt >= policy.maxAttempts || request.method.uppercase() !in setOf("GET", "HEAD", "OPTIONS")) {
                    throw error
                }
                sleeper.sleep(min(policy.initialDelayMs * (1L shl (attempt - 1)), policy.maxDelayMs))
                attempt += 1
                continue
            }
            val retryDelayMs = policy.retryDelayMs(request, response, attempt) ?: return response
            sleeper.sleep(retryDelayMs)
            attempt += 1
        }
    }
}

data class ServerSentEvent(
    val id: String? = null,
    val type: String = "message",
    val data: String = "",
)

class ServerSentEventDecoder {
    fun decode(input: String): List<ServerSentEvent> {
        val events = mutableListOf<ServerSentEvent>()
        var id: String? = null
        var type: String? = null
        val data = mutableListOf<String>()

        fun emit() {
            if (id != null || type != null || data.isNotEmpty()) {
                events += ServerSentEvent(id = id, type = type ?: "message", data = data.joinToString("\n"))
            }
            id = null
            type = null
            data.clear()
        }

        input.lineSequence().forEach { line ->
            when {
                line.isEmpty() -> emit()
                line.startsWith(":") -> Unit
                line.startsWith("id:") -> id = line.removePrefix("id:").trimStart()
                line.startsWith("event:") -> type = line.removePrefix("event:").trimStart()
                line.startsWith("data:") -> data += line.removePrefix("data:").trimStart()
            }
        }
        emit()
        return events
    }
}

@Serializable
data class TvSurfaceChangedHint(
    val user_id: String,
    val reason: String,
    val media_item_id: String? = null,
    val changed_sections: List<String> = emptyList(),
    val debounce_until: String? = null,
)

fun ServerSentEvent.tvSurfaceChangedHint(json: Json = Json { ignoreUnknownKeys = true }): TvSurfaceChangedHint? =
    if (type == "tv_surface_changed") {
        json.decodeFromString<TvSurfaceChangedHint>(data)
    } else {
        null
    }

object DiagnosticsRedactor {
    fun redactedHeaders(headers: Map<String, String>): Map<String, String> = headers.filterKeys {
        !it.equals("Authorization", ignoreCase = true) &&
            !it.equals("Cookie", ignoreCase = true) &&
            !it.equals("Set-Cookie", ignoreCase = true)
    }

    fun redactedUrl(value: String): String = runCatching {
        val uri = URI(value)
        URI(uri.scheme, null, uri.host, uri.port, uri.path, null, null).toString()
    }.getOrElse { "invalid-url" }

    fun errorCode(problem: ProblemDetails): String = problem.title ?: "HTTP_${problem.status ?: 0}"
}
