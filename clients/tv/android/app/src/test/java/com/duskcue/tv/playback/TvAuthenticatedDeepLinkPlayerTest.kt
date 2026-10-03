package com.duskcue.tv.playback

import com.duskcue.tv.ActiveTvSession
import com.duskcue.tv.TvDeepLink
import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.ProblemDetails
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.TvPlaybackStartResponse
import com.duskcue.tv.api.TvResolveResponse
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TvAuthenticatedDeepLinkPlayerTest {
    private val resolved = Json.decodeFromJsonElement<TvResolveResponse>(
        Json.parseToJsonElement(requireNotNull(javaClass.getResource("/deep-link-resolve.json")).readText())
            .jsonObject.getValue("cases").jsonArray[0].jsonObject.getValue("body"),
    )
    private val link = TvDeepLink.Playback(resolved.media_type, resolved.media_item_id)
    private val playback = TvPlaybackStartResponse("fixture-session", "direct_play", "/fixture/stream", resolved.media_item_id, playback_mode = "interactive")
    private var session: ActiveTvSession? = ActiveTvSession(
        ServerOrigin.parse("https://fixture.invalid").getOrThrow(), "fixture-user", "fixture-profile", false, 0,
    )
    private var resolveCalls = 0
    private var createCalls = 0
    private val startedPositions = mutableListOf<Long>()
    private val discarded = mutableListOf<String>()
    private val discardedPositions = mutableListOf<Long>()

    @Test
    fun signInAndProfileSelectionPrecedeAllContentRequests() = runBlocking<Unit> {
        val player = player()
        val selected = session
        session = null
        assertEquals(TvDeepLinkPlaybackResult.AuthenticationRequired, player.launch(link))
        session = selected?.copy(profileSelectionRequired = true)
        assertEquals(TvDeepLinkPlaybackResult.ProfileRequired, player.launch(link))
        assertEquals(0, resolveCalls)
        assertEquals(0, createCalls)
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun authenticatedLaunchUsesOnlyTheFreshServerResumePosition() = runBlocking<Unit> {
        assertTrue(player().launch(link) is TvDeepLinkPlaybackResult.Started)
        assertEquals(listOf(2_400_000L), startedPositions)
        assertEquals(1, resolveCalls)
        assertEquals(1, createCalls)
    }

    @Test
    fun revokedOrUnavailableContentNeverCreatesPlayback() = runBlocking<Unit> {
        for (response in listOf(
            ApiResult.Failure(ProblemDetails(status = 403), 403),
            ApiResult.Success(resolved.copy(access_revalidated = false), null),
            ApiResult.Success(resolved.copy(availability = "unavailable"), null),
            ApiResult.Success(resolved.copy(media_item_id = "different-item"), null),
            ApiResult.NetworkFailure,
        )) {
            assertEquals(TvDeepLinkPlaybackResult.Unavailable, player(resolve = { response }).launch(link))
        }
        assertEquals(0, createCalls)
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun expiredAuthenticationReturnsToSignInBeforePlayback() = runBlocking<Unit> {
        val expired = ApiResult.Failure(ProblemDetails(status = 401), 401)
        assertEquals(TvDeepLinkPlaybackResult.SessionExpired, player(resolve = { expired }).launch(link))
        assertEquals(TvDeepLinkPlaybackResult.SessionExpired, player(create = { expired }).launch(link))
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun switchingAwayAndBackDuringResolveRejectsTheOldReply() = runBlocking<Unit> {
        val player = player(resolve = {
            session = session?.copy(scopeVersion = 2)
            ApiResult.Success(resolved, null)
        })
        assertEquals(TvDeepLinkPlaybackResult.Superseded, player.launch(link))
        assertEquals(0, createCalls)
    }

    @Test
    fun aCreatedSessionFromAnOldProfileIsStoppedBeforeLocalPlayback() = runBlocking<Unit> {
        val player = player(create = {
            session = session?.copy(profileId = "another-profile", scopeVersion = 1)
            ApiResult.Success(playback, null)
        })
        assertEquals(TvDeepLinkPlaybackResult.Superseded, player.launch(link))
        assertEquals(listOf(playback.session_id), discarded)
        assertEquals(listOf(resolved.resume_position_ms), discardedPositions)
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun aNewIntentSupersedesAnOlderSuspendedStartReply() = runBlocking<Unit> {
        val pending = CompletableDeferred<ApiResult<TvPlaybackStartResponse>>()
        val player = player(create = {
            if (createCalls == 1) pending.await() else ApiResult.Success(playback.copy(session_id = "new-session"), null)
        })
        val previous = async(start = CoroutineStart.UNDISPATCHED) { player.launch(link) }
        assertTrue(player.launch(link) is TvDeepLinkPlaybackResult.Started)
        pending.complete(ApiResult.Success(playback, null))
        assertEquals(TvDeepLinkPlaybackResult.Superseded, previous.await())
        assertEquals(listOf(playback.session_id), discarded)
        assertEquals(1, startedPositions.size)
    }

    @Test
    fun scopeChangesDuringRuntimePreparationDiscardTheSession() = runBlocking<Unit> {
        val pending = CompletableDeferred<Unit>()
        val player = player(prepare = { pending.await() })
        val previous = async(start = CoroutineStart.UNDISPATCHED) { player.launch(link) }
        player.invalidate()
        pending.complete(Unit)
        assertEquals(TvDeepLinkPlaybackResult.Superseded, previous.await())
        assertEquals(listOf(playback.session_id), discarded)
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun cancellingRuntimePreparationStillStopsTheKnownServerSession() = runBlocking<Unit> {
        val pending = CompletableDeferred<Unit>()
        val player = player(prepare = { pending.await() })
        val attempt = launch(start = CoroutineStart.UNDISPATCHED) { player.launch(link) }
        attempt.cancelAndJoin()
        assertEquals(listOf(playback.session_id), discarded)
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun wrongPlaybackIdentityOrAmbientModeIsDiscarded() = runBlocking<Unit> {
        for (response in listOf(playback.copy(media_item_id = "different-item"), playback.copy(playback_mode = "ambient"))) {
            assertEquals(TvDeepLinkPlaybackResult.Unavailable, player(create = { ApiResult.Success(response, null) }).launch(link))
        }
        assertEquals(2, discarded.size)
        assertTrue(startedPositions.isEmpty())
    }

    @Test
    fun capabilityBearingAndNoncanonicalLaunchUrisAreRejected() {
        listOf(
            "duskcue://play/movie/${link.mediaItemId}?position_ms=1",
            "duskcue://play/movie/${link.mediaItemId}?token=fixture",
            "duskcue://play/movie/${link.mediaItemId}#resume",
            "duskcue://play@other/movie/${link.mediaItemId}",
            "duskcue://play/series/${link.mediaItemId}",
            "duskcue://play/movie/not-a-uuid",
        ).forEach { assertEquals(TvDeepLink.Invalid, TvDeepLink.parse(it)) }
        assertEquals(link, TvDeepLink.parse("duskcue://play/movie/${link.mediaItemId}"))
    }

    private fun player(
        resolve: suspend () -> ApiResult<TvResolveResponse> = { ApiResult.Success(resolved, null) },
        create: suspend () -> ApiResult<TvPlaybackStartResponse> = { ApiResult.Success(playback, null) },
        prepare: suspend () -> Unit = {},
    ) = TvAuthenticatedDeepLinkPlayer(
        currentSession = { session },
        resolve = { _, _ -> resolveCalls += 1; resolve() },
        create = { _, _ -> createCalls += 1; create() },
        start = { expected, resolved, _, isCurrent ->
            prepare()
            if (isCurrent() && session == expected) { startedPositions += resolved.resume_position_ms; true } else false
        },
        discard = { _, playback, positionMs -> discarded += playback.session_id; discardedPositions += positionMs },
    )
}
