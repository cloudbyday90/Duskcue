// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONObject
import java.util.concurrent.CompletableFuture

internal fun AmbientOwner.statusRead(rt: AmbientRuntime, suffix: String = "", cleanup: Boolean = false): AmbientReply {
    val delay = 1000 - (clock.now() - rt.statusAt)
    if (delay > 0) {
        val elapsed = CompletableFuture<Unit>()
        timers.after(delay) { elapsed.complete(Unit) }
        elapsed.get()
    }
    demand(current(rt, cleanup), "scope_changed")
    rt.statusAt = clock.now()
    return rt.protocol.call("GET", commandPath(rt.record!!.start, suffix), cleanup = cleanup)
}

internal fun AmbientOwner.monitor(rt: AmbientRuntime, once: Boolean) {
    val until = clock.now() + 60000
    do {
        demand(current(rt), "scope_changed")
        val record = rt.record ?: throw AmbientFailure("no_viewer")
        val operation = record.operation
        if (operation == null) {
            adopt(rt, rt.protocol.receipt(statusRead(rt), record.start))
            if (rt.phase !in setOf("preparing", "changing", "stopping", "withdrawing")) return
        } else {
            val reply = statusRead(rt, "/operations/$operation")
            val value = reply.body
            demand(value.text("protocol") == PROTOCOL && value.uuid("request_id", true) == record.start && value.uuid("operation_id", true) == operation)
            expectedResource(rt.identity.origin, value.text("status_url"), commandPath(record.start, "/operations/$operation"))
            val state = value.text("status", 32)
            demand(state in setOf("unknown", "accepted", "preparing", "completed", "failed", "retiring", "cancelled", "uncertain"))
            val complete = value.flag("cleanup_complete")
            if (state in setOf("completed", "failed", "cancelled") && complete) {
                save(rt, record.copy(operation = null))
                adopt(rt, rt.protocol.receipt(statusRead(rt), record.start))
                return
            }
            rt.phase = if (state in setOf("unknown", "uncertain", "failed", "cancelled")) "uncertain" else "changing"
            emit()
            if (rt.phase == "uncertain") { rt.requiresPlay = true; return }
        }
    } while (!once && clock.now() < until)
}

internal fun AmbientOwner.scheduleSamples(rt: AmbientRuntime) {
    rt.sampleTimer?.close()
    if (!current(rt) || rt.output == null) return
    rt.sampleTimer = timers.after(1000) {
        if (!current(rt) || rt.output == null) return@after
        val output = rt.output!!
        val work = try { sampling.run { output.sample().get() } } catch (error: Throwable) { fail(rt, error); return@after }
        work.actual.whenComplete { value, error ->
            if (current(rt) && rt.output === output) {
                if (error != null) fail(rt, error) else {
                    rt.sample = value
                    sendCheckpoint(rt)
                    emit()
                    scheduleSamples(rt)
                }
            }
        }
    }
}

internal fun AmbientOwner.sendCheckpoint(rt: AmbientRuntime) {
    val ready = rt.ready ?: return
    val sample = rt.sample ?: return
    val state = if (sample.buffering) "buffering" else if (!rt.playIntent || !sample.playing) "paused" else "playing"
    if (!outputCurrent(rt, ready.output) || rt.progressUncertain || checkpoints.count() > 0 ||
        clock.now() - rt.checkpointAt < 15000 && state == rt.checkpointState) return
    rt.checkpointState = state
    rt.checkpointAt = clock.now()
    val record = rt.record ?: return
    val sequence = ++rt.checkpointSequence
    val body = JSONObject().put("protocol", PROTOCOL).put("output_revision", ready.output).put("sequence", sequence)
        .put("position_ms", sample.position).put("playback_state", state)
    checkpoints.run {
        val reply = rt.protocol.call("POST", commandPath(record.start, "/checkpoint"), body)
        demand(current(rt) && rt.record?.output == ready.output, "stale_output")
        val value = reply.body
        demand(value.text("protocol") == PROTOCOL && value.uuid("request_id", true) == record.start && value.uuid("output_revision", true) == ready.output &&
            value.number("sequence", 9007199254740991) == sequence && value.number("position_ms") == sample.position && value.text("playback_state") == state)
    }.result.whenComplete { _, error -> if (error != null && current(rt)) { rt.progressUncertain = true; emit() } }
}
