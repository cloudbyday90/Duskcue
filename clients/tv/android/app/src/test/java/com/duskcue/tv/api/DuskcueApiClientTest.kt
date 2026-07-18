package com.duskcue.tv.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DuskcueApiClientTest {
    @Test
    fun parses_the_shared_tv_surface_fixture_and_revalidates_with_its_etag() {
        val fixture = fixture("surface-contract.json")
        val body = fixtureJson.parseToJsonElement(fixture).jsonObject.getValue("body").toString()
        val transport = RecordingTransport(
            ApiResponse(
                status = 200,
                headers = mapOf("ETag" to "\"fixture-tv-v1-surface\""),
                body = body,
            ),
        )
        val client = client(transport)

        val result = client.tvSurface(limit = 8, cacheScope = "profile-scope")

        assertTrue(result is ApiResult.Success<TvSurface>)
        val success = result as ApiResult.Success<TvSurface>
        assertEquals("android_tv", success.value.platform)
        assertEquals(listOf("continue", "next_up", "new_episodes", "recommended"), success.value.sections.map(TvSection::section_type))
        assertEquals("Bearer fixture-token", transport.requests.single().headers["Authorization"])

        client.tvSurface(limit = 8, cacheScope = "profile-scope")

        assertEquals("\"fixture-tv-v1-surface\"", transport.requests.last().headers["If-None-Match"])
    }

    @Test
    fun maps_problem_details_without_exposing_the_request_url() {
        val fixture = fixture("deep-link-resolve.json")
        val cases = fixtureJson.parseToJsonElement(fixture).jsonObject.getValue("cases").toString()
        val revoked = fixtureJson.decodeFromString<List<ResolveCase>>(cases)
            .first { it.id == "revoked_access" }
        val transport = RecordingTransport(
            ApiResponse(status = revoked.status, body = Json.encodeToString(ProblemDetails.serializer(), revoked.problem!!)),
        )

        val result = client(transport).resolveTvItem("duskcue:movie:99999999-9999-4999-8999-999999999999")

        assertTrue(result is ApiResult.Failure)
        val failure = result as ApiResult.Failure
        assertEquals("TV_003", failure.problem.title)
        assertEquals("fixture-tv-revoked-access", failure.problem.trace_id)
        assertTrue(transport.requests.single().path.contains("platform=android_tv"))
        assertNull(transport.requests.single().headers["If-None-Match"])
    }

    @Test
    fun canonicalizes_only_supported_server_origins() {
        assertEquals("https://duskcue.example:48027", ServerOrigin.parse("https://DUSKCUE.example").getOrThrow().value)
        assertTrue(ServerOrigin.parse("https://duskcue.example:8443").isFailure)
        assertTrue(ServerOrigin.parse("https://duskcue.example/api/v1").isFailure)
    }

    private fun client(transport: HttpTransport): DuskcueApiClient = DuskcueApiClient(
        origin = ServerOrigin.parse("https://duskcue.example").getOrThrow(),
        transport = transport,
        tokenProvider = object : BearerTokenProvider {
            override fun currentToken(): String = "fixture-token"
        },
        etagStore = MemoryEtagStore(),
    )

    private fun fixture(name: String): String = requireNotNull(javaClass.classLoader?.getResource(name))
        .openStream()
        .bufferedReader()
        .use { it.readText() }

    private val fixtureJson = Json { ignoreUnknownKeys = true }

    private class RecordingTransport(private val response: ApiResponse) : HttpTransport {
        val requests = mutableListOf<ApiRequest>()

        override fun execute(request: ApiRequest): ApiResponse {
            requests += request
            return response
        }
    }
}

@kotlinx.serialization.Serializable
private data class ResolveCase(
    val id: String,
    val status: Int,
    val problem: ProblemDetails? = null,
)
