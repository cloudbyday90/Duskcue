package com.duskcue.tv.api

import com.duskcue.tv.diagnostics.TvDiagnostics
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonArray
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
    fun records_server_correlation_without_exporting_the_raw_request_path() {
        val diagnostics = TvDiagnostics("0.1.0-test")
        val transport = RecordingTransport(
            ApiResponse(
                status = 403,
                headers = mapOf("X-Request-Id" to "request-123"),
                body = """{"title":"TV_003","status":403,"trace_id":"trace-123"}""",
            ),
        )
        val client = DuskcueApiClient(
            origin = ServerOrigin.parse("https://duskcue.example").getOrThrow(),
            transport = transport,
            tokenProvider = object : BearerTokenProvider {
                override fun currentToken(): String = "fixture-token"
            },
            etagStore = MemoryEtagStore(),
            diagnostics = diagnostics,
        )

        client.resolveTvItem("duskcue:movie:99999999-9999-4999-8999-999999999999")

        val bundle = diagnostics.exportBundleJson()
        assertTrue(bundle.contains("request-123"))
        assertTrue(bundle.contains("trace-123"))
        assertTrue(bundle.contains("TV_003"))
        assertTrue(bundle.contains("/api/v1/tv/resolve/:id"))
        assertTrue(!bundle.contains("99999999-9999-4999-8999-999999999999"))
        assertTrue(!bundle.contains("fixture-token"))
    }

    @Test
    fun canonicalizes_only_supported_server_origins() {
        assertEquals("https://duskcue.example:48027", ServerOrigin.parse("https://DUSKCUE.example").getOrThrow().value)
        assertEquals("https://[2001:db8::1]:48027", ServerOrigin.parse("https://[2001:db8::1]").getOrThrow().value)
        assertTrue(ServerOrigin.parse("https://duskcue.example:8443").isFailure)
        assertTrue(ServerOrigin.parse("https://duskcue.example/api/v1").isFailure)
    }

    @Test
    fun retries_only_safe_requests_with_bounded_retry_after() {
        val attempts = ArrayDeque(
            listOf(
                ApiResponse(status = 503, headers = mapOf("Retry-After" to "60")),
                ApiResponse(status = 200),
            ),
        )
        val delays = mutableListOf<Long>()
        val transport = RetryingTransport(
            delegate = HttpTransport { attempts.removeFirst() },
            policy = RetryPolicy(maxAttempts = 3, initialDelayMs = 100, maxDelayMs = 500),
            sleeper = RetrySleeper { delay -> delays += delay },
        )

        val response = transport.execute(ApiRequest(method = "GET", path = "https://duskcue.example:48027/api/v1/health"))

        assertEquals(200, response.status)
        assertEquals(listOf(500L), delays)
        assertNull(RetryPolicy().retryDelayMs(ApiRequest(method = "POST", path = "https://duskcue.example"), ApiResponse(status = 503), 1))
    }

    @Test
    fun decodes_tv_surface_events_and_redacts_diagnostics() {
        val fixture = fixture("cross-device-resume.json")
        val event = fixtureJson.parseToJsonElement(fixture)
            .jsonObject
            .getValue("scenario")
            .jsonArray[1]
            .jsonObject
            .getValue("event")
            .jsonObject
        val frame = ServerSentEventDecoder().decode(
            "id: ${event.getValue("id").toString().trim('"')}\nevent: ${event.getValue("type").toString().trim('"')}\ndata: ${event.getValue("data")}\n\n",
        ).single()

        val hint = requireNotNull(frame.tvSurfaceChangedHint(fixtureJson))

        assertEquals("resume_position_changed", hint.reason)
        assertEquals("11111111-1111-4111-8111-111111111111", hint.media_item_id)
        assertEquals(
            "https://duskcue.example:48027/api/v1/tv/resolve/item",
            DiagnosticsRedactor.redactedUrl("https://duskcue.example:48027/api/v1/tv/resolve/item?token=secret"),
        )
        assertTrue(DiagnosticsRedactor.redactedHeaders(mapOf("Authorization" to "Bearer secret", "X-Request-Id" to "safe")).containsKey("X-Request-Id"))
        assertTrue(!DiagnosticsRedactor.redactedHeaders(mapOf("Authorization" to "Bearer secret")).containsKey("Authorization"))
    }

    @Test
    fun sends_device_link_requests_without_a_bearer_token() {
        val transport = RecordingTransport(
            ApiResponse(
                status = 200,
                body = """{"device_code":"device-code","user_code":"USER-CODE","verification_uri":"https://duskcue.example/auth/link","expires_in":600,"interval":5}""",
            ),
        )

        val result = client(transport).requestDeviceCode(
            DeviceCodeRequest(
                device_id = "device-id",
                client_name = "Duskcue Android TV",
                client_platform = "android_tv",
                client_version = "0.1.0",
            ),
        )

        assertTrue(result is ApiResult.Success<DeviceCodeResponse>)
        val request = transport.requests.single()
        assertEquals("POST", request.method)
        assertTrue(request.path.endsWith("/api/v1/device/code"))
        assertNull(request.headers["Authorization"])
        assertTrue(requireNotNull(request.body).contains("device-id"))
    }

    @Test
    fun starts_playback_with_a_typed_android_tv_profile_after_a_resolve_handoff() {
        val fixture = fixtureJson.parseToJsonElement(fixture("deep-link-resolve.json"))
        val playable = fixture.jsonObject.getValue("cases").jsonArray.first().jsonObject.getValue("body").toString()
        val startResponse = """
            {
              "session_id":"50505050-5050-4050-8050-505050505050",
              "stream_decision":"direct_stream",
              "stream_url":"/api/v1/transcode/50505050-5050-4050-8050-505050505050/manifest.m3u8",
              "media_item_id":"11111111-1111-4111-8111-111111111111",
              "media_file_id":"66666666-6666-4666-8666-666666666666",
              "selected_audio_stream_index":1,
              "selected_subtitle_stream_index":3,
              "restart_required":true,
              "playback_mode":"interactive"
            }
        """.trimIndent()
        val transport = QueueTransport(
            ApiResponse(status = 200, body = playable),
            ApiResponse(status = 200, body = startResponse),
        )
        val client = client(transport)

        val resolve = client.resolveTvItem("duskcue:movie:11111111-1111-4111-8111-111111111111")
        val resolved = (resolve as ApiResult.Success).value
        val start = client.startTvPlayback(
            StartTvPlaybackRequest(
                media_item_id = resolved.media_item_id,
                audio_stream_index = 1,
                subtitle_stream_index = 3,
                device_profile = TvDeviceProfile.androidTv(),
            ),
        )

        assertEquals(2_400_000, resolved.resume_position_ms)
        assertEquals("/api/v1/playback/start", resolved.playback_start?.path)
        assertTrue(start is ApiResult.Success<TvPlaybackStartResponse>)
        val playbackStart = (start as ApiResult.Success).value
        assertEquals(1, playbackStart.selected_audio_stream_index)
        assertEquals(3, playbackStart.selected_subtitle_stream_index)
        assertTrue(playbackStart.restart_required)
        assertEquals("Bearer fixture-token", transport.requests.last().headers["Authorization"])
        assertTrue(requireNotNull(transport.requests.last().body).contains("android_tv"))
        assertTrue(requireNotNull(transport.requests.last().body).contains("11111111-1111-4111-8111-111111111111"))
        assertTrue(requireNotNull(transport.requests.last().body).contains("\"audio_stream_index\":1"))
        assertTrue(requireNotNull(transport.requests.last().body).contains("\"subtitle_stream_index\":3"))
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

    private class QueueTransport(vararg responses: ApiResponse) : HttpTransport {
        private val responses = ArrayDeque(responses.toList())
        val requests = mutableListOf<ApiRequest>()

        override fun execute(request: ApiRequest): ApiResponse {
            requests += request
            return responses.removeFirst()
        }
    }
}

@kotlinx.serialization.Serializable
private data class ResolveCase(
    val id: String,
    val status: Int,
    val problem: ProblemDetails? = null,
)
