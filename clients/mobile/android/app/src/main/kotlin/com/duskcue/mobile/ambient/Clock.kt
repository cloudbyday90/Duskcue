// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.os.SystemClock
import java.nio.ByteBuffer
import java.security.SecureRandom
import java.util.UUID
import kotlin.math.abs

internal class AmbientClock(
    private val monotonic: () -> Long = SystemClock::elapsedRealtime,
    private val wall: () -> Long = System::currentTimeMillis,
    private val random: SecureRandom = SecureRandom(),
) {
    private var observed: Long? = null
    private var began = 0L
    private var wallAt = 0L
    fun now() = monotonic()
    @Synchronized fun observe(server: Long, start: Long) {
        demand(start <= now(), "clock_unavailable")
        observed = server
        began = start
        wallAt = wall() - (now() - start)
    }
    private fun elapsed(): Long {
        demand(observed != null && now() >= began, "clock_unavailable")
        val elapsed = now() - began
        demand(abs(wall() - wallAt - elapsed) <= 2000, "clock_unavailable")
        return elapsed
    }
    @Synchronized fun stale() = observed == null || elapsed() > 30000
    @Synchronized fun remaining(deadline: Long) = if (observed == null) -1 else deadline - observed!! - elapsed()
    @Synchronized fun id(): String {
        demand(!stale(), "clock_unavailable")
        val time = observed!! + elapsed()
        demand(time in 0 until 281474976710656, "clock_unavailable")
        val bytes = ByteArray(16).also(random::nextBytes)
        for (index in 0..5) bytes[index] = (time ushr (40 - 8 * index)).toByte()
        bytes[6] = ((bytes[6].toInt() and 15) or 112).toByte()
        bytes[8] = ((bytes[8].toInt() and 63) or 128).toByte()
        val buffer = ByteBuffer.wrap(bytes)
        return UUID(buffer.long, buffer.long).toString()
    }
}
