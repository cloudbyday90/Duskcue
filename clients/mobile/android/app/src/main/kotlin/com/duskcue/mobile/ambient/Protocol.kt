// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONArray
import org.json.JSONObject

internal data class AmbientSelection(val channel: String, val item: String, val revision: String, val name: String) {
    fun json() = JSONObject().put("media_item_id", item).put("playback_mode", "ambient")
        .put("ambient_channel_id", channel).put("ambient_channel_updated_at", revision)
        .put("audio_stream_index", JSONObject.NULL).put("subtitle_stream_index", JSONObject.NULL)
        .put("force_transcode", true).put("force_hls", true).put("quality_mode", "manual")
        .put("max_streaming_bitrate", 12000000).put("device_profile", JSONObject()
            .put("client", "duskcue_mobile").put("platform", "android_native_ambient")
            .put("video_codecs", JSONArray().put("h264")).put("audio_codecs", JSONArray().put("aac"))
            .put("subtitle_formats", JSONArray()).put("max_resolution", "1080p").put("max_audio_channels", 2)
            .put("hls_supported", true).put("hdr_supported", false))
}

internal data class AmbientReady(val output: String, val session: String, val item: String, val file: String, val position: Long, val offset: Long) {
    companion object {
        fun parse(value: JSONObject): AmbientReady {
            demand(value.text("playback_mode") == "ambient" && value.text("delivery_mode") == "transcode", "unqualified_output")
            demand(value.isNull("subtitle_stream_index"), "unqualified_output")
            return AmbientReady(value.uuid("output_revision", true), value.uuid("session_id", true), value.uuid("media_item_id"),
                value.uuid("media_file_id"), value.number("position_ms"), value.number("stream_offset_ms"))
        }
    }
}

internal data class AmbientReceipt(val state: String, val deadline: Long?, val ready: AmbientReady?)

internal class AmbientProtocol(
    val identity: AmbientIdentity,
    private val http: AmbientTransport,
    val clock: AmbientClock,
    private val current: (Boolean) -> Boolean,
) {
    var scope: AmbientScope? = null
        private set
    fun context(cleanup: Boolean = false): AmbientScope {
        val reply = http.call(identity, null, "GET", "/api/v1/auth/session-context", purpose = if (cleanup) HttpPurpose.CLEANUP else HttpPurpose.COMMAND) { current(cleanup) }
        val actual = AmbientScope.context(reply.body, identity, cleanup)
        demand(scope == null || actual.matches(scope!!, cleanup), "scope_changed")
        scope = actual
        clock.observe(reply.body.instant("observed_at"), reply.began)
        return actual
    }
    fun next(channel: String, after: String?): AmbientSelection {
        val reply = call("POST", "/api/v1/ambient-channels/${entity(channel)}/next", JSONObject().put("after_media_item_id", after ?: JSONObject.NULL))
        val value = reply.body
        val revision = value.text("channel_updated_at", 64)
        value.instant("channel_updated_at")
        return AmbientSelection(channel, value.uuid("media_item_id"), revision, value.text("channel_name", 256))
    }
    fun acknowledged(selection: AmbientSelection): Boolean {
        val reply = call("POST", "/api/v1/playback/prepare", selection.json().put("capability_protocol", 1))
        demand(reply.body.number("protocol") == 1L)
        val capability = reply.body.optJSONObject("command_capability") ?: return false
        demand(capability.text("protocol") == PROTOCOL)
        val expected = setOf("start-settlement", "conditional-controls", "checkpoints", "lease-observation")
        val features = capability.getJSONArray("features")
        demand(features.length() == expected.size && (0 until features.length()).map { features.getString(it) }.toSet() == expected)
        return true
    }
    fun call(method: String, path: String, body: JSONObject? = null, etag: String? = null, cleanup: Boolean = false, renew: Boolean = false): AmbientReply {
        val actual = scope ?: throw AmbientFailure("scope_changed")
        return http.call(identity, if (cleanup) null else actual, method, path, body, etag,
            if (cleanup) HttpPurpose.CLEANUP else if (renew) HttpPurpose.RENEW else HttpPurpose.COMMAND) { current(cleanup) }
    }
    fun receipt(reply: AmbientReply, start: String): AmbientReceipt {
        val value = reply.body
        demand(value.text("protocol") == PROTOCOL && value.uuid("request_id", true) == start)
        expectedResource(identity.origin, value.text("status_url"), commandPath(start))
        if (reply.status == 202) expectedResource(identity.origin, reply.location ?: "", commandPath(start))
        val state = value.text("status", 32)
        demand(state in setOf("unknown", "preparing", "ready", "changing", "unavailable", "uncertain", "stopping", "released", "failed", "withdrawing", "withdrawn", "cleanup_uncertain"))
        clock.observe(value.instant("observed_at"), reply.began)
        val deadline = if (value.isNull("lease_deadline")) null else value.instant("lease_deadline")
        if (deadline != null) demand(clock.remaining(deadline) in 1..60000, "lease_uncertain")
        val ready = if (state == "ready") AmbientReady.parse(value.getJSONObject("ready")) else null
        if (ready != null) {
            demand(deadline != null)
            expectedResource(identity.origin, value.text("stream_url"), commandPath(start, "/outputs/${ready.output}/hls/manifest.m3u8"))
        } else demand(value.isNull("ready") && value.isNull("stream_url"))
        return AmbientReceipt(state, deadline, ready)
    }
    fun settled(reply: AmbientReply, start: String): Boolean {
        val value = reply.body
        demand(value.text("protocol") == PROTOCOL && value.uuid("request_id", true) == start)
        expectedResource(identity.origin, value.text("status_url"), commandPath(start, "/settlement"))
        value.instant("observed_at")
        val status = value.text("status", 16)
        demand(status in setOf("pending", "uncertain", "settled"))
        val fenced = value.flag("admission_fenced")
        val prepared = value.flag("preparation_complete")
        demand(value.isNull("parent_released") || value.opt("parent_released") is Boolean)
        demand(status != "settled" || fenced && prepared && value.opt("parent_released") != false)
        return status == "settled"
    }
    fun renewed(reply: AmbientReply, start: String): Long {
        val value = reply.body
        demand(value.text("protocol") == PROTOCOL && value.uuid("request_id", true) == start && value.text("status") == "renewed")
        expectedResource(identity.origin, value.text("status_url"), commandPath(start))
        clock.observe(value.instant("observed_at"), reply.began)
        val deadline = value.instant("lease_deadline")
        demand(clock.remaining(deadline) in 1..60000, "lease_uncertain")
        return deadline
    }
}
