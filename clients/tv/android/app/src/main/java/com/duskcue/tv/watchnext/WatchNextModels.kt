package com.duskcue.tv.watchnext

import com.duskcue.tv.TvDeepLink
import com.duskcue.tv.api.TvSurface
import com.duskcue.tv.api.TvSurfaceItem
import com.duskcue.tv.session.PersistedWatchNextMapping
import com.duskcue.tv.session.PersistedWatchNextSuppression
import java.security.MessageDigest
import java.time.Instant
import java.util.UUID

internal enum class WatchNextKind {
    Continue,
    Next,
    New,
}

internal data class WatchNextCandidate(
    val platformContentId: String,
    val mediaItemId: String,
    val seriesId: String?,
    val surfaceItemId: String,
    val mediaType: String,
    val seasonNumber: Int?,
    val episodeNumber: Int?,
    val kind: WatchNextKind,
    val title: String,
    val description: String?,
    val deepLink: String,
    val durationMs: Long,
    val resumePositionMs: Long,
    val lastEngagementTimeMs: Long,
    val fingerprint: String,
)

internal sealed interface WatchNextOperation {
    data class Insert(val candidate: WatchNextCandidate) : WatchNextOperation
    data class Update(
        val mapping: PersistedWatchNextMapping,
        val candidate: WatchNextCandidate,
    ) : WatchNextOperation

    data class Delete(val mapping: PersistedWatchNextMapping) : WatchNextOperation
}

internal data class WatchNextPlan(
    val operations: List<WatchNextOperation>,
)

internal object WatchNextCandidateFactory {
    private const val movieStartedThresholdMs = 120_000L
    private const val episodeStartedThresholdMs = 120_000L
    private const val endCreditsFallbackMs = 180_000L
    private val sourceSections = listOf(
        "continue" to WatchNextKind.Continue,
        "next_up" to WatchNextKind.Next,
        "new_episodes" to WatchNextKind.New,
    )

    fun from(surface: TvSurface): List<WatchNextCandidate> = sourceSections
        .flatMap { (sectionType, kind) ->
            surface.sections
                .asSequence()
                .filter { it.section_type == sectionType }
                .flatMap { it.items.asSequence() }
                .mapNotNull { item -> candidate(sectionType, kind, item) }
                .toList()
        }
        .distinctBy { candidate ->
            candidate.seriesId?.let { "series:$it" } ?: "content:${candidate.platformContentId}"
        }

    private fun candidate(
        sectionType: String,
        kind: WatchNextKind,
        item: TvSurfaceItem,
    ): WatchNextCandidate? {
        val mediaType = item.media_type
        if (item.section_type != sectionType || item.availability != "playable" || mediaType !in setOf("movie", "episode")) {
            return null
        }
        val mediaItemId = canonicalUuid(item.media_item_id) ?: return null
        val expectedContentId = "duskcue:$mediaType:$mediaItemId"
        if (item.platform_content_id != expectedContentId) {
            return null
        }
        val deepLink = item.deep_link ?: return null
        val parsedDeepLink = TvDeepLink.parse(deepLink) as? TvDeepLink.Playback ?: return null
        if (parsedDeepLink.mediaType != mediaType || parsedDeepLink.mediaItemId != mediaItemId) {
            return null
        }
        val durationMs = item.duration_ms?.takeIf { it > 0 } ?: return null
        val lastEngagementTimeMs = item.last_engaged_at
            ?.let { runCatching { Instant.parse(it).toEpochMilli() }.getOrNull() }
            ?: return null
        val title = item.title.trim().takeIf { it.isNotEmpty() } ?: return null
        val seriesId = when (mediaType) {
            "episode" -> item.series_id?.let(::canonicalUuid) ?: return null
            else -> null
        }
        val resumePositionMs = item.resume_position_ms?.coerceAtLeast(0) ?: 0
        if (kind == WatchNextKind.Continue && !isEligibleContinue(mediaType, durationMs, resumePositionMs)) {
            return null
        }
        if (kind != WatchNextKind.Continue && mediaType != "episode") {
            return null
        }
        val description = item.description?.trim()?.takeIf { it.isNotEmpty() }
        val fingerprint = fingerprint(
            expectedContentId,
            mediaItemId,
            seriesId.orEmpty(),
            item.surface_item_id,
            mediaType,
            item.season_number?.toString().orEmpty(),
            item.episode_number?.toString().orEmpty(),
            kind.name,
            title,
            description.orEmpty(),
            deepLink,
            durationMs.toString(),
            resumePositionMs.toString(),
            lastEngagementTimeMs.toString(),
        )
        return WatchNextCandidate(
            platformContentId = expectedContentId,
            mediaItemId = mediaItemId,
            seriesId = seriesId,
            surfaceItemId = item.surface_item_id,
            mediaType = mediaType,
            seasonNumber = item.season_number,
            episodeNumber = item.episode_number,
            kind = kind,
            title = title,
            description = description,
            deepLink = deepLink,
            durationMs = durationMs,
            resumePositionMs = resumePositionMs,
            lastEngagementTimeMs = lastEngagementTimeMs,
            fingerprint = fingerprint,
        )
    }

    private fun isEligibleContinue(mediaType: String, durationMs: Long, resumePositionMs: Long): Boolean {
        val startedThresholdMs = when (mediaType) {
            "movie" -> minOf((durationMs * 0.03).toLong(), movieStartedThresholdMs)
            "episode" -> episodeStartedThresholdMs
            else -> return false
        }
        return resumePositionMs > startedThresholdMs && durationMs - resumePositionMs > endCreditsFallbackMs
    }

    private fun canonicalUuid(value: String): String? = runCatching {
        UUID.fromString(value).toString()
    }.getOrNull()

    private fun fingerprint(vararg values: String): String = MessageDigest.getInstance("SHA-256")
        .digest(values.joinToString("\u001F").toByteArray(Charsets.UTF_8))
        .joinToString(separator = "") { byte -> (byte.toInt() and 0xff).toString(16).padStart(2, '0') }
}

internal object WatchNextReconciler {
    fun plan(
        scopeHash: String,
        candidates: List<WatchNextCandidate>,
        mappings: List<PersistedWatchNextMapping>,
        suppressions: List<PersistedWatchNextSuppression>,
    ): WatchNextPlan {
        val scopedMappings = mappings.filter { it.scope_hash == scopeHash }
        val desiredByContentId = candidates.associateBy(WatchNextCandidate::platformContentId)
        val canonicalMappings = linkedMapOf<String, PersistedWatchNextMapping>()
        val duplicateMappings = mutableListOf<PersistedWatchNextMapping>()
        scopedMappings.forEach { mapping ->
            if (canonicalMappings.putIfAbsent(mapping.platform_content_id, mapping) != null) {
                duplicateMappings += mapping
            }
        }
        val operations = mutableListOf<WatchNextOperation>()
        scopedMappings
            .filter { mapping -> mapping.platform_content_id !in desiredByContentId || mapping in duplicateMappings }
            .forEach { mapping -> operations += WatchNextOperation.Delete(mapping) }
        candidates.forEach { candidate ->
            val mapping = canonicalMappings[candidate.platformContentId]
            when {
                mapping == null && suppressions.none {
                    it.scope_hash == scopeHash &&
                        it.platform_content_id == candidate.platformContentId &&
                        it.fingerprint == candidate.fingerprint
                } -> operations += WatchNextOperation.Insert(candidate)
                mapping != null && mapping.fingerprint != candidate.fingerprint -> {
                    operations += WatchNextOperation.Update(mapping, candidate)
                }
            }
        }
        return WatchNextPlan(operations)
    }
}
