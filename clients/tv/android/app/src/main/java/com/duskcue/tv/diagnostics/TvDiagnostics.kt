package com.duskcue.tv.diagnostics

import com.duskcue.tv.api.DiagnosticsRedactor
import java.net.URI
import java.time.Instant
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class TvDiagnosticRecord(
    val timestamp: String,
    val client_version: String,
    val platform: String = "android_tv",
    val route_or_screen: String,
    val request_id: String,
    val event_type: String,
    val severity: String,
    val privacy_classification: String,
    val server_origin_redacted: String? = null,
    val error_code: String? = null,
    val trace_id: String? = null,
    val playback_session_id: String? = null,
    val tv_surface_event_id: String? = null,
    val status_code: Int? = null,
    val stream_decision: String? = null,
    val operation_summary: String? = null,
)

@Serializable
internal data class TvDiagnosticsBundle(
    val schema_version: Int = 1,
    val exported_at: String,
    val app_logs: List<TvDiagnosticRecord>,
    val device_capability_report: TvDeviceCapabilityReport,
    val server_url_redacted: TvServerUrlRedacted?,
    val playback_failure_summaries: List<TvPlaybackFailureSummary>,
    val network_state: TvNetworkState,
    val recent_request_ids: List<TvRequestIdSummary>,
    val tv_surface_event_ids: List<String>,
)

@Serializable
data class TvDeviceCapabilityReport(
    val platform: String = "android_tv",
    val os_family: String = "android",
    val app_version: String,
    val codec_support_summary: String = "reported_by_playback_profile",
    val hls_support: Boolean = true,
    val hdr_audio_subtitle_summary: String = "reported_by_playback_profile",
    val current_route_or_screen: String,
    val device_family: String = "android_tv",
    val device_model: String = "unknown",
    val android_release: String = "unknown",
    val android_api_level: Int? = null,
    val display_mode: String = "unknown",
    val display_hdr_types: List<String> = emptyList(),
    val video_decoder_mime_types: List<String> = emptyList(),
    val audio_output_types: List<String> = emptyList(),
    val audio_output_encodings: List<String> = emptyList(),
    val network_connection_class: String = "unknown",
    val network_metered_if_known: Boolean? = null,
    val vpn_or_proxy_hint: String = "unknown",
)

@Serializable
internal data class TvServerUrlRedacted(
    val server_origin_host_only: String,
    val network_mode_if_known: String = "unknown",
    val tls_present: Boolean,
)

@Serializable
internal data class TvPlaybackFailureSummary(
    val playback_session_id: String,
    val request_id: String,
    val trace_id: String? = null,
    val error_code: String,
    val stream_decision: String? = null,
    val bounded_failure_category: String,
)

@Serializable
internal data class TvNetworkState(
    val online: Boolean? = null,
    val connection_class: String = "unknown",
    val metered_if_known: Boolean? = null,
    val vpn_or_proxy_hint: String = "unknown",
    val last_reachability_error_code: String? = null,
)

@Serializable
internal data class TvRequestIdSummary(
    val request_id: String,
    val route_or_screen: String,
    val timestamp: String,
    val status_code: Int? = null,
)

