package com.duskcue.tv.ui

import android.view.KeyEvent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TvQualityPolicyTest {
    @Test
    fun backDestinationKeepsNavigationInTheAppUntilHome() {
        assertNull(TvQualityPolicy.backDestination(TvRoute.Home, hasDetail = false))
        assertEquals(TvRoute.Home, TvQualityPolicy.backDestination(TvRoute.Browse, hasDetail = false))
        assertEquals(TvRoute.Home, TvQualityPolicy.backDestination(TvRoute.Search, hasDetail = false))
        assertEquals(TvRoute.Home, TvQualityPolicy.backDestination(TvRoute.Settings, hasDetail = false))
        assertEquals(TvRoute.Home, TvQualityPolicy.backDestination(TvRoute.Profiles, hasDetail = false))
        assertEquals(TvRoute.Detail, TvQualityPolicy.backDestination(TvRoute.Player, hasDetail = true))
        assertEquals(TvRoute.Home, TvQualityPolicy.backDestination(TvRoute.Player, hasDetail = false))
    }

    @Test
    fun playerRemoteShortcutsCoverDpadGamepadMediaAndCaptionInputs() {
        assertEquals(
            TvPlayerRemoteShortcut.SeekBackward,
            TvQualityPolicy.playerRemoteShortcut(KeyEvent.KEYCODE_DPAD_LEFT),
        )
        assertEquals(
            TvPlayerRemoteShortcut.SeekForward,
            TvQualityPolicy.playerRemoteShortcut(KeyEvent.KEYCODE_MEDIA_FAST_FORWARD),
        )
        assertEquals(
            TvPlayerRemoteShortcut.TogglePlayback,
            TvQualityPolicy.playerRemoteShortcut(KeyEvent.KEYCODE_BUTTON_A),
        )
        assertEquals(
            TvPlayerRemoteShortcut.TogglePlayback,
            TvQualityPolicy.playerRemoteShortcut(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE),
        )
        assertEquals(
            TvPlayerRemoteShortcut.CycleCaptions,
            TvQualityPolicy.playerRemoteShortcut(KeyEvent.KEYCODE_CAPTIONS),
        )
        assertNull(TvQualityPolicy.playerRemoteShortcut(KeyEvent.KEYCODE_VOLUME_UP))
    }

    @Test
    fun layoutAndTextPolicyRetainLivingRoomMargins() {
        assertTrue(TvQualityPolicy.HorizontalSafeAreaDp >= 58)
        assertTrue(TvQualityPolicy.VerticalSafeAreaDp >= 28)
        assertTrue(TvQualityPolicy.MinimumReadableTextSp >= 20)
    }

    @Test
    fun sharedAccessibilityFixturesCoverAndroidTvReleaseChecks() {
        val json = Json.parseToJsonElement(loadFixture("platform-review-checklists.json")).jsonObject
        val androidTv = json.getValue("platforms").jsonArray
            .map { it.jsonObject }
            .single { it.getValue("id").jsonPrimitive.content == "android_tv_google_tv" }
        val requiredChecks = androidTv.getValue("required_checks").jsonArray.map { it.jsonPrimitive.content }.toSet()
        assertTrue(requiredChecks.containsAll(setOf(
            "D-pad reachability",
            "visible focus",
            "overscan safety",
            "Back button semantics",
            "caption controls",
            "Watch Next launch focus",
        )))

        val remoteCases = Json.parseToJsonElement(loadFixture("remote-navigation-tests.json")).jsonObject
            .getValue("cases").jsonArray
            .map { it.jsonObject.getValue("id").jsonPrimitive.content }
            .toSet()
        assertTrue(remoteCases.containsAll(setOf("tv_home_row_traversal", "tv_player_transport_controls")))
    }

    private fun loadFixture(name: String): String = requireNotNull(javaClass.classLoader?.getResource(name)) {
        "Missing accessibility fixture $name"
    }.readText()
}
