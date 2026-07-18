package com.duskcue.tv.home

import com.duskcue.tv.api.ApiRequest
import com.duskcue.tv.api.ApiResponse
import com.duskcue.tv.api.BearerTokenProvider
import com.duskcue.tv.api.DuskcueApiClient
import com.duskcue.tv.api.HttpTransport
import com.duskcue.tv.api.MemoryEtagStore
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.ServerSentEvent
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TvLivingRoomStoreTest {
    @Test
    fun reuses_a_profile_scoped_private_cache_only_after_not_modified() {
        val etags = MemoryEtagStore()
        val transport = QueueTransport(
            ApiResponse(status = 200, headers = mapOf("ETag" to "\"fixture-tv-v1-surface\""), body = surfaceBody()),
            ApiResponse(status = 304),
        )
        val store = TvLivingRoomStore(etags = etags)
        val client = client(transport, etags)
        val scope = TvProfileScope("https://duskcue.example:48027", "user-a", "profile-a")

        val first = store.load(client, scope)
        val second = store.load(client, scope)

        assertTrue(first is TvHomeLoadState.Ready)
        assertTrue(second is TvHomeLoadState.Ready)
        assertEquals("Moonlit Harbor", (second as TvHomeLoadState.Ready).surface.sections.first().items.first().title)
        assertEquals("\"fixture-tv-v1-surface\"", transport.requests.last().headers["If-None-Match"])
    }

    @Test
    fun profile_cleanup_removes_private_rows_and_their_etag() = runBlocking {
        val etags = MemoryEtagStore()
        val transport = QueueTransport(
            ApiResponse(status = 200, headers = mapOf("ETag" to "\"fixture-tv-v1-surface\""), body = surfaceBody()),
            ApiResponse(status = 304),
        )
        val store = TvLivingRoomStore(etags = etags)
        val client = client(transport, etags)
        val scope = TvProfileScope("https://duskcue.example:48027", "user-a", "profile-a")

        store.load(client, scope)
        store.clearProfileScope()
        val afterCleanup = store.load(client, scope)

        assertTrue(afterCleanup is TvHomeLoadState.Failure)
        assertFalse(transport.requests.last().headers.containsKey("If-None-Match"))
    }

    @Test
    fun only_the_active_user_surface_event_requests_a_refresh() {
        val store = TvLivingRoomStore(etags = MemoryEtagStore())
        val scope = TvProfileScope("https://duskcue.example:48027", "user-a", "profile-a")
        val matching = ServerSentEvent(type = "tv_surface_changed", data = """{"user_id":"user-a","reason":"watch_data_updated"}""")
        val otherUser = ServerSentEvent(type = "tv_surface_changed", data = """{"user_id":"user-b","reason":"watch_data_updated"}""")

        assertTrue(store.shouldRefresh(matching, scope))
        assertFalse(store.shouldRefresh(otherUser, scope))
    }

    private fun client(transport: HttpTransport, etags: MemoryEtagStore): DuskcueApiClient = DuskcueApiClient(
        origin = ServerOrigin.parse("https://duskcue.example").getOrThrow(),
        transport = transport,
        tokenProvider = object : BearerTokenProvider {
            override fun currentToken(): String = "fixture-token"
        },
        etagStore = etags,
    )

    private fun surfaceBody(): String = fixtureJson.parseToJsonElement(fixture("surface-contract.json"))
        .jsonObject
        .getValue("body")
        .toString()

    private fun fixture(name: String): String = requireNotNull(javaClass.classLoader?.getResource(name))
        .openStream()
        .bufferedReader()
        .use { it.readText() }

    private class QueueTransport(vararg responses: ApiResponse) : HttpTransport {
        private val pending = ArrayDeque(responses.toList())
        val requests = mutableListOf<ApiRequest>()

        override fun execute(request: ApiRequest): ApiResponse {
            requests += request
            return pending.removeFirst()
        }
    }

    private companion object {
        val fixtureJson = Json { ignoreUnknownKeys = true }
    }
}
