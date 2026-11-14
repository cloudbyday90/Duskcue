// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.content.Context
import androidx.media3.session.MediaSession
import androidx.media3.ui.PlayerView
import java.util.concurrent.CompletableFuture
import java.util.concurrent.atomic.AtomicLong

interface AmbientHost {
    fun changed()
    fun stopped()
    fun legacy(args: Map<*, *>)
}

internal class AmbientRuntime(val token: Long, val identity: AmbientIdentity, val channel: String, var name: String) {
    lateinit var protocol: AmbientProtocol
    @Volatile var record: AmbientRecord? = null
    @Volatile var pendingRecord: AmbientRecord? = null
    @Volatile var selection: AmbientSelection? = null
    @Volatile var ready: AmbientReady? = null
    @Volatile var output: AmbientOutput? = null
    @Volatile var deadline: Long? = null
    @Volatile var phase = "checking"
    @Volatile var error: String? = null
    @Volatile var playIntent = true
    @Volatile var requiresPlay = false
    @Volatile var cleanupOnly = false
    @Volatile var dispatched = false
    @Volatile var busy = false
    @Volatile var sample: AmbientSample? = null
    @Volatile var progressUncertain = false
    @Volatile var retryAt = 0L
    var checkpointSequence = 0L
    var checkpointAt = 0L
    var checkpointState = ""
    var statusAt = -1000L
    var stopWork: NativeWork<Unit>? = null
    var cleanupToken: Long? = null
    var advancing = false
    var advanceIntent: Boolean? = null
    var sampleTimer: AutoCloseable? = null
    var lease: AmbientLease? = null
}

internal class AmbientOwner(context: Context) {
    val context: Context = context.applicationContext
    val lock = Any()
    val timers = AmbientTimers()
    val clock = AmbientClock()
    val http = AmbientTransport(clock, timers)
    val journal = AmbientJournal(this.context, timers)
    val commands = NativeLane("ambient-command", 1, timers)
    val cleanup = NativeLane("ambient-cleanup", 2, timers)
    val renewal = NativeLane("ambient-renew", 1, timers)
    val sampling = NativeLane("ambient-sample", 1, timers)
    val checkpoints = NativeLane("ambient-checkpoint", 1, timers)
    val epochs = AtomicLong()
    val outputs = mutableSetOf<AmbientOutput>()
    @Volatile var identity: AmbientIdentity? = null
    private var bindingInitialized = false
    @Volatile var runtime: AmbientRuntime? = null
    @Volatile var host: AmbientHost? = null
    @Volatile var quarantined = false
    @Volatile var legacyMode = false
    @Volatile var fallback: Map<*, *>? = null
    @Volatile var lastError: String? = null
    @Volatile var journalPresent = true
    @Volatile var restoredEpoch = -1L
    var restoring: NativeWork<Unit>? = null
    private var stopping: NativeWork<Unit>? = null
    var scopeFence: CompletableFuture<Unit>? = null
    private var view: PlayerView? = null
    private var viewIdentity: AmbientIdentity? = null

    fun current(rt: AmbientRuntime, forCleanup: Boolean = false): Boolean =
        if (forCleanup) rt.identity.cleanupSame(identity)
        else runtime === rt && rt.token == epochs.get() && !rt.cleanupOnly && rt.identity.same(identity)

    fun outputCurrent(rt: AmbientRuntime, output: String): Boolean = try {
        current(rt) && !rt.requiresPlay && rt.record?.output == output && rt.ready?.output == output &&
            rt.deadline != null && clock.remaining(rt.deadline!!) > 0
    } catch (_: Exception) { false }

    fun emit() { onMain { host?.changed(); runtime?.output?.facade?.changed(); Unit } }
    fun fail(rt: AmbientRuntime, error: Throwable) {
        if (http.failed) quarantined = true
        if (runtime !== rt || !rt.identity.same(identity) || rt.token != epochs.get() && rt.cleanupToken != epochs.get()) return
        val failure = ambientFailure(error)
        rt.error = failure?.code ?: "playback_uncertain"
        rt.retryAt = maxOf(rt.retryAt, clock.now() + (failure?.retryMs ?: 0))
        rt.phase = if (rt.cleanupOnly) "cleanup" else "uncertain"
        rt.requiresPlay = true
        rt.playIntent = false
        if (rt.error in setOf("lease_uncertain", "scope_changed", "session_expired")) rt.lease?.stop()
        rt.output?.let { retireOutput(rt, it) }
        emit()
    }

    fun save(rt: AmbientRuntime, record: AmbientRecord) {
        demand(current(rt, rt.cleanupOnly), "scope_changed")
        rt.pendingRecord = record
        journal.write(record).result.get()
        demand(current(rt, rt.cleanupOnly), "scope_changed")
        rt.record = record
        journalPresent = true
        rt.pendingRecord = null
    }

