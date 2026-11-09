// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import java.io.InputStream
import java.net.HttpURLConnection
import java.net.Proxy
import java.net.URL
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Semaphore
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

internal class HttpRequests(val limit: Int) {
    private val permits = Semaphore(limit)
    private val owners = mutableSetOf<HttpExchange>()
    @Synchronized fun register(exchange: HttpExchange) {
        demand(permits.tryAcquire(), "http_capacity")
        owners.add(exchange)
    }
    @Synchronized fun release(exchange: HttpExchange) { if (owners.remove(exchange)) permits.release() }
    @Synchronized fun count() = owners.size
    fun cancel() { synchronized(this) { owners.toList() }.forEach { it.cancel() } }
    @Synchronized fun drain() = CompletableFuture.allOf(*owners.map { it.completion }.toTypedArray())
}

internal class HttpCancellation {
    private val terminated = CompletableFuture<Unit>()
    private val executor = object : ThreadPoolExecutor(9, 9, 15, TimeUnit.SECONDS, ArrayBlockingQueue<Runnable>(9), namedThreads("ambient-http-close")) {
        override fun terminated() { terminated.complete(Unit) }
    }.apply { allowCoreThreadTimeOut(true) }
    fun run(action: () -> Unit): CompletableFuture<Unit> {
        val complete = CompletableFuture<Unit>()
        try { executor.execute { try { action(); complete.complete(Unit) } catch (error: Throwable) { complete.completeExceptionally(error) } } }
        catch (_: Exception) { complete.completeExceptionally(AmbientFailure("cleanup_capacity")) }
        return complete
    }
    fun close(): CompletableFuture<Unit> { executor.shutdown(); return terminated }
}

