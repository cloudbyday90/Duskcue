package com.duskcue.tv.diagnostics

import java.time.Instant
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TvDiagnosticsTest {
    @Test
    fun exportContainsOnlyBoundedRedactedSupportEvidence() {
        var now = Instant.parse("2026-07-25T12:00:00Z")
        val diagnostics = TvDiagnostics(clientVersion = "0.1.0-test") { now }
        diagnostics.recordScreen("Settings")
        diagnostics.recordRequestFailure(
            url = "https://duskcue.example:48027/api/v1/tv/resolve/secret-item?token=secret",
            statusCode = 403,
            requestId = "request-123",
            traceId = "trace-123",
            errorCode = "TV_003",
        )
        diagnostics.recordPlaybackFailure("playback-123", "direct_play", "PLAY_005")
        diagnostics.recordWatchNextSync(inserted = 1, updated = 2, deleted = 0, failed = 1)

        val bundle = diagnostics.exportBundleJson()

        assertTrue(bundle.contains("duskcue.example"))
        assertTrue(bundle.contains("PLAY_005"))
        assertFalse(bundle.contains("token=secret"))
        assertFalse(bundle.contains("secret-item"))
        assertFalse(bundle.contains("Bearer "))
        assertFalse(bundle.contains("Authorization"))
        assertFalse(bundle.contains("C:\\"))
        assertTrue(bundle.contains("\"playback_failure_summaries\""))
        assertTrue(bundle.contains("\"recent_request_ids\""))
    }

    @Test
    fun recordsExpireAfterTheConfiguredWindow() {
        var now = Instant.parse("2026-07-25T12:00:00Z")
        val diagnostics = TvDiagnostics(clientVersion = "0.1.0-test") { now }
        diagnostics.recordScreen("home")
        now = now.plusSeconds(24 * 60 * 60 + 1)
        diagnostics.recordScreen("settings")

        assertEquals(1, diagnostics.snapshot().size)
        assertEquals("settings", diagnostics.snapshot().single().route_or_screen)
    }

    @Test
    fun exportsCurrentCapabilityReportWithoutHardwareIdentifiers() {
        val diagnostics = TvDiagnostics(
            clientVersion = "0.1.0-test",
            capabilityReportProvider = { route ->
                TvDeviceCapabilityReport(
                    app_version = "0.1.0-test",
                    current_route_or_screen = route,
                    device_family = "nvidia_shield",
                    device_model = "SHIELD_TV_Pro",
                    android_release = "14",
                    android_api_level = 34,
                    display_mode = "3840x2160@60hz",
                    display_hdr_types = listOf("hdr10", "dolby_vision"),
                    video_decoder_mime_types = listOf("h264", "hevc"),
                    audio_output_types = listOf("hdmi"),
                    audio_output_encodings = listOf("eac3", "dolby_truehd"),
                    network_connection_class = "ethernet",
                    network_metered_if_known = false,
                )
            },
        )
        diagnostics.recordScreen("player")

        val bundle = diagnostics.exportBundleJson()

        assertTrue(bundle.contains("nvidia_shield"))
        assertTrue(bundle.contains("3840x2160@60hz"))
        assertTrue(bundle.contains("dolby_truehd"))
        assertFalse(bundle.contains("serial_number"))
        assertFalse(bundle.contains("build_fingerprint"))
    }

    @Test
    fun sharedFixturesRetainTheAndroidTvExportRequirements() {
        val root = Json.parseToJsonElement(loadFixture("platform-export-checklists.json")).jsonObject
        val androidTv = root.getValue("platforms").jsonArray
            .map { it.jsonObject }
            .single { it.getValue("id").jsonPrimitive.content == "android_tv" }
        val checks = androidTv.getValue("required_checks").jsonArray.map { it.jsonPrimitive.content }.toSet()
        assertTrue(checks.containsAll(setOf(
            "include remote/focus and playback launch context",
            "include current display, audio-route, and network capability summaries without hardware serials",
            "exclude account/profile details from communal display",
            "include TV surface event IDs where available",
            "avoid signed artwork or playback URLs",
        )))
    }

    private fun loadFixture(name: String): String = requireNotNull(javaClass.classLoader?.getResource(name)) {
        "Missing diagnostics fixture $name"
    }.readText()
}