class TvDiagnostics(
    private val clientVersion: String,
    private val capabilityReportProvider: (String) -> TvDeviceCapabilityReport = { route ->
        TvDeviceCapabilityReport(
            app_version = clientVersion,
            current_route_or_screen = route,
        )
    },
    private val now: () -> Instant = { Instant.now() },
) {
    private val records = ArrayDeque<TvDiagnosticRecord>()
    private var currentRoute = "launch"
    private var lastServerOrigin: String? = null
    private var lastServerTls: Boolean? = null
    private var lastNetworkFailure: String? = null

    @Synchronized
    fun recordScreen(routeOrScreen: String) {
        currentRoute = boundedRoute(routeOrScreen)
        record(
            routeOrScreen = currentRoute,
            requestId = "unavailable",
            eventType = "screen_viewed",
            severity = "info",
        )
    }

    @Synchronized
    fun recordRequest(url: String, statusCode: Int, requestId: String?) {
        lastServerOrigin = DiagnosticsRedactor.hostOnly(url)
        lastServerTls = usesTls(url)
        lastNetworkFailure = null
        record(
            routeOrScreen = apiRoute(url),
            requestId = requestId,
            eventType = "api_request_completed",
            severity = "info",
            statusCode = statusCode,
        )
    }

    @Synchronized
    fun recordRequestFailure(
        url: String,
        statusCode: Int,
        requestId: String?,
        traceId: String?,
        errorCode: String,
    ) {
        lastServerOrigin = DiagnosticsRedactor.hostOnly(url)
        lastServerTls = usesTls(url)
        record(
            routeOrScreen = apiRoute(url),
            requestId = requestId,
            eventType = "api_request_failed",
            severity = "warn",
            statusCode = statusCode,
            traceId = traceId,
            errorCode = errorCode,
        )
    }

    @Synchronized
    fun recordNetworkFailure(url: String) {
        lastServerOrigin = DiagnosticsRedactor.hostOnly(url)
        lastServerTls = usesTls(url)
        lastNetworkFailure = "network_unreachable"
        record(
            routeOrScreen = apiRoute(url),
            requestId = "unavailable",
            eventType = "api_request_network_failed",
            severity = "warn",
            errorCode = lastNetworkFailure,
        )
    }

    @Synchronized
    fun recordPlaybackStarted(playbackSessionId: String, streamDecision: String) {
        record(
            routeOrScreen = "player",
            requestId = "unavailable",
            eventType = "playback_started",
            severity = "info",
            playbackSessionId = playbackSessionId,
            streamDecision = streamDecision,
        )
    }

    @Synchronized
    fun recordPlaybackFailure(playbackSessionId: String, streamDecision: String?, errorCode: String) {
        record(
            routeOrScreen = "player",
            requestId = "unavailable",
            eventType = "playback_failed",
            severity = "error",
            playbackSessionId = playbackSessionId,
            streamDecision = streamDecision,
            errorCode = errorCode,
        )
    }

    @Synchronized
    fun recordWatchNextSync(inserted: Int, updated: Int, deleted: Int, failed: Int) {
        record(
            routeOrScreen = "watch_next",
            requestId = "unavailable",
            eventType = if (failed == 0) "watch_next_sync_completed" else "watch_next_sync_partial",
            severity = if (failed == 0) "info" else "warn",
            errorCode = if (failed == 0) null else "watch_next_provider_failed",
            operationSummary = "inserted_${inserted}_updated_${updated}_deleted_${deleted}_failed_${failed}",
        )
    }

    @Synchronized
    fun recordWatchNextDisabled() {
        record(
            routeOrScreen = "watch_next",
            requestId = "unavailable",
            eventType = "watch_next_program_disabled",
            severity = "info",
        )
    }

    @Synchronized
    fun recordTvSurfaceEvent(eventId: String?) {
        record(
            routeOrScreen = "tv_surface",
            requestId = "unavailable",
            eventType = "tv_surface_changed_received",
            severity = "info",
            tvSurfaceEventId = eventId,
        )
    }

    @Synchronized
    fun clear() {
        records.clear()
        currentRoute = "launch"
        lastServerOrigin = null
        lastServerTls = null
        lastNetworkFailure = null
    }

    @Synchronized
    fun exportBundleJson(): String = Json.encodeToString(bundle())

    @Synchronized
    internal fun snapshot(): List<TvDiagnosticRecord> = records.toList()

    private fun bundle(): TvDiagnosticsBundle {
        prune()
        val snapshot = records.toList()
        val capabilityReport = capabilityReportProvider(currentRoute)
        return TvDiagnosticsBundle(
            exported_at = now().toString(),
            app_logs = snapshot,
            device_capability_report = capabilityReport,
            server_url_redacted = lastServerOrigin?.let { host ->
                TvServerUrlRedacted(
                    server_origin_host_only = host,
                    tls_present = lastServerTls == true,
                )
            },
            playback_failure_summaries = snapshot.filter { it.event_type == "playback_failed" }
                .mapNotNull { record ->
                    record.playback_session_id?.let { sessionId ->
                        TvPlaybackFailureSummary(
                            playback_session_id = sessionId,
                            request_id = record.request_id,
                            trace_id = record.trace_id,
                            error_code = requireNotNull(record.error_code),
                            stream_decision = record.stream_decision,
                            bounded_failure_category = "media_playback",
                        )
                    }
                },
            network_state = TvNetworkState(
                online = if (lastNetworkFailure == null) null else false,
                connection_class = capabilityReport.network_connection_class,
                metered_if_known = capabilityReport.network_metered_if_known,
                vpn_or_proxy_hint = capabilityReport.vpn_or_proxy_hint,
                last_reachability_error_code = lastNetworkFailure,
            ),
            recent_request_ids = snapshot.filter { it.request_id != "unavailable" }
                .takeLast(MAX_REQUEST_IDS)
                .map { record ->
                    TvRequestIdSummary(
                        request_id = record.request_id,
                        route_or_screen = record.route_or_screen,
                        timestamp = record.timestamp,
                        status_code = record.status_code,
                    )
                },
            tv_surface_event_ids = snapshot.mapNotNull(TvDiagnosticRecord::tv_surface_event_id)
                .distinct()
                .takeLast(MAX_TV_SURFACE_EVENT_IDS),
        )
    }

    private fun record(
        routeOrScreen: String,
        requestId: String?,
        eventType: String,
        severity: String,
        statusCode: Int? = null,
        traceId: String? = null,
        errorCode: String? = null,
        playbackSessionId: String? = null,
        streamDecision: String? = null,
        operationSummary: String? = null,
        tvSurfaceEventId: String? = null,
    ) {
        prune()
        records += TvDiagnosticRecord(
            timestamp = now().toString(),
            client_version = clientVersion,
            route_or_screen = boundedRoute(routeOrScreen),
            request_id = requestId?.takeIf(::isOpaqueId) ?: "unavailable",
            event_type = eventType,
            severity = severity,
            privacy_classification = "operational",
            server_origin_redacted = lastServerOrigin,
            error_code = errorCode?.takeIf(::isSafeCode),
            trace_id = traceId?.takeIf(::isOpaqueId),
            playback_session_id = playbackSessionId?.takeIf(::isOpaqueId),
            status_code = statusCode,
            stream_decision = streamDecision?.takeIf(::isSafeCode),
            operation_summary = operationSummary?.takeIf(::isSafeCode),
            tv_surface_event_id = tvSurfaceEventId?.takeIf(::isOpaqueId),
        )
        while (records.size > MAX_RECORDS) records.removeFirst()
    }

    private fun prune() {
        val oldest = now().minusSeconds(MAX_AGE_SECONDS)
        while (records.firstOrNull()?.timestamp?.let { Instant.parse(it).isBefore(oldest) } == true) {
            records.removeFirst()
        }
    }

    private fun apiRoute(url: String): String = runCatching {
        URI(url).path.split('/').filter(String::isNotBlank).joinToString(separator = "/", prefix = "/") { segment ->
            if (segment in STATIC_ROUTE_SEGMENTS) segment else ":id"
        }
    }.getOrDefault("api/invalid")

    private fun boundedRoute(value: String): String = value
        .lowercase()
        .replace(Regex("[^a-z0-9_/:.-]"), "_")
        .take(MAX_ROUTE_LENGTH)
        .ifBlank { "unknown" }

    private fun isOpaqueId(value: String): Boolean = value.length in 1..128 &&
        value.none { it.isWhitespace() || it == '/' || it == '?' || it == '&' || it == '=' }

    private fun isSafeCode(value: String): Boolean = value.matches(Regex("^[a-zA-Z0-9_.-]{1,96}$"))

    private fun usesTls(value: String): Boolean = runCatching { URI(value).scheme.equals("https", ignoreCase = true) }
        .getOrDefault(false)

    private companion object {
        const val MAX_RECORDS = 1_000
        const val MAX_AGE_SECONDS = 24 * 60 * 60L
        const val MAX_REQUEST_IDS = 100
        const val MAX_TV_SURFACE_EVENT_IDS = 100
        const val MAX_ROUTE_LENGTH = 96
        val STATIC_ROUTE_SEGMENTS = setOf(
            "api", "v1", "users", "me", "tv-surface", "tv", "resolve", "libraries", "items",
            "collections", "media-items", "files", "segments", "search", "settings", "playback",
            "start", "heartbeat", "seek", "stop", "qoe", "device", "code", "token", "profiles",
            "switch", "parent-unlock", "auth", "logout", "logout-all",
        )
    }
}