internal class HttpExchange(
    private val registry: HttpRequests,
    private val cancellers: HttpCancellation,
    private val timers: AmbientTimers,
    private val authorized: () -> Boolean,
    private val limit: Int,
    private val media: Boolean,
    private val connect: (URL) -> HttpURLConnection = { it.openConnection(Proxy.NO_PROXY) as HttpURLConnection },
) {
    val completion = CompletableFuture<Unit>()
    private val cancelled = AtomicBoolean()
    private val closed = AtomicBoolean()
    @Volatile private var connection: HttpURLConnection? = null
    @Volatile private var input: InputStream? = null
    @Volatile private var cancellation: CompletableFuture<Unit>? = null
    private var whole: AutoCloseable? = null
    private var headers: AutoCloseable? = null
    private var body: AutoCloseable? = null
    private var total = 0
    private var responseLimit = limit
    var length = -1L
        private set
    var status = 0
        private set
    private var fields = emptyMap<String, List<String>>()
    private var operations = 0
    private var ioDone = CompletableFuture.completedFuture(Unit)
    init { registry.register(this) }
    private fun <T> io(action: () -> T): T {
        synchronized(this) {
            check()
            if (operations++ == 0) ioDone = CompletableFuture()
        }
        try { return action() }
        finally { synchronized(this) { if (--operations == 0) ioDone.complete(Unit) } }
    }
    fun check() { demand(!cancelled.get() && !closed.get() && authorized(), "scope_changed") }
    fun cancel() {
        synchronized(this) {
            if (closed.get() || !cancelled.compareAndSet(false, true)) return
            cancellation = cancellers.run { connection?.disconnect() }
        }
    }
    fun open(identity: AmbientIdentity, method: String, path: String, scope: AmbientScope?, bytes: ByteArray?, etag: String? = null, range: String? = null) {
        try { io { openActual(identity, method, path, scope, bytes, etag, range) } }
        catch (error: Throwable) { close(); throw error }
    }
    private fun openActual(identity: AmbientIdentity, method: String, path: String, scope: AmbientScope?, bytes: ByteArray?, etag: String?, range: String?) {
            check()
            demand(path.length <= 512 && path.startsWith("/api/v1/") && !path.contains("..") &&
                path.none { it == '%' || it == '?' || it == '#' || it == '\\' || it.code <= 32 }, "invalid_target")
            demand(ambientRoute(method, path, media) && (method == "GET") == (bytes == null) && (media || range == null), "invalid_target")
            demand(bytes == null || bytes.size <= 32768, "body_limit")
            whole = timers.after(if (media) 75000 else 5000) { cancel() }
            headers = timers.after(5000) { cancel() }
            val request = connect(URL(identity.origin + path))
            connection = request
            check()
            request.instanceFollowRedirects = false
            request.useCaches = false
            request.connectTimeout = 5000
            request.readTimeout = if (media) 10000 else 5000
            request.requestMethod = method
            request.setRequestProperty("Authorization", "Bearer ${identity.bearer}")
            request.setRequestProperty("Accept-Encoding", "identity")
            request.setRequestProperty("Cache-Control", "no-store")
            if (scope != null) request.setRequestProperty("X-Session-Context", scope.context)
            if (etag != null) request.setRequestProperty("If-Match", etag)
            if (range != null) request.setRequestProperty("Range", range)
            if (bytes != null) {
                request.doOutput = true
                request.setRequestProperty("Content-Type", "application/json")
                request.setFixedLengthStreamingMode(bytes.size)
                check()
                request.outputStream.use { stream -> check(); stream.write(bytes); stream.flush(); check() }
            }
            check()
            status = request.responseCode
            check()
            responseLimit = if (status in 200..299) limit else minOf(limit, 32768)
            headers?.close()
            val received = mutableMapOf<String, List<String>>()
            request.headerFields.entries.filter { it.key != null }.forEach {
                val name = it.key.lowercase(java.util.Locale.ROOT)
                demand(received.put(name, it.value.toList()) == null, "duplicate_header")
            }
            fields = received
            demand(fields.size <= 128 && fields.entries.sumOf { it.key.length + it.value.sumOf(String::length) } <= 16384, "header_limit")
            demand(header("content-encoding") in setOf(null, "identity"), "unsupported_encoding")
            val rawLength = header("content-length")
            demand(rawLength == null || Regex("0|[1-9][0-9]{0,9}").matches(rawLength), "invalid_length")
            length = rawLength?.toLong() ?: -1
            demand(length <= responseLimit, "body_limit")
            val transfer = header("transfer-encoding")
            demand(transfer == null || transfer.lowercase(java.util.Locale.ROOT) == "chunked")
            demand(transfer == null || rawLength == null)
            demand(status !in 300..399, "redirect_refused")
            input = if (status in 200..299) request.inputStream else request.errorStream
            check()
            if (media) body = timers.after(60000) { cancel() }
    }
    fun header(name: String): String? {
        val values = fields[name] ?: return null
        demand(values.size == 1 && values[0].length <= 8192)
        return values[0]
    }
    fun read(buffer: ByteArray, offset: Int, count: Int): Int = io {
        check()
        val stream = input ?: throw AmbientFailure("missing_body")
        val timer = timers.after(if (media) 10000 else 5000) { cancel() }
        try {
            val amount = stream.read(buffer, offset, minOf(count, 65536, responseLimit + 1 - total))
            check()
            if (amount == -1) {
                demand(length < 0 || total.toLong() == length, "truncated_response")
                return@io -1
            }
            demand(amount > 0 && total + amount <= responseLimit, "body_limit")
            total += amount
            demand(length < 0 || total <= length, "invalid_length")
            amount
        } finally { timer.close() }
    }
    fun bytes(): ByteArray = io { readLimited(object : InputStream() {
        override fun read(): Int { val one = ByteArray(1); return if (read(one, 0, 1) == -1) -1 else one[0].toInt() and 255 }
        override fun read(buffer: ByteArray, offset: Int, length: Int) = this@HttpExchange.read(buffer, offset, length)
    }, responseLimit) }
    fun close() {
        val closing = synchronized(this) {
            if (!closed.compareAndSet(false, true)) return
            cancellation to ioDone
        }
        var failure: Throwable? = null
        try { connection?.disconnect() } catch (error: Throwable) { failure = failure ?: error }
        try { closing.first?.get() } catch (error: Throwable) { failure = failure ?: error }
        try { closing.second.get() } catch (error: Throwable) { failure = failure ?: error }
        try { input?.close() } catch (error: Throwable) { failure = failure ?: error }
        try { connection?.disconnect() } catch (error: Throwable) { failure = failure ?: error }
        whole?.close(); headers?.close(); body?.close()
        input = null
        connection = null
        if (failure == null) {
            registry.release(this)
            completion.complete(Unit)
        } else {
            val error = AmbientFailure("http_cleanup_failed")
            completion.completeExceptionally(error)
            throw error
        }
    }
}
