// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONObject
import java.net.URI
import java.security.MessageDigest

internal fun fingerprint(value: String): String = MessageDigest.getInstance("SHA-256")
    .digest(value.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it.toInt() and 255) }

internal class AmbientIdentity(args: Map<*, *>) {
    val origin = args["server_origin"] as? String ?: throw AmbientFailure("scope_changed")
    val bearer = args["bearer_token"] as? String ?: throw AmbientFailure("scope_changed")
    val account = entity(args["user_id"] as? String ?: throw AmbientFailure("scope_changed"))
    val profile = entity(args["profile_id"] as? String ?: throw AmbientFailure("scope_changed"))
    val epoch = args["scope_epoch"] as? String ?: throw AmbientFailure("scope_changed")
    val uri = URI(origin)
    init {
        demand(uri.scheme in setOf("http", "https") && !uri.host.isNullOrEmpty() && uri.rawUserInfo == null &&
            uri.rawQuery == null && uri.rawFragment == null && uri.rawPath.orEmpty().isEmpty() &&
            origin == "${uri.scheme}://${uri.rawAuthority}" && uri.port in -1..65535 && uri.port != 0 &&
            bearer.length in 1..8192 && bearer.none { it.code <= 32 || it.code >= 127 } && Regex("[0-9]{1,20}:[0-9]{1,20}").matches(epoch), "scope_changed")
    }
    fun same(other: AmbientIdentity?) = other != null && origin == other.origin && bearer == other.bearer &&
        account == other.account && profile == other.profile && epoch == other.epoch
    fun cleanupSame(other: AmbientIdentity?) = other != null && origin == other.origin && bearer == other.bearer &&
        account == other.account && epoch.substringBefore(':') == other.epoch.substringBefore(':')
    override fun toString() = "AmbientIdentity(redacted)"
}

internal data class AmbientScope(
    val originHash: String,
    val user: String,
    val session: String,
    val credential: String,
    val profile: String,
    val revision: String,
    val context: String,
) {
    fun matches(other: AmbientScope, cleanup: Boolean = false) = originHash == other.originHash &&
        user == other.user && session == other.session && credential == other.credential &&
        (cleanup || profile == other.profile && revision == other.revision && context == other.context)
    fun json() = JSONObject().put("origin_hash", originHash).put("user", user).put("session", session)
        .put("credential", credential).put("profile", profile).put("revision", revision).put("context", context)
    companion object {
        fun parse(value: JSONObject): AmbientScope {
            value.exact("origin_hash", "user", "session", "credential", "profile", "revision", "context")
            val hash = value.text("origin_hash", 64)
            val credential = value.text("credential", 20)
            val revision = value.text("revision", 20)
            val context = value.text("context", 47)
            demand(Regex("[a-f0-9]{64}").matches(hash) && Regex("[0-9]{1,20}").matches(credential) &&
                Regex("[0-9]{1,20}").matches(revision) && Regex("sc1\\.[A-Za-z0-9_-]{43}").matches(context))
            return AmbientScope(hash, value.uuid("user"), value.uuid("session"), credential, value.uuid("profile"), revision, context)
        }
        fun context(value: JSONObject, identity: AmbientIdentity, cleanup: Boolean = false): AmbientScope {
            demand(value.text("transport") == "bearer" && value.uuid("user_id") == identity.account, "scope_changed")
            if (!cleanup) demand(!value.flag("profile_selection_required") && value.uuid("profile_id") == identity.profile, "scope_changed")
            demand(value.instant("session_expires_at") > value.instant("observed_at"), "session_expired")
            return parse(JSONObject().put("origin_hash", fingerprint(identity.origin)).put("user", value.get("user_id"))
                .put("session", value.get("session_id")).put("credential", value.get("credential_revision"))
                .put("profile", value.get("profile_id")).put("revision", value.get("profile_context_revision"))
                .put("context", value.get("context_id")))
        }
    }
}

internal fun commandPath(id: String, suffix: String = "") = "/api/v1/playback-commands/${entity(id, true)}$suffix"

internal fun expectedResource(origin: String, supplied: String, path: String): URI {
    val actual = URI(supplied)
    demand(supplied == path || supplied == origin + path)
    demand(actual.rawFragment == null && actual.rawQuery == null && actual.rawUserInfo == null)
    return URI(origin + path)
}
