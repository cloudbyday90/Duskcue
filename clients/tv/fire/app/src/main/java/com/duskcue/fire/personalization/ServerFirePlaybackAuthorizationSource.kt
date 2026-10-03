package com.duskcue.fire.personalization

import com.duskcue.tv.api.TvFirePlaybackAuthorization
import java.time.Instant

class ServerFirePlaybackAuthorizationSource(
    private val customerOptedIn: () -> Boolean,
    private val request: suspend (String) -> TvFirePlaybackAuthorization?,
    private val nowMs: () -> Long = System::currentTimeMillis,
) : FirePlaybackAuthorizationSource {
    override suspend fun authorize(mediaItemId: String, sessionId: String): FirePlaybackAuthorization? {
        if (!customerOptedIn()) return null
        val response = request(sessionId) ?: return null
        if (!response.eligible || !customerOptedIn() || (response.mapping_revision ?: 0) < 1) return null
        val contentId = response.catalog_content_id ?: return null
        val profileKey = response.opaque_profile_key ?: return null
        val expiry = runCatching { Instant.parse(response.expires_at).toEpochMilli() }.getOrNull() ?: return null
        val now = nowMs()
        if (expiry <= now || expiry - now > 60_000) return null
        return FirePlaybackAuthorization(
            catalogContentId = contentId,
            opaqueProfileKey = profileKey,
            expiresAtMs = expiry,
            integrationEnabled = true,
            customerOptedIn = true,
            profileType = "standard",
            profileSelectionRequired = false,
            playbackMode = "interactive",
            accessRevalidated = true,
        ).takeIf { it.eligible(now) }
    }
}
