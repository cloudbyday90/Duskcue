package com.duskcue.fire.personalization

import com.duskcue.tv.playback.TvInteractivePlayback
import com.duskcue.tv.playback.TvPlaybackSample
import com.duskcue.tv.playback.TvPlaybackSignal
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FireWatchActivityReporterTest {
    private class Sink : FirePlaybackSink {
        var supported = true
        var accepted = true
        var fail = false
        val events = mutableListOf<FirePlaybackEvent>()
        override fun available(): Boolean = supported
        override fun sendActive(event: FirePlaybackEvent): Boolean {
            if (fail) throw IllegalStateException("unavailable")
            if (accepted) events += event
            return accepted
        }
    }

    private val sink = Sink()
    private var clock = 0L
    private var wallClock = 1_000L
    private var enabled = true
    private val reporter = FireWatchActivityReporter(sink, { enabled }, { wallClock }, { clock })
    private val authorization = FirePlaybackAuthorization(
        catalogContentId = "accepted-fixture-movie-1",
        opaqueProfileKey = "a".repeat(43),
        expiresAtMs = 600_000,
        integrationEnabled = true,
        customerOptedIn = true,
        profileType = "standard",
        profileSelectionRequired = false,
        playbackMode = "interactive",
        accessRevalidated = true,
    )

    @Test
    fun consumesTheCanonicalWatchActivitySequence() {
        val resource = requireNotNull(javaClass.classLoader?.getResource("watch-activity.json"))
        val fixture = Json.parseToJsonElement(resource.readText()).jsonObject
        start()
        fixture.getValue("samples").jsonArray.forEach { value ->
            val step = value.jsonObject
            clock = step.getValue("elapsed_ms").jsonPrimitive.long
            val sample = sample(
                position = step.getValue("position_ms").jsonPrimitive.long,
                loaded = step.getValue("loaded").jsonPrimitive.boolean,
                playing = step.getValue("playing").jsonPrimitive.boolean,
            )
            val trigger = step.getValue("trigger").jsonPrimitive.content
            if (trigger == "Exit") reporter.exited(sample)
            else reporter.sample(sample, TvPlaybackSignal.valueOf(trigger))
            val expected = step["expected_state"]?.jsonPrimitive?.contentOrNull
            if (expected != null) {
                assertEquals(expected, sink.events.last().state.name)
                assertEquals(sample.positionMs, sink.events.last().positionMs)
            }
            assertEquals(step.getValue("total_events").jsonPrimitive.long.toInt(), sink.events.size)
        }
        assertTrue(sink.events.all { it.catalogContentId == authorization.catalogContentId })
        assertTrue(sink.events.all { it.opaqueProfileKey == authorization.opaqueProfileKey })
    }

    @Test
    fun deniesEveryIndependentPrivacyAndAuthorizationGate() {
        listOf(
            null,
            authorization.copy(integrationEnabled = false),
            authorization.copy(customerOptedIn = false),
            authorization.copy(profileType = "kids"),
            authorization.copy(profileSelectionRequired = true),
            authorization.copy(playbackMode = "ambient"),
            authorization.copy(accessRevalidated = false),
            authorization.copy(expiresAtMs = wallClock),
            authorization.copy(catalogContentId = ""),
            authorization.copy(catalogContentId = "duskcue:movie:01900000-0000-7000-8000-000000000001"),
            authorization.copy(catalogContentId = " accepted-fixture-movie-1"),
            authorization.copy(catalogContentId = "invalid\ncontent"),
            authorization.copy(opaqueProfileKey = "01900000-0000-7000-8000-000000000001"),
        ).forEach { denied ->
            start(denied)
            reporter.sample(sample(), TvPlaybackSignal.Loaded)
            reporter.sample(sample(position = 90_000), TvPlaybackSignal.Seek)
            reporter.exited(sample())
            assertTrue(sink.events.isEmpty())
        }
    }

    @Test
    fun featureFlagAndDeviceAvailabilityCannotBeAssumed() {
        start()
        enabled = false
        reporter.sample(sample(), TvPlaybackSignal.Loaded)
        enabled = true
        sink.supported = false
        reporter.sample(sample(), TvPlaybackSignal.Loaded)
        assertTrue(sink.events.isEmpty())
    }

    @Test
    fun rejectsUnloadedUnknownDurationAndOutOfRangePositions() {
        start()
        listOf(
            sample(loaded = false),
            sample().copy(durationMs = -1),
            sample().copy(durationMs = 0),
            sample(position = -1),
            sample(position = 900_001),
            sample().copy(sessionId = "old-session"),
        ).forEach { reporter.sample(it, TvPlaybackSignal.Loaded) }
        assertTrue(sink.events.isEmpty())
        reporter.sample(sample(position = 45_000), TvPlaybackSignal.Loaded)
        assertEquals(45_000, sink.events.single().positionMs)
    }

    @Test
    fun clearsBeforeRevokedScopeExitAndRejectsLaterCallbacks() {
        start()
        reporter.sample(sample(), TvPlaybackSignal.Loaded)
        reporter.clear()
        reporter.exited(sample())
        clock = 60_000
        reporter.sample(sample(), TvPlaybackSignal.Tick)
        assertEquals(1, sink.events.size)
    }

    @Test
    fun authorizationExpiryStopsAllSubsequentEvents() {
        start()
        reporter.sample(sample(), TvPlaybackSignal.Loaded)
        wallClock = authorization.expiresAtMs
        reporter.sample(sample(), TvPlaybackSignal.Seek)
        reporter.exited(sample())
        assertEquals(1, sink.events.size)
    }

    @Test
    fun sdkFailuresDoNotEscapeOrReplayOldActiveEvents() {
        listOf(false, true).forEach { exception ->
            start()
            sink.fail = exception
            sink.accepted = exception
            reporter.sample(sample(), TvPlaybackSignal.Loaded)
            sink.fail = false
            sink.accepted = true
            clock += 60_000
            reporter.sample(sample(), TvPlaybackSignal.Tick)
            reporter.exited(sample())
            assertTrue(sink.events.isEmpty())
        }
    }

    @Test
    fun replacingPlaybackDoesNotLetTheOldExitEraseTheNewBinding() {
        start()
        reporter.sample(sample(), TvPlaybackSignal.Loaded)
        reporter.bind("replacement", authorization)
        reporter.exited(sample())
        reporter.started(playback().copy(sessionId = "replacement"))
        reporter.sample(sample(position = 12_000).copy(sessionId = "replacement"), TvPlaybackSignal.Loaded)
        assertEquals(2, sink.events.size)
        assertEquals(12_000, sink.events.last().positionMs)
    }

    @Test
    fun interstitialUsesTheProvidedContentPositionAndCompletionUsesExit() {
        start()
        reporter.sample(sample(position = 45_000).copy(interstitial = true), TvPlaybackSignal.StateChanged)
        assertEquals(FirePlaybackState.INTERSTITIAL, sink.events.single().state)
        assertEquals(45_000, sink.events.single().positionMs)
        reporter.exited(sample(position = 900_000))
        assertEquals(FirePlaybackState.EXIT, sink.events.last().state)
        assertEquals(900_000, sink.events.last().positionMs)
    }

    private fun start(value: FirePlaybackAuthorization? = authorization) {
        reporter.bind("session", value)
        reporter.started(playback())
    }

    private fun sample(position: Long = 45_000, loaded: Boolean = true, playing: Boolean = true) =
        TvPlaybackSample("session", position, 900_000, loaded, playing)

    private fun playback() = TvInteractivePlayback(
        serverOrigin = "https://fixture.invalid",
        bearerToken = "fixture-only",
        sessionId = "session",
        streamUrl = "/stream/fixture",
        mediaItemId = "fixture-item",
        title = "Fixture",
        startPositionMs = 45_000,
        qualityMode = "auto",
        streamDecision = "direct_play",
        audioLanguage = null,
        subtitleLanguage = null,
    )
}
