package com.duskcue.tv.watchnext

import com.duskcue.tv.api.TvSection
import com.duskcue.tv.api.TvSurface
import com.duskcue.tv.home.TvProfileScope
import com.duskcue.tv.session.PersistedAccountSession
import com.duskcue.tv.session.PersistedWatchNextArtwork
import com.duskcue.tv.session.PersistedWatchNextMapping
import com.duskcue.tv.session.SecureTvState
import com.duskcue.tv.session.TvSessionStore
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WatchNextPublisherTest {
    @Test
    fun selects_only_eligible_watch_next_content_and_keeps_one_episode_per_series() {
        val base = surface()
        val nextUp = base.sections.first { it.section_type == "next_up" }.items.single()
        val duplicateContinue = nextUp.copy(
            surface_item_id = "continue:55555555-5555-4555-8555-555555555555",
            platform_content_id = "duskcue:episode:55555555-5555-4555-8555-555555555555",
            media_item_id = "55555555-5555-4555-8555-555555555555",
            section_type = "continue",
            resume_position_ms = 600_000,
            deep_link = "duskcue://play/episode/55555555-5555-4555-8555-555555555555",
        )
        val duplicateSurface = base.copy(
            sections = base.sections.map { section ->
                if (section.section_type == "continue") section.copy(items = section.items + duplicateContinue) else section
            },
        )

        val candidates = WatchNextCandidateFactory.from(duplicateSurface)

        assertEquals(2, candidates.size)
        assertEquals(WatchNextKind.Continue, candidates.single { it.mediaType == "movie" }.kind)
        assertEquals(WatchNextKind.Continue, candidates.single { it.mediaType == "episode" }.kind)
        assertTrue(candidates.none { it.kind == WatchNextKind.New })
    }

    @Test
    fun plans_only_changed_rows_for_update() {
        val candidates = WatchNextCandidateFactory.from(surface())
        val scope = scope()
        val scopeHash = scopeHash(scope.origin, scope.userId, scope.profileId)
        val mappings = candidates.mapIndexed { index, candidate ->
            mapping(scopeHash, candidate, index.toLong() + 1)
        }
        val changed = candidates[1].copy(fingerprint = "changed")

        val plan = WatchNextReconciler.plan(scopeHash, listOf(candidates[0], changed), mappings, emptyList())

        assertEquals(1, plan.operations.size)
        assertTrue(plan.operations.single() is WatchNextOperation.Update)
    }

    @Test
    fun suppresses_a_missing_system_row_until_the_source_changes() = runBlocking {
        val source = movieOnlySurface()
        val candidate = WatchNextCandidateFactory.from(source).single()
        val scope = scope()
        val scopeHash = scopeHash(scope.origin, scope.userId, scope.profileId)
        val staleMapping = mapping(scopeHash, candidate, 7).copy(fingerprint = "stale")
        val store = MemorySessionStore(state(scope, listOf(staleMapping)))
        val provider = FakeProvider(updateResult = WatchNextProviderResult.Missing)
        val publisher = WatchNextPublisher(provider, store)

        publisher.sync(scope, source)
        publisher.sync(scope, source)

        assertTrue(store.value.watch_next_mappings.isEmpty())
        assertEquals(1, store.value.watch_next_suppressions.size)
        assertTrue(provider.inserted.isEmpty())

        val changed = source.copy(
            sections = source.sections.map { section ->
                section.copy(items = section.items.map { item -> item.copy(title = "Moonlit Harbor Redux") })
            },
        )
        publisher.sync(scope, changed)

        assertEquals(1, provider.inserted.size)
        assertEquals(1, store.value.watch_next_mappings.size)
        assertTrue(store.value.watch_next_suppressions.isEmpty())
    }

    @Test
    fun profile_cleanup_deletes_provider_rows_and_forgets_all_profile_state() = runBlocking {
        val candidate = WatchNextCandidateFactory.from(movieOnlySurface()).single()
        val scope = scope()
        val scopeHash = scopeHash(scope.origin, scope.userId, scope.profileId)
        val store = MemorySessionStore(state(scope, listOf(mapping(scopeHash, candidate, 42))))
        val provider = FakeProvider(deleteResult = WatchNextProviderResult.Deleted)

        WatchNextPublisher(provider, store).clear()

        assertEquals(listOf(42L), provider.deleted)
        assertTrue(store.value.watch_next_mappings.isEmpty())
        assertTrue(store.value.watch_next_suppressions.isEmpty())
        assertTrue(store.value.pending_watch_next_program_ids.isEmpty())
    }

    @Test
    fun disabled_system_program_is_deleted_and_suppressed_until_the_source_changes() = runBlocking {
        val source = movieOnlySurface()
        val candidate = WatchNextCandidateFactory.from(source).single()
        val scope = scope()
        val scopeHash = scopeHash(scope.origin, scope.userId, scope.profileId)
        val store = MemorySessionStore(state(scope, listOf(mapping(scopeHash, candidate, 58))))
        val provider = FakeProvider(deleteResult = WatchNextProviderResult.Missing)
        val publisher = WatchNextPublisher(provider, store)

        publisher.handleProgramDisabled(58)
        publisher.sync(scope, source)

        assertEquals(listOf(58L), provider.deleted)
        assertTrue(store.value.watch_next_mappings.isEmpty())
        assertEquals(1, store.value.watch_next_suppressions.size)
        assertTrue(provider.inserted.isEmpty())
    }

    @Test
    fun accepts_only_a_canonical_relative_poster_path_at_the_tv_artwork_size() {
        val candidate = WatchNextCandidateFactory.from(movieOnlySurface()).single()
        val expected = "/api/v1/items/${candidate.mediaItemId}/artwork/poster?size=w500"

        assertEquals(expected, WatchNextArtworkPolicy.posterRequestPath(candidate))
        assertNull(WatchNextArtworkPolicy.posterRequestPath(candidate.copy(posterUrl = "https://example.invalid/poster")))
        assertNull(WatchNextArtworkPolicy.posterRequestPath(candidate.copy(posterUrl = "${candidate.posterUrl}?token=secret")))
        assertNull(WatchNextArtworkPolicy.posterRequestPath(candidate.copy(posterUrl = "/api/v1/items/other/artwork/poster")))
    }

    @Test
    fun changes_the_provider_row_only_when_the_local_artwork_uri_changes() = runBlocking {
        val source = movieOnlySurface()
        val scope = scope()
        val store = MemorySessionStore(state(scope, emptyList()))
        val provider = FakeProvider()
        val artwork = FakeArtworkStore()
        val publisher = WatchNextPublisher(provider, store, artwork)

        publisher.sync(scope, source)
        publisher.sync(scope, source)

        assertEquals(1, provider.inserted.size)
        assertTrue(provider.updated.isEmpty())
        assertEquals(1, store.value.watch_next_artwork.size)

        artwork.cacheKey = "77777777-7777-4777-8777-777777777777"
        publisher.sync(scope, source)

        assertEquals(1, provider.updated.size)
        assertEquals("content://com.duskcue.tv.watchnext-artwork/poster/77777777-7777-4777-8777-777777777777", provider.updated.single().posterArtUri)
    }

    private fun movieOnlySurface(): TvSurface = surface().copy(
        sections = listOf(surface().sections.first { it.section_type == "continue" }),
    )

    private fun surface(): TvSurface = json.decodeFromString<SurfaceFixture>(fixture("surface-contract.json")).body

    private fun scope(): TvProfileScope = TvProfileScope(
        origin = "https://duskcue.example:48027",
        userId = "11111111-1111-4111-8111-111111111111",
        profileId = "22222222-2222-4222-8222-222222222222",
    )

    private fun state(scope: TvProfileScope, mappings: List<PersistedWatchNextMapping>): SecureTvState = SecureTvState(
        device_id = "device-id",
        session = PersistedAccountSession(
            origin = scope.origin,
            token = "token",
            user_id = scope.userId,
            username = "user",
            display_name = "User",
            role = "user",
            active_profile_id = scope.profileId,
            profile_selection_required = false,
        ),
        watch_next_mappings = mappings,
    )

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
    )

    private fun fixture(name: String): String = requireNotNull(javaClass.classLoader?.getResource(name))
        .openStream()
        .bufferedReader()
        .use { it.readText() }

    private class MemorySessionStore(initial: SecureTvState) : TvSessionStore {
        var value = initial

        override suspend fun current(): SecureTvState = value

        override suspend fun replace(value: SecureTvState) {
            this.value = value
        }
    }

    private class FakeProvider(
        private val updateResult: WatchNextProviderResult = WatchNextProviderResult.Updated,
        private val deleteResult: WatchNextProviderResult = WatchNextProviderResult.Deleted,
    ) : WatchNextProvider {
        val inserted = mutableListOf<WatchNextCandidate>()
        val updated = mutableListOf<WatchNextCandidate>()
        val deleted = mutableListOf<Long>()

        override fun insert(candidate: WatchNextCandidate): WatchNextProviderResult {
            inserted += candidate
            return WatchNextProviderResult.Inserted(inserted.size.toLong())
        }

        override fun update(programId: Long, candidate: WatchNextCandidate): WatchNextProviderResult {
            updated += candidate
            return updateResult
        }

        override fun delete(programId: Long): WatchNextProviderResult {
            deleted += programId
            return deleteResult
        }
    }

    private class FakeArtworkStore : WatchNextArtworkStore {
        var cacheKey = "66666666-6666-4666-8666-666666666666"

        override fun resolve(
            scope: String,
            scopeHash: String,
            candidate: WatchNextCandidate,
            existing: PersistedWatchNextArtwork?,
        ): WatchNextArtworkResolution = WatchNextArtworkResolution(
            posterArtUri = "content://com.duskcue.tv.watchnext-artwork/poster/$cacheKey",
            record = PersistedWatchNextArtwork(
                scope_hash = scopeHash,
                platform_content_id = candidate.platformContentId,
                source_hash = WatchNextArtworkPolicy.sourceHash(candidate.posterUrl),
                cache_key = cacheKey,
                etag = "\"$cacheKey\"",
            ),
        )

        override fun remove(records: Collection<PersistedWatchNextArtwork>) = Unit

        override fun clear() = Unit
    }

    @Serializable
    private data class SurfaceFixture(val body: TvSurface)

    private companion object {
        val json = Json { ignoreUnknownKeys = true }
    }
}
