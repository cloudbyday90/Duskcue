// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import androidx.media3.exoplayer.util.ReleasableExecutor
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

internal class AmbientLoaders {
    private val owners = mutableSetOf<Owned>()
    private var closed = false
    @Synchronized fun create(): ReleasableExecutor {
        demand(!closed && owners.size < 4, "loader_capacity")
        return Owned().also { owners.add(it) }
    }
    fun close(): CompletableFuture<Void> {
        val captured = synchronized(this) { closed = true; owners.toList() }
        captured.forEach { it.release() }
        return CompletableFuture.allOf(*captured.map { it.complete }.toTypedArray())
    }
    private inner class Owned : ReleasableExecutor {
        val complete = CompletableFuture<Unit>()
        private val executor = object : ThreadPoolExecutor(1, 1, 0, TimeUnit.SECONDS, ArrayBlockingQueue<Runnable>(1), namedThreads("ambient-hls-loader")) {
            override fun terminated() {
                synchronized(this@AmbientLoaders) { owners.remove(this@Owned) }
                complete.complete(Unit)
            }
        }
        override fun execute(command: Runnable) { executor.execute(command) }
        override fun release() { executor.shutdown() }
    }
}
