package com.duskcue.tv.home

import com.duskcue.tv.api.ApiResult
import com.duskcue.tv.api.DuskcueApiClient
import com.duskcue.tv.api.EtagStore
import com.duskcue.tv.api.ServerSentEvent
import com.duskcue.tv.api.TvSurface
import com.duskcue.tv.api.tvSurfaceChangedHint
import com.duskcue.tv.session.TvLocalStateCleaner

data class TvProfileScope(
    val origin: String,
    val userId: String,
    val profileId: String,
) {
    init {
        require(origin.isNotBlank())
        require(userId.isNotBlank())
        require(profileId.isNotBlank())
    }

    val cacheKey: String = "$origin:$userId:$profileId"
}

sealed interface TvHomeLoadState {
    data object Loading : TvHomeLoadState
    data class Ready(val surface: TvSurface, val stale: Boolean = false) : TvHomeLoadState
    data class Failure(val title: String, val retryable: Boolean = true) : TvHomeLoadState
    data object SessionExpired : TvHomeLoadState
}

interface TvSurfaceCache {
    fun read(scope: TvProfileScope): TvSurface?
    fun write(scope: TvProfileScope, surface: TvSurface)
    fun clearProfileScope()
    fun clearIdentityScope()
}

class MemoryTvSurfaceCache : TvSurfaceCache {
    private val surfaces = mutableMapOf<String, TvSurface>()

    override fun read(scope: TvProfileScope): TvSurface? = surfaces[scope.cacheKey]

    override fun write(scope: TvProfileScope, surface: TvSurface) {
        surfaces[scope.cacheKey] = surface
    }

    override fun clearProfileScope() {
        surfaces.clear()
    }

    override fun clearIdentityScope() {
        surfaces.clear()
    }
}

class TvLivingRoomStore(
    private val cache: TvSurfaceCache = MemoryTvSurfaceCache(),
    private val etags: EtagStore,
) : TvLocalStateCleaner {
    fun load(client: DuskcueApiClient, scope: TvProfileScope): TvHomeLoadState {
        val cached = cache.read(scope)
        return when (val result = client.tvSurface(limit = 8, cacheScope = scope.cacheKey)) {
            is ApiResult.Success -> {
                cache.write(scope, result.value)
                TvHomeLoadState.Ready(result.value)
            }
            ApiResult.NotModified -> cached?.let { TvHomeLoadState.Ready(it) }
                ?: TvHomeLoadState.Failure("The TV home view needs a fresh refresh.")
            ApiResult.NetworkFailure -> cached?.let { TvHomeLoadState.Ready(it, stale = true) }
                ?: TvHomeLoadState.Failure("Duskcue could not reach this server.")
            is ApiResult.Failure -> when (result.status) {
                401 -> TvHomeLoadState.SessionExpired
                else -> cached?.let { TvHomeLoadState.Ready(it, stale = true) }
                    ?: TvHomeLoadState.Failure(result.problem.title ?: "The TV home view is unavailable.")
            }
        }
    }

    fun shouldRefresh(event: ServerSentEvent, scope: TvProfileScope): Boolean =
        event.tvSurfaceChangedHint()?.user_id == scope.userId

    override suspend fun clearProfileScope() {
        cache.clearProfileScope()
        etags.clear()
    }

    override suspend fun clearIdentityScope() {
        cache.clearIdentityScope()
        etags.clear()
    }
}
