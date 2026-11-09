// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import org.json.JSONObject
import java.util.concurrent.TimeUnit

internal fun AmbientOwner.open(rt: AmbientRuntime, after: String? = null) {
    val restored = journal.read().result.get()
    demand(current(rt), "scope_changed")
    val actual = try { rt.protocol.context() } catch (error: AmbientFailure) {
        if (restored == null && error.status in setOf(404, 405)) { legacyFallback(rt); return }
        throw error
    }
    if (restored != null) {
        if (actual.matches(restored.scope, cleanup = true)) {
            rt.record = restored
            rt.dispatched = true
            rt.cleanupOnly = true
            rt.requiresPlay = true
            rt.phase = "cleanup"
            emit()
            return
        }
        journal.clear().result.get()
        demand(current(rt), "scope_changed")
    }
    demand(!quarantined && rt.record == null && rt.output == null, "ambient_cleanup_pending")
    var selection = rt.protocol.next(rt.channel, after)
    var acknowledged = false
    for (attempt in 0..1) {
        demand(current(rt), "scope_changed")
        rt.selection = selection
        rt.name = selection.name
        try { acknowledged = rt.protocol.acknowledged(selection); break }
        catch (error: AmbientFailure) {
            if (error.status in setOf(404, 405)) break
            if (attempt != 0 || error.status != 409 || error.problemCode != "PLAY_019") throw error
            selection = rt.protocol.next(rt.channel, after)
        }
    }
    demand(current(rt), "scope_changed")
    if (!acknowledged) { legacyFallback(rt); return }
    demand(!legacyMode, "legacy_cleanup_pending")
    if (clock.stale()) rt.protocol.context()
    val record = AmbientRecord(rt.protocol.scope!!, clock.id())
    save(rt, record)
    demand(current(rt), "scope_changed")
    rt.phase = "starting"
    rt.dispatched = true
    emit()
    val reply = rt.protocol.call("POST", "/api/v1/playback-commands", JSONObject()
        .put("protocol", PROTOCOL).put("request_id", record.start).put("selection", selection.json()))
    demand(reply.status == 202)
    adopt(rt, rt.protocol.receipt(reply, record.start))
    monitor(rt, once = false)
    if (rt.phase == "ready" && !rt.requiresPlay) attach(rt)
}

private fun AmbientOwner.legacyFallback(rt: AmbientRuntime) {
    demand(current(rt) && rt.record == null && rt.pendingRecord == null && !quarantined && !journal.pending() && !journal.failed, "ambient_cleanup_pending")
    val args = fallback ?: throw AmbientFailure("scope_changed")
    legacyMode = true
    if (runtime === rt) runtime = null
    onMain { host?.legacy(args); Unit }.get()
    fallback = null
}

internal fun AmbientOwner.adopt(rt: AmbientRuntime, receipt: AmbientReceipt) {
    demand(current(rt), "scope_changed")
    val record = rt.record ?: throw AmbientFailure("no_viewer")
    val ready = receipt.ready
    if (ready != null) {
        demand(ready.item == rt.selection?.item && (rt.ready == null || ready.file == rt.ready!!.file && ready.session == rt.ready!!.session))
        if (ready.output != record.output) { rt.checkpointSequence = 0; rt.checkpointState = ""; rt.sample = null }
        save(rt, record.copy(output = ready.output, operation = null, phase = "active"))
        rt.ready = ready
    }
    rt.deadline = receipt.deadline
    rt.phase = receipt.state
    if (receipt.deadline != null && receipt.state in setOf("ready", "preparing", "changing", "unavailable")) {
        if (rt.lease == null) rt.lease = AmbientLease(timers, renewal, clock,
            remaining = { if (!current(rt) || rt.cleanupOnly) -1 else rt.deadline?.let(clock::remaining) ?: -1 },
            renew = {
                val id = rt.record?.start ?: throw AmbientFailure("no_viewer")
                val reply = rt.protocol.call("POST", commandPath(id, "/renew"), JSONObject(), renew = true)
                demand(current(rt), "scope_changed")
                rt.deadline = rt.protocol.renewed(reply, id)
            }, uncertain = { error -> fail(rt, AmbientFailure("lease_uncertain", retryMs = ambientFailure(error)?.retryMs ?: 0)) })
        rt.lease!!.start()
    } else rt.lease?.stop()
    emit()
}

internal fun AmbientOwner.attach(rt: AmbientRuntime) {
    val ready = rt.ready ?: throw AmbientFailure("viewer_unavailable")
    demand(current(rt) && !quarantined && !rt.requiresPlay && rt.record?.operation == null &&
        rt.deadline?.let(clock::remaining)?.let { it > 0 } == true, "viewer_unavailable")
    rt.output?.let {
        if (it.ready.output == ready.output) { it.setPlaying(rt.playIntent).get(5, TimeUnit.SECONDS); return }
        retireOutput(rt, it).get(5, TimeUnit.SECONDS)
    }
    demand(rt.output == null && !quarantined, "ambient_cleanup_pending")
    val start = rt.record!!.start
    val output = AmbientOutput(context, start, ready, rt.identity, rt.record!!.scope, rt.name, timers, http,
        object : AmbientControls {
            override fun playing(value: Boolean) = synchronized(lock) {
                demand(runtime === rt && rt.record?.output == ready.output && rt.output?.ready?.output == ready.output, "stale_output")
                action(if (value) "play" else "pause")
            }
            override fun stop() = synchronized(lock) {
                demand(runtime === rt && rt.record?.output == ready.output, "stale_output")
                this@attach.stop()
            }
            override fun seek(position: Long) = synchronized(lock) {
                demand(outputCurrent(rt, ready.output), "stale_output")
                action("seek", position)
            }
            override fun available() = outputCurrent(rt, ready.output) && !quarantined
            override fun seekable() = !rt.busy
        }, current = { outputCurrent(rt, ready.output) }, playIntent = { rt.playIntent },
        ended = { advance(rt) }, failed = { error -> fail(rt, error) })
    rt.output = output
    synchronized(lock) { outputs.add(output) }
    output.attach().get(30, TimeUnit.SECONDS)
    demand(outputCurrent(rt, ready.output), "stale_output")
    showOutput(rt)
    scheduleSamples(rt)
}

internal fun AmbientOwner.advance(rt: AmbientRuntime) {
    synchronized(lock) {
        if (!current(rt) || rt.advancing || rt.error != null || rt.requiresPlay) return
        rt.advancing = true
        rt.busy = true
        rt.advanceIntent = rt.playIntent
    }
    val after = rt.selection?.item
    var owned = rt
    val work = try { commands.run(timeoutMs = 60000, cancel = http::cancelCommands) {
        demand(current(rt), "scope_changed")
        settle(rt, send = true, continuing = true)
        val next = synchronized(lock) {
            demand(rt.token == epochs.get() && rt.identity.same(identity), "scope_changed")
            AmbientRuntime(epochs.incrementAndGet(), rt.identity, rt.channel, rt.name).also { runtime = it }
        }
        owned = next
        next.playIntent = rt.advanceIntent == true
        next.protocol = AmbientProtocol(next.identity, http, clock) { current(next, it) }
        next.busy = true
        try { open(next, after) } catch (error: Throwable) { fail(next, error) }
        finally { next.busy = false; emit() }
        Unit
    } } catch (error: Throwable) { rt.advancing = false; rt.busy = false; fail(rt, error); return }
    work.result.whenComplete { _, error -> if (error != null) fail(owned, error) }
    work.actual.whenComplete { _, _ -> rt.advancing = false; rt.busy = false; emit() }
}
