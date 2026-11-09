// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONObject

internal data class AmbientReply(val status: Int, val body: JSONObject, val began: Long, val etag: String?, val location: String?)
internal enum class HttpPurpose { COMMAND, RENEW, CLEANUP }

internal fun responseType(raw: String?): String {
    val parts = raw?.split(';') ?: throw AmbientFailure("invalid_content_type")
    demand(parts.size in 1..2 && (parts.size == 1 || Regex("charset\\s*=\\s*(?:utf-8|\"utf-8\")", RegexOption.IGNORE_CASE).matches(parts[1].trim())), "invalid_content_type")
    return parts[0].trim().lowercase(java.util.Locale.ROOT)
}

internal fun retryDelay(raw: String?, date: String?): Long {
    if (raw == null) return 0
    if (raw.length > 128) return 5000
    val value = raw.trim()
    if (Regex("[0-9]{1,8}").matches(value)) return value.toLong().coerceIn(0, 86400) * 1000
    return try {
        val format = java.time.format.DateTimeFormatter.RFC_1123_DATE_TIME
        val at = java.time.ZonedDateTime.parse(value, format).toInstant().toEpochMilli()
        val reference = if (date == null) System.currentTimeMillis() else java.time.ZonedDateTime.parse(date, format).toInstant().toEpochMilli()
        (at - reference).coerceIn(0, 86400000)
    } catch (_: Exception) { 5000 }
}

internal class AmbientTransport(private val clock: AmbientClock, private val timers: AmbientTimers) {
    private val regular = HttpRequests(2)
    private val renewal = HttpRequests(1)
    private val cleanup = HttpRequests(2)
    val cancellers = HttpCancellation()
    @Volatile var failed = false
        private set
    fun call(identity: AmbientIdentity, scope: AmbientScope?, method: String, path: String, body: JSONObject? = null,
             etag: String? = null, purpose: HttpPurpose = HttpPurpose.COMMAND, current: () -> Boolean): AmbientReply {
        val began = clock.now()
        val registry = when (purpose) { HttpPurpose.COMMAND -> regular; HttpPurpose.RENEW -> renewal; HttpPurpose.CLEANUP -> cleanup }
        val exchange = HttpExchange(registry, cancellers, timers, current, 32768, false)
        try {
            val bytes = body?.let { Wire.bytes(it, if (path.endsWith("/checkpoint") || path.endsWith("/withdraw")) 1024 else 32768) }
            exchange.open(identity, method, path, scope, bytes, etag)
            val type = responseType(exchange.header("content-type"))
            demand(type == "application/json" || exchange.status !in 200..299 && type == "application/problem+json")
            val value = Wire.parse(exchange.bytes())
            if (exchange.status !in 200..299) {
                val retry = retryDelay(exchange.header("retry-after"), exchange.header("date"))
                val code = value.opt("code") as? String
                demand(code == null || Regex("[A-Z0-9_]{1,64}").matches(code))
                demand(value.number("status", 599) == exchange.status.toLong())
                throw AmbientFailure("server_refused", exchange.status, retry, code)
            }
            return AmbientReply(exchange.status, value, began, exchange.header("etag"), exchange.header("location"))
        } finally {
            try { exchange.close() }
            finally { if (exchange.completion.isCompletedExceptionally) failed = true }
        }
    }
    fun cancel() { regular.cancel(); renewal.cancel(); cleanup.cancel() }
    fun cancelCommands() { regular.cancel() }
    fun cancelRenewal() { renewal.cancel() }
    fun drainRegular() = java.util.concurrent.CompletableFuture.allOf(regular.drain(), renewal.drain())
    fun drain() = java.util.concurrent.CompletableFuture.allOf(regular.drain(), renewal.drain(), cleanup.drain())
}
