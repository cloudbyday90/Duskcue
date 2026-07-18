package com.duskcue.tv.api

import java.net.URI

data class ApiRequest(
    val method: String,
    val path: String,
    val headers: Map<String, String> = emptyMap(),
    val body: String? = null,
)

data class ApiResponse(
    val status: Int,
    val headers: Map<String, String> = emptyMap(),
    val body: String = "",
)

fun interface HttpTransport {
    fun execute(request: ApiRequest): ApiResponse
}

interface BearerTokenProvider {
    fun currentToken(): String?
}

class MutableBearerTokenProvider(initialToken: String? = null) : BearerTokenProvider {
    @Volatile
    private var token: String? = initialToken

    override fun currentToken(): String? = token

    fun replace(value: String?) {
        token = value
    }

    fun clear() {
        token = null
    }
}

interface EtagStore {
    fun read(key: String): String?
    fun write(key: String, value: String)
    fun remove(key: String)
    fun clear()
}

class MemoryEtagStore : EtagStore {
    private val values = mutableMapOf<String, String>()

    override fun read(key: String): String? = values[key]

    override fun write(key: String, value: String) {
        values[key] = value
    }

    override fun remove(key: String) {
        values.remove(key)
    }

    override fun clear() {
        values.clear()
    }
}

class ServerOrigin private constructor(val value: String) {
    companion object {
        fun parse(input: String): Result<ServerOrigin> = runCatching {
            val uri = URI(input.trim())
            require(uri.scheme == "http" || uri.scheme == "https")
            require(!uri.host.isNullOrBlank())
            require(uri.userInfo == null)
            require(uri.query == null && uri.fragment == null)
            require(uri.path.isNullOrBlank() || uri.path == "/")
            require(uri.port == -1 || uri.port == 48027)
            val scheme = uri.scheme.lowercase()
            val host = uri.host.lowercase()
            val normalizedHost = when {
                host.startsWith("[") && host.endsWith("]") -> host
                host.contains(":") -> "[$host]"
                else -> host
            }
            ServerOrigin("$scheme://$normalizedHost:48027")
        }
    }
}

internal fun Map<String, String>.header(name: String): String? =
    entries.firstOrNull { it.key.equals(name, ignoreCase = true) }?.value
