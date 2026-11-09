// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import java.io.ByteArrayOutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal class AmbientHttpProbe(private val timers: AmbientTimers) {
    private val modes = listOf("ok", "redirect", "gzip", "large", "truncated", "duplicate", "held")
    private fun path(mode: String) = commandPath("019a6275-6000-7000-8000-00000000000${modes.indexOf(mode)}")
    fun run(): Int {
        val server = ServerSocket(0, 1, InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)))
        val failure = AtomicReference<Throwable?>()
        val sockets = mutableSetOf<Socket>()
        val posts = AtomicInteger()
        val held = CountDownLatch(1)
        val accepted = CountDownLatch(1)
        val serving = Thread({
            try {
                while (!server.isClosed) {
                    val socket = try { server.accept() } catch (error: SocketException) { if (server.isClosed) break else throw error }
                    synchronized(sockets) { sockets.add(socket) }
                    try { socket.use { serve(it, posts, held, accepted) } }
                    finally { synchronized(sockets) { sockets.remove(socket) } }
                }
            } catch (error: Throwable) { if (!server.isClosed) failure.set(error) }
        }, "ambient-http-probe").also { it.start() }
        val registry = HttpRequests(1)
        val cancellers = HttpCancellation()
        val identity = AmbientIdentity(mapOf("server_origin" to "http://127.0.0.1:${server.localPort}", "bearer_token" to "qualification-only",
            "user_id" to "019a6275-6000-7000-8000-000000000001", "profile_id" to "019a6275-6000-7000-8000-000000000002", "scope_epoch" to "1:1"))
        fun request(mode: String, post: Boolean = false): ByteArray {
            val exchange = HttpExchange(registry, cancellers, timers, { true }, 32768, false)
            try {
                exchange.open(identity, if (post) "POST" else "GET", if (post) "/api/v1/playback-commands" else path(mode), null, if (post) "{}".toByteArray() else null)
                return exchange.bytes()
            } finally { exchange.close() }
        }
        try {
            check(String(request("ok"), Charsets.UTF_8) == "{\"ok\":true}")
            for (mode in listOf("redirect", "gzip", "large", "truncated", "duplicate")) {
                rejected { request(mode) }
                check(registry.count() == 0 && registry.drain().isDone)
            }
            rejected { request("lost", post = true) }
            check(posts.get() == 1 && registry.count() == 0)
            val exchange = HttpExchange(registry, cancellers, timers, { true }, 32768, false)
            val closeAllowed = CountDownLatch(1)
            val reader = Thread({
                try { exchange.open(identity, "GET", path("held"), null, null); exchange.bytes() }
                catch (_: Exception) { }
                finally { closeAllowed.await(5, TimeUnit.SECONDS); exchange.close() }
            }, "ambient-http-probe-reader").also { it.start() }
            try {
                check(accepted.await(2, TimeUnit.SECONDS) && registry.count() == 1)
                rejected { HttpExchange(registry, cancellers, timers, { true }, 32768, false) }
                exchange.cancel()
                check(registry.count() == 1)
            } finally { held.countDown(); closeAllowed.countDown() }
            reader.join(5000)
            check(!reader.isAlive)
            registry.drain().get(5, TimeUnit.SECONDS)
            check(registry.count() == 0 && posts.get() == 1)
            return posts.get()
        } finally {
            held.countDown()
            server.close()
            synchronized(sockets) { sockets.toList() }.forEach { it.close() }
            serving.join(5000)
            check(!serving.isAlive && synchronized(sockets) { sockets.isEmpty() })
            registry.cancel()
            registry.drain().get(5, TimeUnit.SECONDS)
            cancellers.close().get(5, TimeUnit.SECONDS)
            failure.get()?.let { throw it }
        }
    }
    private fun serve(socket: Socket, posts: AtomicInteger, held: CountDownLatch, accepted: CountDownLatch) {
        socket.soTimeout = 5000
        val input = socket.getInputStream()
        val headers = ByteArrayOutputStream()
        while (headers.size() < 8192) {
            val byte = input.read()
            check(byte >= 0)
            headers.write(byte)
            val data = headers.toByteArray()
            if (data.size >= 4 && data.takeLast(4) == listOf<Byte>(13, 10, 13, 10)) break
        }
        val text = headers.toString("US-ASCII")
        check(text.endsWith("\r\n\r\n") && text.contains("Authorization: Bearer qualification-only", ignoreCase = true))
        val length = Regex("(?im)^Content-Length: ([0-9]+)\\r?$").find(text)?.groupValues?.get(1)?.toInt() ?: 0
        check(length in 0..32768 && input.readNBytes(length).size == length)
        val supplied = text.lineSequence().first().split(' ')[1]
        val path = if (supplied == "/api/v1/playback-commands") "lost" else modes[supplied.last().digitToInt()]
        if (path == "lost") { check(text.startsWith("POST ") && length == 2); posts.incrementAndGet(); return }
        if (path == "held") { accepted.countDown(); check(held.await(5, TimeUnit.SECONDS)) }
        val body = if (path == "truncated") "{" else "{\"ok\":true}"
        val fields = when (path) {
            "redirect" -> "Location: http://example.invalid/secret\r\nContent-Length: 0"
            "gzip" -> "Content-Encoding: gzip\r\nContent-Length: ${body.length}"
            "large" -> "Content-Length: 32769"
            "truncated" -> "Content-Length: 12"
            "duplicate" -> "Content-Length: 11\r\nContent-Length: 12"
            else -> "Content-Length: ${body.length}"
        }
        val response = "HTTP/1.1 ${if (path == "redirect") "302 Found" else "200 OK"}\r\nContent-Type: application/json\r\nConnection: close\r\n$fields\r\n\r\n${if (path == "redirect") "" else body}"
        try { socket.getOutputStream().write(response.toByteArray(Charsets.US_ASCII)); socket.getOutputStream().flush() }
        catch (error: SocketException) { if (path != "held") throw error }
    }
}
