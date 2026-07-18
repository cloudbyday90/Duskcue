package com.duskcue.tv

import java.net.URI

data class TvDeepLinkRequest(
    val sequence: Long,
    val uri: String?,
)

sealed interface TvDeepLink {
    data class Playback(
        val mediaType: String,
        val mediaItemId: String,
    ) : TvDeepLink {
        val platformContentId: String = "duskcue:$mediaType:$mediaItemId"
    }

    data object Absent : TvDeepLink
    data object Invalid : TvDeepLink

    companion object {
        private val playbackPath = Regex(
            "^/(movie|episode)/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$",
        )

        fun parse(rawUri: String?): TvDeepLink {
            if (rawUri.isNullOrBlank()) return Absent
            val uri = runCatching { URI(rawUri) }.getOrNull() ?: return Invalid
            if (!uri.scheme.equals("duskcue", ignoreCase = true) ||
                !uri.host.equals("play", ignoreCase = true) ||
                uri.userInfo != null ||
                uri.port != -1 ||
                uri.rawQuery != null ||
                uri.rawFragment != null
            ) {
                return Invalid
            }
            val match = playbackPath.matchEntire(uri.rawPath ?: return Invalid) ?: return Invalid
            return Playback(
                mediaType = match.groupValues[1],
                mediaItemId = match.groupValues[2].lowercase(),
            )
        }
    }
}
