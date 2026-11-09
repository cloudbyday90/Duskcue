// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONObject
import java.net.URI
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLException

internal fun ambientTlsProbe(fixture: JSONObject, timers: AmbientTimers) {
    val origin = fixture.text("origin")
    val parsed = URI(origin)
    check(parsed.scheme == "https" && parsed.host == "localhost" && parsed.port in 1024..65535)
    fun identity(value: String) = AmbientIdentity(mapOf("server_origin" to value, "bearer_token" to fixture.text("token"),
        "user_id" to fixture.uuid("user"), "profile_id" to fixture.uuid("profile"), "scope_epoch" to "1:1"))
    val registry = HttpRequests(1)
    val cancellers = HttpCancellation()
    try {
        val valid = HttpExchange(registry, cancellers, timers, { true }, 32768, false)
        try {
            valid.open(identity(origin), "GET", "/api/v1/auth/session-context", null, null)
            check(valid.status == 200 && Wire.parse(valid.bytes()).uuid("user_id") == fixture.uuid("user"))
        } finally { valid.close() }
        val mismatch = HttpExchange(registry, cancellers, timers, { true }, 32768, false)
        try {
            var failure: Throwable? = null
            try { mismatch.open(identity("https://127.0.0.1:${parsed.port}"), "GET", "/api/v1/auth/session-context", null, null) }
            catch (error: Throwable) { failure = error }
            var tls = false
            repeat(8) { if (failure is SSLException) tls = true; failure = failure?.cause }
            check(tls)
        } finally { mismatch.close() }
        check(registry.count() == 0)
    } finally { registry.cancel(); registry.drain().get(5, TimeUnit.SECONDS); cancellers.close().get(5, TimeUnit.SECONDS) }
}