    fun start(args: Map<*, *>) {
        val nextIdentity = AmbientIdentity(args)
        val channel = entity(args["channel_id"] as? String ?: "")
        val name = (args["channel_name"] as? String)?.takeIf { it.length in 1..256 } ?: throw AmbientFailure("invalid_channel")
        synchronized(lock) {
            demand(scopeFence?.isDone != false && commands.count() == 0 && cleanup.count() == 0 && !quarantined && !http.failed && !journal.pending() && !journal.failed && outputs.all { it === runtime?.output }, "ambient_cleanup_pending")
            demand(identity == null || identity!!.same(nextIdentity), "scope_changed")
            identity = nextIdentity
            lastError = null
            val previous = runtime
            demand(previous == null || clock.now() >= previous.retryAt, "temporarily_unavailable")
            val epoch = epochs.incrementAndGet()
            previous?.cleanupToken = epoch
            previous?.busy = true
            val rt = AmbientRuntime(epoch, nextIdentity, channel, name)
            rt.playIntent = args["start_paused"] != true
            rt.protocol = AmbientProtocol(nextIdentity, http, clock) { current(rt, it) }
            rt.busy = true
            fallback = args.toMap()
            val work = commands.run(timeoutMs = 60000, cancel = http::cancelCommands, publish = { outcome ->
                rt.busy = false
                previous?.busy = false
                outcome.exceptionOrNull()?.let { fail(if (runtime === previous && previous != null) previous else rt, it) }
                emit()
            }) {
                if (previous != null) settle(previous, send = true)
                synchronized(lock) {
                    demand(epoch == epochs.get() && nextIdentity.same(identity), "scope_changed")
                    runtime = rt
                }
                open(rt)
                Unit
            }
            work.result.whenComplete { _, error -> if (error != null) fail(if (runtime === previous && previous != null) previous else rt, error) }
        }
        emit()
    }

    fun action(action: String, position: Long? = null): CompletableFuture<Unit> {
        val rt = runtime ?: return CompletableFuture.completedFuture(Unit)
        if (action == "pause") {
            rt.playIntent = false
            if (rt.advancing) rt.advanceIntent = false
            return (rt.output?.setPlaying(false) ?: CompletableFuture.completedFuture(Unit)).whenComplete { _, _ -> emit() }
        }
        if (action == "stop") return stop()
        demand(action in setOf("play", "check", "seek") && current(rt, action == "check" && rt.cleanupOnly) && !quarantined && !rt.busy && clock.now() >= rt.retryAt, "control_unavailable")
        rt.busy = true
        val work = try { commands.run(timeoutMs = 60000, cancel = http::cancelCommands, publish = { outcome ->
            rt.busy = false
            outcome.exceptionOrNull()?.let { fail(rt, it) }
            emit()
        }) {
            when (action) {
                "seek" -> seek(rt, position ?: throw AmbientFailure("invalid_position"))
                "play" -> {
                    rt.protocol.context()
                    monitor(rt, once = true)
                    demand(rt.ready != null && rt.phase == "ready", "viewer_unavailable")
                    rt.requiresPlay = false
                    rt.playIntent = true
                    attach(rt)
                    demand(outputCurrent(rt, rt.ready!!.output) && !quarantined && !http.failed, "viewer_unavailable")
                    rt.error = null
                }
                "check" -> if (rt.cleanupOnly) settle(rt, send = false) else { rt.protocol.context(); monitor(rt, once = false) }
            }
            Unit
        } } catch (error: Throwable) { rt.busy = false; throw error }
        work.result.whenComplete { _, error -> if (error != null) fail(rt, error) }
        emit()
        return work.result
    }

    fun stop(): CompletableFuture<Unit> = synchronized(lock) {
        stopping?.let { if (!it.completion.isDone) return it.result }
        val rt = runtime
        if (scopeFence?.isDone == false) return scopeFence!!.thenCompose { stop() }
        if (rt == null && journalPresent && identity != null && commands.count() == 0) {
            return restore(force = true).thenCompose { stop() }
        }
        val fence = epochs.incrementAndGet()
        rt?.let {
            it.cleanupToken = fence
            it.cleanupOnly = true
            it.busy = true
            it.playIntent = false
            it.requiresPlay = true
            it.lease?.stop()
            it.sampleTimer?.close()
        }
        http.cancelCommands()
        rt?.output?.let { retireOutput(rt, it) }
        val work = cleanup.run(timeoutMs = 60000, publish = { rt?.busy = false; emit() }) {
            commands.drain().get()
            retireTrackedOutputs()
            val captured = rt ?: runtime
            if (captured != null) settle(captured, send = true) else {
                demand(!journalPresent && !quarantined && outputs.isEmpty() && !journal.pending() && !journal.failed, "ambient_cleanup_pending")
                onMain { host?.stopped(); Unit }
            }
            Unit
        }
        stopping = work
        work.result.whenComplete { _, error ->
            if (error != null) {
                if (runtime === rt && rt != null) {
                    rt.error = "cleanup_uncertain"
                    rt.phase = "cleanup"
                    rt.retryAt = maxOf(rt.retryAt, clock.now() + (ambientFailure(error)?.retryMs ?: 0))
                }
                emit()
            }
        }
        work.result
    }

