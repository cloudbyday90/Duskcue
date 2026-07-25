package com.duskcue.tv.watchnext

import android.annotation.SuppressLint
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.net.Uri
import android.os.ParcelFileDescriptor
import com.duskcue.tv.api.BearerTokenProvider
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.session.PersistedWatchNextArtwork
import java.io.File
import java.io.FileNotFoundException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.security.MessageDigest
import java.util.UUID

internal object WatchNextArtworkPolicy {
    const val posterVariant = "w500"
    const val backdropVariant = "w1280"
    const val thumbnailVariant = "w300"
    const val logoVariant = "original"

    fun posterRequestPath(candidate: WatchNextCandidate): String? {
        val raw = candidate.posterUrl ?: return null
        val uri = runCatching { URI(raw) }.getOrNull() ?: return null
        val expectedPath = "/api/v1/items/${candidate.mediaItemId}/artwork/poster"
        if (
            uri.isAbsolute ||
            uri.rawAuthority != null ||
            uri.rawQuery != null ||
            uri.rawFragment != null ||
            uri.path != expectedPath
        ) {
            return null
        }
        return "$expectedPath?size=$posterVariant"
    }

    fun sourceHash(path: String?): String = sha256(path.orEmpty())

    fun fallbackKey(scopeHash: String, platformContentId: String): String = UUID
        .nameUUIDFromBytes("$scopeHash\u001F$platformContentId".toByteArray(Charsets.UTF_8))
        .toString()

    fun sha256(value: String): String = MessageDigest.getInstance("SHA-256")
        .digest(value.toByteArray(Charsets.UTF_8))
        .joinToString(separator = "") { byte -> (byte.toInt() and 0xff).toString(16).padStart(2, '0') }
}

internal data class WatchNextArtworkFetchResponse(
    val status: Int,
    val etag: String? = null,
    val bytes: ByteArray? = null,
)

internal fun interface WatchNextArtworkFetcher {
    fun fetch(origin: ServerOrigin, token: String, path: String, etag: String?): WatchNextArtworkFetchResponse?
}

internal class UrlConnectionWatchNextArtworkFetcher : WatchNextArtworkFetcher {
    override fun fetch(
        origin: ServerOrigin,
        token: String,
        path: String,
        etag: String?,
    ): WatchNextArtworkFetchResponse? = runCatching {
        val base = URI(origin.value)
        val target = base.resolve(path)
        require(target.scheme == base.scheme && target.authority == base.authority)
        val connection = URL(target.toString()).openConnection() as HttpURLConnection
        connection.requestMethod = "GET"
        connection.connectTimeout = 10_000
        connection.readTimeout = 20_000
        connection.instanceFollowRedirects = false
        connection.setRequestProperty("Accept", "image/webp")
        connection.setRequestProperty("Authorization", "Bearer $token")
        etag?.let { connection.setRequestProperty("If-None-Match", it) }
        try {
            val status = connection.responseCode
            if (status == HttpURLConnection.HTTP_NOT_MODIFIED) {
                return@runCatching WatchNextArtworkFetchResponse(status = status, etag = etag)
            }
            if (status !in 200..299 || connection.contentType?.startsWith("image/webp") != true) {
                return@runCatching WatchNextArtworkFetchResponse(status = status)
            }
            val bytes = connection.inputStream.use { input -> readAtMost(input, maxArtworkBytes + 1) }
            if (bytes.size > maxArtworkBytes) {
                return@runCatching WatchNextArtworkFetchResponse(status = status)
            }
            WatchNextArtworkFetchResponse(
                status = status,
                etag = connection.getHeaderField("ETag"),
                bytes = bytes,
            )
        } finally {
            connection.disconnect()
        }
    }.getOrNull()

    private companion object {
        const val maxArtworkBytes = 5 * 1024 * 1024

        fun readAtMost(input: InputStream, limit: Int): ByteArray {
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (output.size() < limit) {
                val read = input.read(buffer, 0, minOf(buffer.size, limit - output.size()))
                if (read < 0) {
                    break
                }
                output.write(buffer, 0, read)
            }
            return output.toByteArray()
        }
    }
}

internal data class WatchNextArtworkResolution(
    val posterArtUri: String?,
    val record: PersistedWatchNextArtwork?,
)

internal interface WatchNextArtworkStore {
    fun resolve(
        scope: String,
        scopeHash: String,
        candidate: WatchNextCandidate,
        existing: PersistedWatchNextArtwork?,
    ): WatchNextArtworkResolution

    fun remove(records: Collection<PersistedWatchNextArtwork>)
    fun clear()
}

