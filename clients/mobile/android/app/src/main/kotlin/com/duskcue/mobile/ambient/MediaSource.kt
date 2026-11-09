// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.net.Uri
import androidx.media3.common.C
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.TransferListener
import androidx.media3.exoplayer.hls.HlsDataSourceFactory
import java.io.ByteArrayInputStream
import java.io.IOException

internal class AmbientMediaSource(
    private val identity: AmbientIdentity,
    private val scope: AmbientScope,
    private val prefix: String,
    private val timers: AmbientTimers,
    private val cancellers: HttpCancellation,
    private val current: () -> Boolean,
) : HlsDataSourceFactory {
    val requests = HttpRequests(4)
    private val playlists = AmbientPlaylists(prefix, current)
    @Volatile private var closed = false
    override fun createDataSource(dataType: Int): DataSource {
        demand(dataType in setOf(C.DATA_TYPE_MANIFEST, C.DATA_TYPE_MEDIA, C.DATA_TYPE_MEDIA_INITIALIZATION), "unsupported_resource")
        return Source(dataType)
    }
    fun cancel() { closed = true; playlists.clear(); requests.cancel() }
    fun drain() = requests.drain()
    private inner class Source(private val type: Int) : DataSource {
        private var exchange: HttpExchange? = null
        private var buffer: ByteArrayInputStream? = null
        private var uri: Uri? = null
        override fun addTransferListener(listener: TransferListener) = Unit
        override fun getUri() = uri
        override fun getResponseHeaders(): Map<String, List<String>> = emptyMap()
        override fun open(dataSpec: DataSpec): Long {
            try {
                demand(exchange == null && buffer == null && dataSpec.httpMethod == DataSpec.HTTP_METHOD_GET && dataSpec.httpBody == null)
                demand(current() && !closed, "stale_output")
                val raw = dataSpec.uri.toString()
                demand(raw.startsWith(identity.origin + prefix), "invalid_target")
                val suffix = raw.removePrefix(identity.origin + prefix)
                demand(playlists.permits(suffix), "stale_output")
                val manifest = suffix == "manifest.m3u8" || AmbientPlaylists.playlist.matches(suffix)
                demand(manifest == (type == C.DATA_TYPE_MANIFEST))
                val maximum = if (manifest) 1048576 else 16777216
                demand(dataSpec.position in 0..maximum.toLong() && (dataSpec.length == C.LENGTH_UNSET.toLong() || dataSpec.length in 1..maximum.toLong()))
                demand(!manifest || dataSpec.position == 0L && dataSpec.length == C.LENGTH_UNSET.toLong())
                val range = if (!manifest && (dataSpec.position > 0 || dataSpec.length != C.LENGTH_UNSET.toLong())) {
                    val end = if (dataSpec.length == C.LENGTH_UNSET.toLong()) "" else (dataSpec.position + dataSpec.length - 1).toString()
                    "bytes=${dataSpec.position}-$end"
                } else null
                val revision = playlists.revision()
                val request = HttpExchange(requests, cancellers, timers, { current() && !closed }, maximum, true)
                exchange = request
                request.open(identity, "GET", prefix + suffix, scope, null, range = range)
                if (request.status !in setOf(200, 206)) throw AmbientFailure("media_unavailable", request.status,
                    retryDelay(request.header("retry-after"), request.header("date")))
                demand(request.status == 200 && range == null || request.status == 206 && range != null)
                val content = responseType(request.header("content-type"))
                demand(content in if (manifest) setOf("application/vnd.apple.mpegurl", "application/x-mpegurl") else setOf(if (suffix.endsWith("init.mp4")) "video/mp4" else "video/iso.segment"))
                if (request.status == 206) {
                    val match = Regex("bytes ([0-9]{1,10})-([0-9]{1,10})/([0-9]{1,10})")
                        .matchEntire(request.header("content-range").orEmpty()) ?: throw AmbientFailure("invalid_range")
                    val (start, end, total) = match.groupValues.drop(1).map(String::toLong)
                    demand(start == dataSpec.position && end >= start && end < total && total <= maximum && request.length == end - start + 1)
                    demand(end == if (dataSpec.length == C.LENGTH_UNSET.toLong()) total - 1 else minOf(total - 1, start + dataSpec.length - 1))
                } else demand(request.header("content-range") == null)
                uri = dataSpec.uri
                return if (manifest) {
                    val bytes = playlists.validate(request.bytes(), suffix, revision)
                    request.close()
                    exchange = null
                    buffer = ByteArrayInputStream(bytes)
                    bytes.size.toLong()
                } else request.length
            } catch (error: Throwable) { close(); throw IOException("Ambient output unavailable", error) }
        }
        override fun read(target: ByteArray, offset: Int, length: Int): Int {
            try {
                demand(current() && !closed, "stale_output")
                if (length == 0) return 0
                return buffer?.read(target, offset, length) ?: (exchange ?: throw AmbientFailure("missing_body")).read(target, offset, length)
            } catch (error: Throwable) { throw IOException("Ambient output unavailable", error) }
        }
        override fun close() {
            try { buffer?.close(); exchange?.close() }
            finally { buffer = null; exchange = null; uri = null }
        }
    }
}
