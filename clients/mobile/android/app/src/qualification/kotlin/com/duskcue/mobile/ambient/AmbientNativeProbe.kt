// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.util.UUID
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

internal fun rejected(action: () -> Unit) {
    check(runCatching(action).isFailure) { "Expected native refusal" }
}

internal class AmbientNativeProbe(private val context: Context) {
    fun run(fixture: JSONObject): Map<String, Any> {
        check(context.packageName == "com.duskcue.mobile.qualification")
        val timers = AmbientTimers()
        try {
            fun verify(name: String, action: () -> Unit) { try { action() } catch (_: Throwable) { throw AmbientFailure("probe_$name") } }
            verify("wire", ::wire)
            verify("clock", ::clock)
            verify("hls", ::playlists)
            verify("operations") { lanes(timers) }
            verify("loaders", ::loaders)
            verify("journal") { journal(timers) }
            var requests = 0
            verify("http") { requests = AmbientHttpProbe(timers).run() }
            verify("io") { AmbientIoProbe(timers).run() }
            verify("publication") { AmbientPublicationProbe(context, timers).run() }
            verify("notification") {
                val old = AmbientStopIntents.pending(context, 101, -1)
                val next = AmbientStopIntents.pending(context, 102, -1)
                val queued = AmbientStopIntents.pending(context, 102, 7)
                val captured = AmbientStopIntents.intent(context, 101, -1)
                val otherProcess = if (captured.getStringExtra("ambient_incarnation") == "a".repeat(32)) "b".repeat(32) else "a".repeat(32)
                val restarted = AmbientStopIntents.pending(context, 101, -1, otherProcess)
                try {
                    check(old != next && next != queued && old != restarted)
                    val intent = AmbientStopIntents.intent(context, 101, -1)
                    check(AmbientStopIntents.matches(intent, 101, -1))
                    check(!AmbientStopIntents.matches(intent, 102, -1))
                    check(!AmbientStopIntents.matches(intent, 101, 7))
                    check(!AmbientStopIntents.matches(AmbientStopIntents.intent(context, 101, -1, otherProcess), 101, -1))
                } finally { old.cancel(); next.cancel(); queued.cancel(); restarted.cancel() }
            }
            verify("tls") { ambientTlsProbe(fixture, timers) }
            return mapOf("strict_wire" to true, "uuidv7_clock" to true, "atomic_hls" to true,
                "retained_operations" to true, "joined_loaders" to true, "atomic_journal" to true,
                "native_http" to true, "native_tls" to true, "retained_io" to true, "terminal_publication" to true, "notification_identity" to true,
                "tls_name_refused" to true, "lost_post_count" to requests)
        } finally { timers.close().get(5, TimeUnit.SECONDS) }
    }
    private fun wire() {
        check(Wire.parse("{\"ok\":true}".toByteArray()).flag("ok"))
        for (value in listOf("{\"x\":1,\"x\":2}", "{\"x\":NaN}", "[]", "{\"x\":1} trailing",
            "{\"x\":" + "[".repeat(17) + "0" + "]".repeat(17) + "}")) rejected { Wire.parse(value.toByteArray()) }
        rejected { Wire.parse(byteArrayOf(123, 34, 120, 34, 58, 34, 0xc3.toByte(), 34, 125)) }
        rejected { Wire.parse("{}".toByteArray(), 1) }
        rejected { JSONObject().put("x", 1.5).number("x") }
        rejected { JSONObject().put("x", -1).number("x") }
        check(responseType("Application/JSON; charset=\"UTF-8\"") == "application/json")
        rejected { responseType("application/json; charset=utf-16") }
        rejected { responseType("application/json; charset=utf-8; charset=utf-8") }
        check(ambientRoute("GET", commandPath("019a6275-6000-7000-8000-000000000001"), false))
        check(!ambientRoute("POST", "/api/v1/auth/logout", false))
        check(!ambientRoute("GET", "/api/v1/playback-commands/019a6275-6000-7000-8000-000000000001?token=x", false))
    }
    private fun clock() {
        var monotonic = 1000L
        var wall = 1794182400000L
        val clock = AmbientClock({ monotonic }, { wall })
        rejected { clock.id() }
        clock.observe(wall, monotonic)
        val id = UUID.fromString(clock.id())
        check(id.version() == 7 && id.variant() == 2 && id.mostSignificantBits ushr 16 == wall)
        check(clock.remaining(wall + 60000) == 60000L)
        monotonic += 1000; wall += 1000
        check(clock.remaining(wall + 1000) == 1000L)
        wall += 3000
        rejected { clock.id() }
    }
    private fun playlists() {
        val prefix = "/api/v1/playback-commands/019a6275-6000-7000-8000-000000000001/outputs/019a6275-6000-7000-8000-000000000002/hls/"
        val playlists = AmbientPlaylists(prefix) { true }
        val master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000000,RESOLUTION=1280x720,CODECS=\"avc1.64001f,mp4a.40.2\"\n${prefix}v/index.m3u8\n"
        playlists.validate(master.toByteArray(), "manifest.m3u8", 0)
        val revision = playlists.revision()
        check(playlists.permits("v/index.m3u8") && !playlists.permits("v/init.mp4"))
        val media = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MAP:URI=\"${prefix}v/init.mp4\"\n#EXTINF:4,\n${prefix}v/seg_0.m4s\n#EXT-X-ENDLIST\n"
        for (candidate in listOf(media.replace("${prefix}v/seg_0.m4s", "https://example.invalid/key"),
            media.replace("#EXTINF:4,", "#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n#EXTINF:4,"),
            media.replace("#EXT-X-MAP:URI=", "#EXT-X-MAP:URI=\"bad\",URI="))) {
            rejected { playlists.validate(candidate.toByteArray(), "v/index.m3u8", revision) }
            check(!playlists.permits("v/init.mp4"))
        }
        playlists.validate(media.toByteArray(), "v/index.m3u8", revision)
        check(playlists.permits("v/init.mp4") && playlists.permits("v/seg_0.m4s"))
        playlists.validate(master.toByteArray(), "manifest.m3u8", revision)
        rejected { playlists.validate(media.toByteArray(), "v/index.m3u8", revision) }
        playlists.clear()
        check(!playlists.permits("manifest.m3u8") && !playlists.permits("v/init.mp4"))
        rejected { AmbientPlaylists.variant("BANDWIDTH=1,CODECS=\"hvc1.1.6,mp4a.40.2\"") }
        rejected { AmbientPlaylists.variant("BANDWIDTH=1,BANDWIDTH=2") }
    }
    private fun lanes(timers: AmbientTimers) {
        val lane = NativeLane("ambient-probe", 1, timers)
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val cancelled = AtomicInteger()
        val work = lane.run(timeoutMs = 40, cancel = { cancelled.incrementAndGet() }) {
            entered.countDown(); check(release.await(5, TimeUnit.SECONDS)); 7
        }
        try {
            check(entered.await(2, TimeUnit.SECONDS))
            rejected { work.result.get(2, TimeUnit.SECONDS) }
            check(!work.actual.isDone && !work.completion.isDone && lane.count() == 1 && cancelled.get() == 1)
            rejected { lane.run { 8 } }
            check(!lane.drain().isDone)
        } finally { release.countDown() }
        check(work.actual.get(2, TimeUnit.SECONDS) == 7)
        lane.drain().get(2, TimeUnit.SECONDS)
        lane.close().get(2, TimeUnit.SECONDS)
        rejected { lane.run { 9 }.actual.get(2, TimeUnit.SECONDS) }
    }
    private fun loaders() {
        val loaders = AmbientLoaders()
        val gates = List(4) { CountDownLatch(1) }
        val entered = CountDownLatch(4)
        gates.forEach { gate -> loaders.create().execute { entered.countDown(); check(gate.await(5, TimeUnit.SECONDS)) } }
        val closed: CompletableFuture<Void>
        try {
            check(entered.await(2, TimeUnit.SECONDS))
            rejected { loaders.create() }
            closed = loaders.close()
            check(!closed.isDone)
            rejected { loaders.create() }
        } finally { gates.forEach { it.countDown() } }
        closed.get(2, TimeUnit.SECONDS)
    }
    private fun journal(timers: AmbientTimers) {
        val directory = File(context.noBackupFilesDir, "ambient-probe")
        check(directory.mkdirs() || directory.isDirectory)
        val path = File(directory, "journal.json")
        val journal = AmbientJournal(path, timers)
        val record = AmbientRecord(AmbientScope("a".repeat(64), "019a6275-6000-7000-8000-000000000001",
            "019a6275-6000-7000-8000-000000000002", "1", "019a6275-6000-7000-8000-000000000003", "1", "sc1." + "a".repeat(43)),
            "019a6275-6000-7000-8000-000000000004")
        try {
            journal.clear().actual.get(2, TimeUnit.SECONDS)
            check(journal.read().actual.get(2, TimeUnit.SECONDS) == null)
            journal.write(record).actual.get(2, TimeUnit.SECONDS)
            check(path.length() in 1..4096 && journal.read().actual.get(2, TimeUnit.SECONDS) == record)
            File(path.path + ".new").writeText("{interrupted")
            check(journal.read().actual.get(2, TimeUnit.SECONDS) == record)
            path.writeText("{corrupt")
            rejected { journal.read().actual.get(2, TimeUnit.SECONDS) }
            check(journal.failed)
            journal.clear().actual.get(2, TimeUnit.SECONDS)
            check(!journal.failed && !path.exists())
            path.writeBytes(ByteArray(4097) { 32 })
            rejected { journal.read().actual.get(2, TimeUnit.SECONDS) }
            journal.clear().actual.get(2, TimeUnit.SECONDS)
            check(path.mkdir())
            val occupied = File(path, "occupied")
            occupied.writeText("Owned failure fixture")
            rejected { journal.write(record).actual.get(2, TimeUnit.SECONDS) }
            check(journal.failed && path.isDirectory)
            check(occupied.delete())
            check(path.delete())
            journal.clear().actual.get(2, TimeUnit.SECONDS)
        } finally { journal.drain().get(5, TimeUnit.SECONDS); journal.close().get(5, TimeUnit.SECONDS) }
        check(directory.listFiles().orEmpty().isEmpty() && directory.delete())
    }
}
