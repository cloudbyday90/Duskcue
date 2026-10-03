package com.duskcue.fire.personalization

import com.duskcue.tv.playback.TvInteractivePlayback
import com.duskcue.tv.playback.TvPlaybackSample
import com.duskcue.tv.playback.TvPlaybackSignal
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class FirePlaybackAuthorizationControllerTest {
    private val authorization = FirePlaybackAuthorization(
        "ACCEPTED_MOVIE_1", "a".repeat(43), 600_000, true, true,
        "standard", false, "interactive", true,
    )
    private val events = mutableListOf<FirePlaybackEvent>()
    private val reporter = FireWatchActivityReporter(
        object : FirePlaybackSink {
            override fun available() = true
            override fun sendActive(event: FirePlaybackEvent): Boolean { events += event; return true }
        }, { true }, { 1_000 }, { 0 },
    )

    @Test
    fun cleanupRejectsLatePreparationFromAnOldProfile() = runBlocking<Unit> {
        val pending = CompletableDeferred<FirePlaybackAuthorization?>()
        val controller = FirePlaybackAuthorizationController(reporter, { _, _ -> pending.await() }, this)
        val preparation = async(start = CoroutineStart.UNDISPATCHED) { controller.prepare("item", "session") }
        controller.clear()
        pending.complete(authorization)
        assertFalse(preparation.await())
        controller.started(playback())
        controller.sample(sample(), TvPlaybackSignal.Loaded)
        assertEquals(0, events.size)
        controller.clear()
    }

    @Test
    fun anOldAttemptCannotClearTheReplacementSession() = runBlocking<Unit> {
        val refresh = Channel<Unit>()
        val controller = FirePlaybackAuthorizationController(
            reporter, { _, _ -> authorization }, this, awaitRefresh = { refresh.receive() },
        )
        controller.prepare("item", "old")
        controller.prepare("item", "session")
        controller.clearIfSession("old")
        controller.started(playback())
        controller.sample(sample(), TvPlaybackSignal.Loaded)
        assertEquals(1, events.size)
        controller.clear()
        refresh.close()
    }

    @Test
    fun deniedRenewalClearsReportingBeforeAnyFurtherPlaybackSignal() = runBlocking<Unit> {
        var calls = 0
        val refresh = Channel<Unit>()
        val controller = FirePlaybackAuthorizationController(
            reporter, { _, _ -> calls += 1; if (calls == 1) authorization else null }, this,
            awaitRefresh = { refresh.receive() },
        )
        controller.prepare("item", "session")
        controller.started(playback())
        controller.sample(sample(), TvPlaybackSignal.Loaded)
        refresh.send(Unit)
        yield()
        controller.sample(sample().copy(positionMs = 60_000), TvPlaybackSignal.Seek)
        controller.exited(sample())
        assertEquals(1, events.size)
        assertEquals(2, calls)
        controller.clear()
        refresh.close()
    }

    private fun sample() = TvPlaybackSample("session", 45_000, 900_000, true, true)

    private fun playback() = TvInteractivePlayback(
        "https://fixture.invalid", "fixture-only", "session", "/stream/fixture", "item", "Fixture",
        45_000, "auto", "direct_play", null, null,
    )
}
