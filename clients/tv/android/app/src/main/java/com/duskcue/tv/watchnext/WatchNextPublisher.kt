package com.duskcue.tv.watchnext

import android.annotation.SuppressLint
import android.content.ContentResolver
import android.content.ContentUris
import android.net.Uri
import androidx.tvprovider.media.tv.TvContractCompat
import androidx.tvprovider.media.tv.WatchNextProgram
import com.duskcue.tv.api.TvSurface
import com.duskcue.tv.home.TvProfileScope
import com.duskcue.tv.session.PersistedWatchNextArtwork
import com.duskcue.tv.session.PersistedWatchNextMapping
import com.duskcue.tv.session.PersistedWatchNextSuppression
import com.duskcue.tv.session.SecureTvState
import com.duskcue.tv.session.TvSessionStore
import java.security.MessageDigest
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal sealed interface WatchNextProviderResult {
    data class Inserted(val programId: Long) : WatchNextProviderResult
    data object Updated : WatchNextProviderResult
    data object Deleted : WatchNextProviderResult
    data object Missing : WatchNextProviderResult
    data object Failed : WatchNextProviderResult
}

internal interface WatchNextProvider {
    fun insert(candidate: WatchNextCandidate): WatchNextProviderResult
    fun update(programId: Long, candidate: WatchNextCandidate): WatchNextProviderResult
    fun delete(programId: Long): WatchNextProviderResult
}

internal class AndroidWatchNextProvider(
    private val contentResolver: ContentResolver,
) : WatchNextProvider {
    override fun insert(candidate: WatchNextCandidate): WatchNextProviderResult {
        val uri = runCatching {
            contentResolver.insert(
                TvContractCompat.WatchNextPrograms.CONTENT_URI,
                program(candidate).toContentValues(),
            )
        }.getOrNull() ?: return WatchNextProviderResult.Failed
        val programId = runCatching { ContentUris.parseId(uri) }.getOrNull() ?: return WatchNextProviderResult.Failed
        return if (programId >= 0) WatchNextProviderResult.Inserted(programId) else WatchNextProviderResult.Failed
    }

    override fun update(programId: Long, candidate: WatchNextCandidate): WatchNextProviderResult {
        val rows = runCatching {
            contentResolver.update(
                TvContractCompat.buildWatchNextProgramUri(programId),
                program(candidate).toContentValues(),
                null,
                null,
            )
        }.getOrNull() ?: return WatchNextProviderResult.Failed
        return if (rows > 0) WatchNextProviderResult.Updated else WatchNextProviderResult.Missing
    }

    override fun delete(programId: Long): WatchNextProviderResult {
        val rows = runCatching {
            contentResolver.delete(TvContractCompat.buildWatchNextProgramUri(programId), null, null)
        }.getOrNull() ?: return WatchNextProviderResult.Failed
        return if (rows > 0) WatchNextProviderResult.Deleted else WatchNextProviderResult.Missing
    }

    @SuppressLint("RestrictedApi", "UseKtx")
    private fun program(candidate: WatchNextCandidate): WatchNextProgram {
        val builder = WatchNextProgram.Builder()
            .setType(
                if (candidate.mediaType == "movie") {
                    TvContractCompat.WatchNextPrograms.TYPE_MOVIE
                } else {
                    TvContractCompat.WatchNextPrograms.TYPE_TV_EPISODE
                },
            )
            .setTitle(candidate.title)
            .setIntentUri(Uri.parse(candidate.deepLink))
            .setInternalProviderId(candidate.platformContentId)
            .setContentId(candidate.platformContentId)
            .setLastEngagementTimeUtcMillis(candidate.lastEngagementTimeMs)
            .setWatchNextType(
                when (candidate.kind) {
                    WatchNextKind.Continue -> TvContractCompat.WatchNextPrograms.WATCH_NEXT_TYPE_CONTINUE
                    WatchNextKind.Next -> TvContractCompat.WatchNextPrograms.WATCH_NEXT_TYPE_NEXT
                    WatchNextKind.New -> TvContractCompat.WatchNextPrograms.WATCH_NEXT_TYPE_NEW
                },
            )
        candidate.description?.let(builder::setDescription)
        candidate.posterArtUri?.let { builder.setPosterArtUri(Uri.parse(it)) }
        candidate.seriesId?.let { builder.setSeriesId("duskcue:series:$it") }
        candidate.seasonNumber?.let { builder.setSeasonNumber(it) }
        candidate.episodeNumber?.let { builder.setEpisodeNumber(it) }
        if (candidate.kind == WatchNextKind.Continue) {
            builder
                .setLastPlaybackPositionMillis(candidate.resumePositionMs.toInt())
                .setDurationMillis(candidate.durationMs.toInt())
        }
        return builder.build()
    }
}

