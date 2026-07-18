package com.duskcue.tv

import org.junit.Assert.assertEquals
import org.junit.Test

class TvDeepLinkTest {
    @Test
    fun accepts_only_canonical_movie_and_episode_playback_links() {
        assertEquals(
            TvDeepLink.Playback(
                mediaType = "movie",
                mediaItemId = "11111111-1111-4111-8111-111111111111",
            ),
            TvDeepLink.parse("duskcue://play/movie/11111111-1111-4111-8111-111111111111"),
        )
        assertEquals(
            TvDeepLink.Playback(
                mediaType = "episode",
                mediaItemId = "22222222-2222-4222-8222-222222222222",
            ),
            TvDeepLink.parse("DUSKCUE://PLAY/episode/22222222-2222-4222-8222-222222222222"),
        )
    }

    @Test
    fun rejects_noncanonical_or_capability_bearing_links() {
        listOf(
            "duskcue://play/series/11111111-1111-4111-8111-111111111111",
            "duskcue://play/movie/not-a-uuid",
            "duskcue://play/movie/11111111-1111-4111-8111-111111111111?token=secret",
            "duskcue://play/movie/11111111-1111-4111-8111-111111111111#resume",
            "duskcue://play@attacker/movie/11111111-1111-4111-8111-111111111111",
            "https://duskcue.example/play/movie/11111111-1111-4111-8111-111111111111",
        ).forEach { uri ->
            assertEquals(TvDeepLink.Invalid, TvDeepLink.parse(uri))
        }
    }
}
