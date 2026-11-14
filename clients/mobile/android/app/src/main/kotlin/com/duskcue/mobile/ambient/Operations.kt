// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import java.util.concurrent.CompletableFuture
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadFactory
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

internal class AmbientTimers {
    private val count = AtomicInteger()
    private val terminated = CompletableFuture<Unit>()
    private val executor = object : ScheduledThreadPoolExecutor(1, namedThreads("ambient-timer")) {
        override fun terminated() { terminated.complete(Unit) }
    }.apply { removeOnCancelPolicy = true }
    fun after(ms: Long, action: () -> Unit): AutoCloseable {
        demand(ms >= 0, "timer_capacity")
        if (count.incrementAndGet() > 32) { count.decrementAndGet(); throw AmbientFailure("timer_capacity") }
        val done = java.util.concurrent.atomic.AtomicBoolean()
        fun release() { if (done.compareAndSet(false, true)) count.decrementAndGet() }
        val scheduled = try { executor.schedule({ release(); action() }, ms, TimeUnit.MILLISECONDS) }
        catch (error: Exception) { release(); throw error }
        return AutoCloseable { scheduled.cancel(false); release() }
    }
    fun close(): CompletableFuture<Unit> {
        demand(count.get() == 0, "timer_pending")
        executor.shutdown()
        return terminated
    }
}

internal fun namedThreads(prefix: String): ThreadFactory {
    val next = AtomicInteger()
    return ThreadFactory { task -> Thread(task, "$prefix-${next.incrementAndGet()}").apply { isDaemon = true } }
}

internal class NativeWork<T>(val result: CompletableFuture<T>, val actual: CompletableFuture<T>, val completion: CompletableFuture<Unit>, val cancel: () -> Unit)

internal class NativeLane(private val name: String, val capacity: Int, private val timers: AmbientTimers) {
    private val terminated = CompletableFuture<Unit>()
    private val executor = object : ThreadPoolExecutor(capacity, capacity, 15, TimeUnit.SECONDS, ArrayBlockingQueue<Runnable>(capacity), namedThreads(name)) {
        override fun terminated() { terminated.complete(Unit) }
    }.apply { allowCoreThreadTimeOut(true) }
    private val pending = mutableSetOf<CompletableFuture<Unit>>()
    @Synchronized fun count() = pending.size
    fun <T> run(timeoutMs: Long = 5000, cancel: () -> Unit = {}, publish: (Result<T>) -> Unit = {}, action: () -> T): NativeWork<T> {
        val result = CompletableFuture<T>()
        val actual = CompletableFuture<T>()
        val complete = CompletableFuture<Unit>()
        val ended = java.util.concurrent.atomic.AtomicBoolean()
        val cancelled = java.util.concurrent.atomic.AtomicBoolean()
        val cancellation = {
            if (!ended.get() && cancelled.compareAndSet(false, true)) try { cancel() } catch (_: Exception) { }
            Unit
        }
        synchronized(this) { demand(pending.size < capacity, "operation_busy"); pending.add(complete) }
        val timer = try { timers.after(timeoutMs) { cancellation(); result.completeExceptionally(AmbientFailure("observation_timeout")) } }
        catch (error: Exception) { synchronized(this) { pending.remove(complete) }; complete.complete(Unit); throw error }
        try {
            executor.execute {
                val computed = runCatching(action)
                ended.set(true)
                val outcome = try { publish(computed); computed } catch (error: Throwable) {
                    val failure = computed.exceptionOrNull() ?: error
                    if (failure !== error) failure.addSuppressed(error)
                    Result.failure<T>(failure)
                }
                timer.close()
                synchronized(this) { pending.remove(complete) }
                complete.complete(Unit)
                outcome.fold({ actual.complete(it); result.complete(it) }, { actual.completeExceptionally(it); result.completeExceptionally(it) })
            }
        } catch (_: Exception) {
            val rejected = AmbientFailure("operation_busy")
            val failure = try { publish(Result.failure(rejected)); rejected } catch (error: Throwable) { error }
            timer.close()
            synchronized(this) { pending.remove(complete) }
            complete.complete(Unit)
            actual.completeExceptionally(failure)
            result.completeExceptionally(failure)
        }
        return NativeWork(result, actual, complete, cancellation)
    }
    @Synchronized fun drain(): CompletableFuture<Void> = CompletableFuture.allOf(*pending.toTypedArray())
    @Synchronized fun close(): CompletableFuture<Unit> {
        demand(pending.isEmpty(), "operation_busy")
        executor.shutdown()
        return terminated
    }
}