internal class WatchNextPublisher(
    private val provider: WatchNextProvider,
    private val store: TvSessionStore,
    private val artwork: WatchNextArtworkStore? = null,
) {
    private val mutex = Mutex()

    suspend fun sync(scope: TvProfileScope, surface: TvSurface) = mutex.withLock {
        val current = store.current()
        val scopeHash = scopeHash(scope.origin, scope.userId, scope.profileId)
        var pendingProgramIds = drainPendingRemovals(current.pending_watch_next_program_ids)
        if (activeScopeHash(current) != scopeHash || surface.platform != "android_tv") {
            persist(
                current,
                current.watch_next_mappings,
                current.watch_next_suppressions,
                current.watch_next_artwork,
                pendingProgramIds,
            )
            return@withLock
        }

        val artworkRecords = current.watch_next_artwork.toMutableList()
        val replacedArtwork = mutableMapOf<String, PersistedWatchNextArtwork>()
        val candidates = WatchNextCandidateFactory.from(surface).map { candidate ->
            val existing = artworkRecords.firstOrNull {
                it.scope_hash == scopeHash && it.platform_content_id == candidate.platformContentId
            }
            val resolution = artwork?.resolve(scope.origin, scopeHash, candidate, existing)
            if (resolution?.record != null && resolution.record != existing) {
                existing?.let {
                    artworkRecords.remove(it)
                    replacedArtwork[candidate.platformContentId] = it
                }
                artworkRecords += resolution.record
            }
            WatchNextCandidateFactory.withPosterArtUri(candidate, resolution?.posterArtUri)
        }
        val plan = WatchNextReconciler.plan(
            scopeHash = scopeHash,
            candidates = candidates,
            mappings = current.watch_next_mappings,
            suppressions = current.watch_next_suppressions,
        )
        val mappings = current.watch_next_mappings.toMutableList()
        val suppressions = current.watch_next_suppressions.toMutableList()
        plan.operations.forEach { operation ->
            when (operation) {
                is WatchNextOperation.Delete -> {
                    when (provider.delete(operation.mapping.program_id)) {
                        WatchNextProviderResult.Deleted, WatchNextProviderResult.Missing -> {
                            mappings.remove(operation.mapping)
                            removeArtwork(artworkRecords, operation.mapping)
                        }
                        else -> Unit
                    }
                }

                is WatchNextOperation.Insert -> {
                    when (val result = provider.insert(operation.candidate)) {
                        is WatchNextProviderResult.Inserted -> {
                            mappings.removeAll {
                                it.scope_hash == scopeHash &&
                                    it.platform_content_id == operation.candidate.platformContentId
                            }
                            mappings += mapping(scopeHash, operation.candidate, result.programId)
                            removeSuppressions(suppressions, scopeHash, operation.candidate.platformContentId)
                            removeReplacedArtwork(replacedArtwork, operation.candidate.platformContentId)
                        }

                        else -> Unit
                    }
                }

                is WatchNextOperation.Update -> {
                    when (provider.update(operation.mapping.program_id, operation.candidate)) {
                        WatchNextProviderResult.Updated -> {
                            mappings.remove(operation.mapping)
                            mappings += mapping(scopeHash, operation.candidate, operation.mapping.program_id)
                            removeSuppressions(suppressions, scopeHash, operation.candidate.platformContentId)
                            removeReplacedArtwork(replacedArtwork, operation.candidate.platformContentId)
                        }

                        WatchNextProviderResult.Missing -> {
                            mappings.remove(operation.mapping)
                            removeArtwork(artworkRecords, operation.mapping)
                            removeReplacedArtwork(replacedArtwork, operation.candidate.platformContentId)
                            removeSuppressions(suppressions, scopeHash, operation.candidate.platformContentId)
                            suppressions += PersistedWatchNextSuppression(
                                scope_hash = scopeHash,
                                platform_content_id = operation.candidate.platformContentId,
                                fingerprint = operation.candidate.sourceFingerprint,
                            )
                        }

                        else -> Unit
                    }
                }
            }
        }
        persist(current, mappings, suppressions, artworkRecords, pendingProgramIds)
    }

    suspend fun clear() = mutex.withLock {
        val current = store.current()
        val pendingProgramIds = drainPendingRemovals(
            current.pending_watch_next_program_ids + current.watch_next_mappings.map(PersistedWatchNextMapping::program_id),
        )
        artwork?.clear()
        persist(current, emptyList(), emptyList(), emptyList(), pendingProgramIds)
    }

    suspend fun handleProgramDisabled(programId: Long) = mutex.withLock {
        val current = store.current()
        val affected = current.watch_next_mappings.filter { it.program_id == programId }
        if (affected.isEmpty()) {
            return@withLock
        }
        val pendingProgramIds = if (
            provider.delete(programId) in setOf(WatchNextProviderResult.Deleted, WatchNextProviderResult.Missing)
        ) {
            current.pending_watch_next_program_ids.filterNot { it == programId }
        } else {
            (current.pending_watch_next_program_ids + programId).distinct()
        }
        val suppressions = current.watch_next_suppressions.toMutableList()
        val artworkRecords = current.watch_next_artwork.toMutableList()
        affected.forEach { mapping ->
            removeSuppressions(suppressions, mapping.scope_hash, mapping.platform_content_id)
            suppressions += PersistedWatchNextSuppression(
                scope_hash = mapping.scope_hash,
                platform_content_id = mapping.platform_content_id,
                fingerprint = mapping.source_fingerprint,
            )
        }
        persist(
            current,
            current.watch_next_mappings.filterNot { it.program_id == programId },
            suppressions,
            artworkRecords.also { records -> affected.forEach { mapping -> removeArtwork(records, mapping) } },
            pendingProgramIds,
        )
    }

    private fun drainPendingRemovals(programIds: List<Long>): List<Long> = programIds
        .distinct()
        .filter { programId ->
            provider.delete(programId) !in setOf(WatchNextProviderResult.Deleted, WatchNextProviderResult.Missing)
        }

    private suspend fun persist(
        current: SecureTvState,
        mappings: List<PersistedWatchNextMapping>,
        suppressions: List<PersistedWatchNextSuppression>,
        artworkRecords: List<PersistedWatchNextArtwork>,
        pendingProgramIds: List<Long>,
    ) {
        if (
            mappings == current.watch_next_mappings &&
            suppressions == current.watch_next_suppressions &&
            artworkRecords == current.watch_next_artwork &&
            pendingProgramIds == current.pending_watch_next_program_ids
        ) {
            return
        }
        store.replace(
            current.copy(
                watch_next_mappings = mappings,
                watch_next_suppressions = suppressions,
                watch_next_artwork = artworkRecords,
                pending_watch_next_program_ids = pendingProgramIds,
            ),
        )
    }

    private fun activeScopeHash(current: SecureTvState): String? = current.session
        ?.takeUnless { it.profile_selection_required }
        ?.let { session -> scopeHash(session.origin, session.user_id, session.active_profile_id) }

    private fun mapping(
        scopeHash: String,
        candidate: WatchNextCandidate,
        programId: Long,
    ): PersistedWatchNextMapping = PersistedWatchNextMapping(
        scope_hash = scopeHash,
        platform_content_id = candidate.platformContentId,
        media_item_id = candidate.mediaItemId,
        series_id = candidate.seriesId,
        surface_item_id = candidate.surfaceItemId,
        program_id = programId,
        fingerprint = candidate.fingerprint,
        source_fingerprint = candidate.sourceFingerprint,
    )

    private fun removeSuppressions(
        suppressions: MutableList<PersistedWatchNextSuppression>,
        scopeHash: String,
        platformContentId: String,
    ) {
        suppressions.removeAll {
            it.scope_hash == scopeHash && it.platform_content_id == platformContentId
        }
    }

    private fun removeArtwork(
        records: MutableList<PersistedWatchNextArtwork>,
        mapping: PersistedWatchNextMapping,
    ) {
        val removed = records.filter {
            it.scope_hash == mapping.scope_hash && it.platform_content_id == mapping.platform_content_id
        }
        artwork?.remove(removed)
        records.removeAll(removed)
    }

    private fun removeReplacedArtwork(
        replaced: MutableMap<String, PersistedWatchNextArtwork>,
        platformContentId: String,
    ) {
        replaced.remove(platformContentId)?.let { artwork?.remove(listOf(it)) }
    }
}

internal fun scopeHash(origin: String, userId: String, profileId: String): String = MessageDigest.getInstance("SHA-256")
    .digest("$origin\u001F$userId\u001F$profileId".toByteArray(Charsets.UTF_8))
    .joinToString(separator = "") { byte -> (byte.toInt() and 0xff).toString(16).padStart(2, '0') }
