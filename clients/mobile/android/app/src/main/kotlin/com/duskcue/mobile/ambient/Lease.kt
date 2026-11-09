// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

internal class AmbientLease(
    private val timers: AmbientTimers,
    private val lane: NativeLane,
    private val clock: AmbientClock,
    private val remaining: () -> Long,
    private val renew: () -> Unit,
    private val uncertain: (Throwable) -> Unit,
) {
    private var timer: AutoCloseable? = null
    private var epoch = 0L
    private var pending = false
    private var last = 0L
    @Synchronized fun start() {
        if (timer != null) return
        epoch++
        last = clock.now()
        schedule(epoch)
    }
    @Synchronized private fun schedule(expected: Long) { timer = timers.after(1000) { tick(expected) } }
    @Synchronized private fun tick(expected: Long) {
        if (expected != epoch) return
        timer = null
        val left = try { remaining() } catch (_: Exception) { -1 }
        if (left <= 0 || clock.now() - last > 60000) { stop(); uncertain(AmbientFailure("lease_uncertain")); return }
        if (!pending && (clock.now() - last >= 15000 || left <= 15000)) {
            pending = true
            last = clock.now()
            val work = try { lane.run { renew() } } catch (error: Exception) { pending = false; stop(); uncertain(error); return }
            work.result.whenComplete { _, error ->
                synchronized(this) {
                    if (error != null && expected == epoch) { stop(); uncertain(error) }
                }
            }
            work.completion.whenComplete { _, _ -> synchronized(this) { pending = false } }
        }
        if (expected == epoch) schedule(expected)
    }
    @Synchronized fun stop() { epoch++; timer?.close(); timer = null }
}
