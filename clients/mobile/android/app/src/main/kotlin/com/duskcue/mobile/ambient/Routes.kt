// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

private const val UUID = "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}"
private val commandRoute = Regex("/api/v1/playback-commands/$UUID(.*)")
private val channelRoute = Regex("/api/v1/ambient-channels/$UUID/next")
private val operationRoute = Regex("/operations/$UUID")
private val mediaRoute = Regex("/outputs/$UUID/hls/(?:manifest\\.m3u8|[A-Za-z0-9_-]{1,64}/(?:index\\.m3u8|init\\.mp4|seg_[0-9]{1,50}\\.m4s))")

internal fun ambientRoute(method: String, path: String, media: Boolean): Boolean {
    if (!media) {
        if (method == "GET" && path == "/api/v1/auth/session-context") return true
        if (method == "POST" && (path in setOf("/api/v1/playback/prepare", "/api/v1/playback-commands") || channelRoute.matches(path))) return true
    }
    val suffix = commandRoute.matchEntire(path)?.groupValues?.get(1) ?: return false
    return if (media) method == "GET" && mediaRoute.matches(suffix)
        else method == "GET" && (suffix in setOf("", "/configuration", "/checkpoint", "/settlement") || operationRoute.matches(suffix)) ||
            method == "POST" && suffix in setOf("/configuration", "/renew", "/withdraw", "/checkpoint")
}