internal class AndroidWatchNextArtworkStore(
    context: Context,
    private val tokenProvider: BearerTokenProvider,
    private val fetcher: WatchNextArtworkFetcher = UrlConnectionWatchNextArtworkFetcher(),
) : WatchNextArtworkStore {
    private val cacheDirectory = File(context.applicationContext.cacheDir, "watch-next-artwork")

    override fun resolve(
        scope: String,
        scopeHash: String,
        candidate: WatchNextCandidate,
        existing: PersistedWatchNextArtwork?,
    ): WatchNextArtworkResolution {
        val sourcePath = WatchNextArtworkPolicy.posterRequestPath(candidate)
        val sourceHash = WatchNextArtworkPolicy.sourceHash(sourcePath)
        val reusable = existing?.takeIf {
            it.scope_hash == scopeHash &&
                it.platform_content_id == candidate.platformContentId &&
                it.source_hash == sourceHash &&
                cacheFile(it.cache_key).isFile
        }
        val origin = ServerOrigin.parse(scope).getOrNull()
        val token = tokenProvider.currentToken()
        if (sourcePath != null && origin != null && token != null) {
            when (val response = fetcher.fetch(origin, token, sourcePath, reusable?.etag)) {
                null -> Unit
                else -> when {
                    response.status == HttpURLConnection.HTTP_NOT_MODIFIED && reusable != null -> {
                        return WatchNextArtworkResolution(uriFor(reusable.cache_key), reusable)
                    }

                    response.status in 200..299 && response.bytes != null -> {
                        val cacheKey = UUID.randomUUID().toString()
                        if (write(cacheKey, response.bytes)) {
                            val record = PersistedWatchNextArtwork(
                                scope_hash = scopeHash,
                                platform_content_id = candidate.platformContentId,
                                source_hash = sourceHash,
                                cache_key = cacheKey,
                                etag = response.etag,
                            )
                            return WatchNextArtworkResolution(uriFor(cacheKey), record)
                        }
                    }
                }
            }
        }
        if (reusable != null) {
            return WatchNextArtworkResolution(uriFor(reusable.cache_key), reusable)
        }
        val fallbackKey = WatchNextArtworkPolicy.fallbackKey(scopeHash, candidate.platformContentId)
        if (!cacheFile(fallbackKey).isFile && !write(fallbackKey, fallback(candidate.title, candidate.platformContentId))) {
            return WatchNextArtworkResolution(null, null)
        }
        return WatchNextArtworkResolution(
            posterArtUri = uriFor(fallbackKey),
            record = PersistedWatchNextArtwork(
                scope_hash = scopeHash,
                platform_content_id = candidate.platformContentId,
                source_hash = sourceHash,
                cache_key = fallbackKey,
            ),
        )
    }

    override fun remove(records: Collection<PersistedWatchNextArtwork>) {
        records.forEach { delete(it.cache_key) }
    }

    override fun clear() {
        cacheDirectory.deleteRecursively()
    }

    private fun write(cacheKey: String, bytes: ByteArray): Boolean {
        return runCatching {
            cacheDirectory.mkdirs()
            val target = cacheFile(cacheKey)
            val temporary = File(cacheDirectory, "$cacheKey.tmp")
            temporary.outputStream().use { it.write(bytes) }
            if (!temporary.renameTo(target)) {
                temporary.delete()
                false
            } else {
                true
            }
        }.getOrDefault(false)
    }

    @SuppressLint("UseKtx")
    @Suppress("DEPRECATION")
    private fun fallback(title: String, platformContentId: String): ByteArray {
        val bitmap = Bitmap.createBitmap(500, 750, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val digest = WatchNextArtworkPolicy.sha256(platformContentId)
        canvas.drawColor(Color.rgb(digest.substring(0, 2).toInt(16), digest.substring(2, 4).toInt(16), digest.substring(4, 6).toInt(16)))
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.WHITE
            typeface = Typeface.create(Typeface.SANS_SERIF, Typeface.BOLD)
            textSize = 38f
        }
        val words = title.trim().split(Regex("\\s+")).filter(String::isNotBlank)
        val lines = words.fold(mutableListOf<String>()) { lines, word ->
            val current = lines.lastOrNull().orEmpty()
            val candidate = listOf(current, word).filter(String::isNotBlank).joinToString(" ")
            if (paint.measureText(candidate) > 420f && current.isNotBlank()) {
                lines += word
            } else if (lines.isEmpty()) {
                lines += candidate
            } else {
                lines[lines.lastIndex] = candidate
            }
            lines
        }.take(4)
        val startY = 340f - ((lines.size - 1) * 26f)
        lines.forEachIndexed { index, line -> canvas.drawText(line, 40f, startY + index * 56f, paint) }
        return java.io.ByteArrayOutputStream().use { output ->
            bitmap.compress(Bitmap.CompressFormat.WEBP, 88, output)
            bitmap.recycle()
            output.toByteArray()
        }
    }

    private fun uriFor(cacheKey: String): String = WatchNextArtworkProvider.uriFor(cacheKey).toString()

    private fun delete(cacheKey: String) {
        cacheFile(cacheKey).delete()
    }

    private fun cacheFile(cacheKey: String): File = File(cacheDirectory, "$cacheKey.webp")
}

class WatchNextArtworkProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String? = if (cacheKey(uri) != null) "image/webp" else null

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") {
            throw FileNotFoundException("read-only artwork")
        }
        val cacheKey = cacheKey(uri) ?: throw FileNotFoundException("unknown artwork")
        val file = File(requireNotNull(context).cacheDir, "watch-next-artwork/$cacheKey.webp")
        if (!file.isFile) {
            throw FileNotFoundException("artwork unavailable")
        }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0

    private fun cacheKey(uri: Uri): String? {
        if (uri.authority != authority || uri.pathSegments.size != 2 || uri.pathSegments[0] != "poster") {
            return null
        }
        return uri.pathSegments[1].takeIf { runCatching { UUID.fromString(it) }.isSuccess }
    }

    companion object {
        private const val authority = "com.duskcue.tv.watchnext-artwork"

        @SuppressLint("UseKtx")
        fun uriFor(cacheKey: String): Uri = Uri.parse("content://$authority/poster/$cacheKey")
    }
}