    fun bind(args: Map<*, *>?): CompletableFuture<Unit> = synchronized(lock) {
        val next = args?.let(::AmbientIdentity)
        if (!bindingInitialized) { bindingInitialized = true; identity = next }
        demand(next?.same(identity) == true || next == null && identity == null, "scope_changed")
        val epoch = epochs.get()
        val settled = scopeFence ?: CompletableFuture.completedFuture(Unit)
        return settled.thenCompose {
            synchronized(lock) {
                demand(epoch == epochs.get() && (next?.same(identity) == true || next == null && identity == null), "scope_changed")
                restore()
            }
        }
    }

    fun fence(args: Map<*, *>?): CompletableFuture<Unit> = synchronized(lock) {
        val next = args?.let(::AmbientIdentity)
        bindingInitialized = true
        identity = next
        epochs.incrementAndGet()
        http.cancel()
        onMain { view?.player = null; view = null; viewIdentity = null; Unit }
        runtime?.let { rt ->
            rt.cleanupOnly = true
            rt.requiresPlay = true
            rt.playIntent = false
            rt.lease?.stop()
            rt.sampleTimer?.close()
            rt.output?.let { retireOutput(rt, it) }
        }
        fenceScope()
        emit()
        return scopeFence!!
    }

    fun retireOutput(rt: AmbientRuntime?, output: AmbientOutput): CompletableFuture<Unit> {
        onMain { view?.let { output.detachView(it) }; Unit }
        return output.retire(cleanup) { outcome ->
            if (outcome.isFailure) quarantined = true
            else {
                synchronized(lock) { outputs.remove(output) }
                if (rt != null && rt.output === output) rt.output = null
            }
            emit()
        }
    }
    fun retireTrackedOutputs() {
        val captured = synchronized(lock) { outputs.toList() }
        captured.forEach { retireOutput(runtime, it).get() }
        synchronized(lock) { demand(outputs.isEmpty() && !quarantined, "ambient_cleanup_pending") }
    }
    fun backgroundPlaying(): Boolean {
        val rt = runtime ?: return false
        return rt.phase == "ready" && rt.output?.backgroundPlaying() == true
    }
    fun attachView(value: PlayerView) {
        val rt = runtime
        if (rt == null || !current(rt)) { value.player = null; return }
        view?.let { if (it !== value) it.player = null }
        view = value
        viewIdentity = rt.identity
        rt.output?.attachView(value)
    }
    fun detachView(value: PlayerView) { value.player = null; if (view === value) { view = null; viewIdentity = null } }
    fun showOutput(rt: AmbientRuntime) {
        onMain {
            if (current(rt) && viewIdentity?.same(rt.identity) == true) view?.let { rt.output?.attachView(it) }
            host?.changed()
            Unit
        }
    }
    fun mediaSession(): MediaSession? = runtime?.output?.session
    fun status(): Map<String, Any?> {
        val rt = runtime
        val visible = rt?.identity?.same(identity) == true
        val sample = if (visible) rt?.sample else null
        return mapOf("protocol" to if (legacyMode) "legacy" else PROTOCOL,
            "is_active" to (rt != null), "state" to (rt?.phase ?: "idle"), "busy" to (rt?.busy == true),
            "channel_id" to if (visible) rt?.channel else null, "channel_name" to if (visible) rt?.name else null, "media_item_id" to if (visible) rt?.ready?.item else null,
            "position_ms" to (sample?.position ?: 0), "is_playing" to (sample?.playing == true && rt?.phase == "ready" && rt.playIntent && !rt.requiresPlay && !rt.cleanupOnly),
            "is_buffering" to (sample?.buffering == true && rt?.playIntent == true && rt.phase == "ready" && !rt.requiresPlay && !rt.cleanupOnly),
            "duration_ms" to if (sample != null && sample.duration > 0) ((rt?.ready?.offset ?: 0) + sample.duration).coerceAtMost(2147483647) else 0,
            "error" to (rt?.error ?: lastError), "cleanup_pending" to (quarantined || http.failed || cleanup.count() > 0 || scopeFence?.isDone == false || journal.pending() || journal.failed || journalPresent && rt == null || rt?.cleanupOnly == true),
            "requires_play" to (rt?.requiresPlay == true), "request_id" to rt?.record?.start,
            "output_revision" to rt?.record?.output, "video_frames" to (sample?.frames ?: 0).coerceAtMost(2147483647), "progress_uncertain" to (rt?.progressUncertain == true))
    }
}
