// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import java.io.ByteArrayInputStream
import java.io.InputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal class AmbientIoProbe(private val timers: AmbientTimers) {
    private val identity = AmbientIdentity(mapOf("server_origin" to "http://127.0.0.1:12345", "bearer_token" to "probe-only",
        "user_id" to "019a6275-6000-7000-8000-000000000001", "profile_id" to "019a6275-6000-7000-8000-000000000002", "scope_epoch" to "1:1"))
    private class Connection(url: URL, private val body: InputStream, private val disconnecting: () -> Unit = {}) : HttpURLConnection(url) {
        override fun connect() = Unit
        override fun usingProxy() = false
        override fun disconnect() = disconnecting()
        override fun getResponseCode() = 200
        override fun getHeaderFields() = mapOf("Content-Type" to listOf("application/json"))
        override fun getInputStream() = body
    }
    fun run() {
        val cancellers = HttpCancellation()
        try { closingRead(cancellers); lateConnection(cancellers); failedClose(cancellers) }
        finally { cancellers.close().get(5, TimeUnit.SECONDS) }
    }
    private fun closingRead(cancellers: HttpCancellation) {
        val readEntered = CountDownLatch(1)
        val releaseRead = CountDownLatch(1)
        val closeEntered = CountDownLatch(1)
        val releaseClose = CountDownLatch(1)
        val disconnected = CountDownLatch(1)
        val registry = HttpRequests(1)
        val body = object : InputStream() {
            override fun read(): Int { readEntered.countDown(); check(releaseRead.await(5, TimeUnit.SECONDS)); return -1 }
            override fun close() { closeEntered.countDown(); check(releaseClose.await(5, TimeUnit.SECONDS)) }
        }
        val exchange = HttpExchange(registry, cancellers, timers, { true }, 32768, false,
            { url -> Connection(url, body) { disconnected.countDown() } })
        val failure = AtomicReference<Throwable?>()
        val reader = Thread({
            try { exchange.open(identity, "GET", "/api/v1/auth/session-context", null, null); exchange.bytes() }
            catch (_: AmbientFailure) { }
            catch (error: Throwable) { failure.set(error) }
        }, "ambient-probe-read").also { it.start() }
        var closer: Thread? = null
        try {
            check(readEntered.await(2, TimeUnit.SECONDS))
            closer = Thread({ try { exchange.close() } catch (error: Throwable) { failure.set(error) } }, "ambient-probe-close").also { it.start() }
            check(disconnected.await(2, TimeUnit.SECONDS))
            check(registry.count() == 1 && !exchange.completion.isDone)
            rejected { HttpExchange(registry, cancellers, timers, { true }, 32768, false) }
            releaseRead.countDown()
            check(closeEntered.await(2, TimeUnit.SECONDS))
            check(registry.count() == 1 && !exchange.completion.isDone)
        } finally { releaseRead.countDown(); releaseClose.countDown(); reader.join(5000); closer?.join(5000) }
        check(!reader.isAlive && closer?.isAlive != true && registry.count() == 0)
        exchange.completion.get(2, TimeUnit.SECONDS)
        failure.get()?.let { throw it }
    }
    private fun lateConnection(cancellers: HttpCancellation) {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val disconnected = AtomicInteger()
        val registry = HttpRequests(1)
        val failure = AtomicReference<Throwable?>()
        val exchange = HttpExchange(registry, cancellers, timers, { true }, 32768, false, { url ->
            entered.countDown(); check(release.await(5, TimeUnit.SECONDS))
            Connection(url, ByteArrayInputStream(byteArrayOf())) { disconnected.incrementAndGet() }
        })
        val opener = Thread({
            try { exchange.open(identity, "GET", "/api/v1/auth/session-context", null, null); failure.set(AssertionError("Late open succeeded")) }
            catch (_: AmbientFailure) { }
            catch (error: Throwable) { failure.set(error) }
        }, "ambient-probe-open").also { it.start() }
        var closer: Thread? = null
        try {
            check(entered.await(2, TimeUnit.SECONDS))
            val closing = CountDownLatch(1)
            closer = Thread({ closing.countDown(); try { exchange.close() } catch (error: Throwable) { failure.set(error) } }, "ambient-probe-close").also { it.start() }
            check(closing.await(2, TimeUnit.SECONDS))
            exchange.cancel()
            check(registry.count() == 1)
        } finally { release.countDown(); opener.join(5000); closer?.join(5000) }
        check(!opener.isAlive && closer?.isAlive != true && disconnected.get() > 0 && registry.count() == 0)
        exchange.completion.get(2, TimeUnit.SECONDS)
        failure.get()?.let { throw it }
    }
    private fun failedClose(cancellers: HttpCancellation) {
        val registry = HttpRequests(1)
        val exchange = HttpExchange(registry, cancellers, timers, { true }, 32768, false, { url -> Connection(url,
            object : ByteArrayInputStream("{}".toByteArray()) { override fun close() { throw IOException("Controlled public close failure") } }) })
        exchange.open(identity, "GET", "/api/v1/auth/session-context", null, null)
        check(exchange.bytes().contentEquals("{}".toByteArray()))
        rejected { exchange.close() }
        check(exchange.completion.isCompletedExceptionally && registry.count() == 1)
        rejected { registry.drain().get(2, TimeUnit.SECONDS) }
        rejected { HttpExchange(registry, cancellers, timers, { true }, 32768, false) }
        exchange.close()
        check(registry.count() == 1)
    }
}
