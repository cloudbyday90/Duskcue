// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONObject
import java.util.concurrent.TimeUnit

internal fun AmbientOwner.seek(rt: AmbientRuntime, position: Long) {
    demand(position in 0..2147483647 && current(rt) && rt.record?.operation == null && rt.ready != null, "invalid_position")
    if (clock.stale()) rt.protocol.context()
    val old = rt.record!!
    val configReply = rt.protocol.call("GET", commandPath(old.start, "/configuration"))
    val config = configReply.body
    demand(config.text("protocol") == PROTOCOL && config.uuid("request_id", true) == old.start &&
        config.uuid("output_revision", true) == old.output && config.isNull("pending_operation"))
    val etag = configReply.etag ?: throw AmbientFailure("invalid_configuration")
    demand(Regex("\"vod-config-[a-f0-9]{64}\"").matches(etag))
    rt.phase = "changing"
    emit()
    rt.sampleTimer?.close()
    rt.output?.let { retireOutput(rt, it).get(5, TimeUnit.SECONDS) }
    demand(current(rt) && rt.output == null && !quarantined, "ambient_cleanup_pending")
    val next = old.copy(operation = clock.id(), phase = "uncertain")
    save(rt, next)
    val reply = rt.protocol.call("POST", commandPath(old.start, "/configuration"), JSONObject()
        .put("protocol", PROTOCOL).put("operation_id", next.operation).put("expected_output", old.output)
        .put("action", JSONObject().put("kind", "seek").put("position_ms", position)), etag)
    demand(reply.status == 202)
    expectedResource(rt.identity.origin, reply.location ?: "", commandPath(old.start, "/operations/${next.operation}"))
    demand(reply.body.uuid("operation_id", true) == next.operation && reply.body.uuid("request_id", true) == old.start)
    monitor(rt, once = false)
    if (rt.phase == "ready" && !rt.requiresPlay) attach(rt)
}

private fun AmbientOwner.finalCheckpoint(rt: AmbientRuntime) {
    if (rt.progressUncertain) return
    val record = rt.record ?: return
    val sample = rt.sample ?: return
    val ready = rt.ready ?: return
    val expected = rt.cleanupToken ?: rt.token
    if (record.output != ready.output || !rt.identity.same(identity) || expected != epochs.get()) return
    checkpoints.drain().get()
    val sequence = ++rt.checkpointSequence
    val body = JSONObject().put("protocol", PROTOCOL).put("output_revision", ready.output).put("sequence", sequence)
        .put("position_ms", sample.position).put("playback_state", if (sample.buffering) "buffering" else if (sample.playing) "playing" else "paused")
    try {
        val reply = http.call(rt.identity, record.scope, "POST", commandPath(record.start, "/checkpoint"), body) {
            rt.identity.same(identity) && expected == epochs.get()
        }
        demand(reply.body.number("sequence", 9007199254740991) == sequence && reply.body.uuid("output_revision", true) == ready.output)
    } catch (_: Exception) { rt.progressUncertain = true }
}

internal fun AmbientOwner.settle(rt: AmbientRuntime, send: Boolean, continuing: Boolean = false) {
    rt.cleanupOnly = true
    rt.phase = "stopping"
    rt.playIntent = false
    rt.requiresPlay = true
    rt.lease?.stop()
    rt.sampleTimer?.close()
    val retired = rt.output?.let { retireOutput(rt, it) }
    http.cancelCommands()
    http.cancelRenewal()
    emit()
    renewal.drain().get()
    checkpoints.drain().get()
    http.drainRegular().get()
    journal.drain().get()
    val stored = journal.read().result.get()
    if (rt.record == null && stored != null) {
        if (stored.start != rt.pendingRecord?.start) rt.dispatched = true
        rt.record = stored
    }
    finalCheckpoint(rt)
    retired?.get()
    demand(!quarantined && rt.output == null, "ambient_cleanup_pending")
    sampling.drain().get()
    checkpoints.drain().get()
    val record = rt.record
    if (record != null && rt.dispatched) {
        demand(clock.now() >= rt.retryAt, "temporarily_unavailable")
        val actual = rt.protocol.context(cleanup = true)
        demand(actual.matches(record.scope, cleanup = true), "scope_changed")
        save(rt, record.copy(phase = "cleanup"))
        if (send) {
            val reply = rt.protocol.call("POST", commandPath(record.start, "/withdraw"), JSONObject().put("protocol", PROTOCOL), cleanup = true)
            if (rt.protocol.settled(reply, record.start)) { clearSettled(rt, continuing); return }
        }
        val until = clock.now() + 60000
        do {
            demand(current(rt, true), "scope_changed")
            if (rt.protocol.settled(statusRead(rt, "/settlement", cleanup = true), record.start)) { clearSettled(rt, continuing); return }
        } while (clock.now() < until)
        throw AmbientFailure("cleanup_pending")
    }
    clearSettled(rt, continuing)
}

private fun AmbientOwner.clearSettled(rt: AmbientRuntime, continuing: Boolean) {
    demand(!quarantined && rt.output == null, "ambient_cleanup_pending")
    journal.clear().result.get()
    journalPresent = false
    rt.record = null
    rt.pendingRecord = null
    rt.phase = "settled"
    rt.error = null
    if (!continuing && runtime === rt) runtime = null
    emit()
    if (commands.count() == 0) onMain { if (runtime == null) host?.stopped(); Unit }
}
