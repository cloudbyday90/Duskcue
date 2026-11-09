// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

internal class AmbientPlaylists(private val prefix: String, private val current: () -> Boolean) {
    private val references = mutableMapOf<String, Set<String>>()
    private var revision = 0L
    private var closed = false
    @Synchronized fun permits(path: String) = !closed && current() &&
        (path == "manifest.m3u8" || references.values.any { path in it })
    @Synchronized fun revision() = revision
    @Synchronized fun clear() { closed = true; references.clear(); revision++ }
    @Synchronized fun validate(bytes: ByteArray, path: String, expected: Long): ByteArray {
        demand(permits(path) && (path == "manifest.m3u8" || expected == revision), "stale_output")
        demand(bytes.size <= 1048576, "playlist_limit")
        val text = Wire.utf8(bytes)
        demand(text.startsWith("#EXTM3U\n") && text.endsWith('\n') && '\r' !in text && '\u0000' !in text)
        val lines = text.split('\n')
        val root = path == "manifest.m3u8"
        val master = lines.any { it.startsWith("#EXT-X-STREAM-INF:") }
        demand(!master || root)
        var rendition: String? = if (root) null else path.substringBefore('/')
        val refs = mutableSetOf<String>()
        val tags = mutableSetOf<String>()
        var variants = 0
        var awaitingVariant = false
        var awaitingSegment = false
        var mapped = false
        var ended = false
        fun reference(value: String): String {
            demand(value.startsWith(prefix))
            val suffix = value.removePrefix(prefix)
            if (master) demand(playlist.matches(suffix)) else {
                demand(binary.matches(suffix))
                if (rendition == null) rendition = suffix.substringBefore('/')
                demand(rendition == suffix.substringBefore('/'))
            }
            demand(refs.add(suffix) && refs.size <= 8192, "playlist_limit")
            return suffix
        }
        for (line in lines) {
            demand(line.length <= 8192 && line == line.trim())
            if (line.isEmpty()) continue
            if (!line.startsWith('#')) {
                if (master) { demand(awaitingVariant && ++variants <= 16); awaitingVariant = false }
                else { demand(awaitingSegment && mapped && !ended && line.endsWith(".m4s")); awaitingSegment = false }
                reference(line)
                continue
            }
            val tag = line.substringBefore(':')
            demand(!ended && !awaitingVariant && !awaitingSegment)
            if (tag !in setOf("#EXTINF", "#EXT-X-STREAM-INF")) demand(tags.add(tag))
            when (tag) {
                "#EXT-X-MAP" -> {
                    demand(!master && !mapped && refs.isEmpty())
                    val match = Regex("#EXT-X-MAP:URI=\"([^\"]+)\"").matchEntire(line) ?: throw AmbientFailure("invalid_playlist")
                    demand(reference(match.groupValues[1]).endsWith("/init.mp4"))
                    mapped = true
                }
                "#EXT-X-STREAM-INF" -> {
                    demand(master && line.startsWith("$tag:"))
                    variant(line.substring(tag.length + 1))
                    awaitingVariant = true
                }
                "#EXTINF" -> {
                    demand(!master && mapped && Regex("#EXTINF:[0-9]{1,5}(?:\\.[0-9]{1,9})?,").matches(line))
                    demand(line.substring(8, line.length - 1).toDouble() in 0.000000001..86400.0)
                    awaitingSegment = true
                }
                "#EXT-X-ENDLIST" -> { demand(!master && line == tag && refs.size > 1); ended = true }
                "#EXT-X-VERSION" -> demand(Regex("#EXT-X-VERSION:[1-9][0-9]?").matches(line))
                "#EXT-X-TARGETDURATION" -> demand(!master && Regex("#EXT-X-TARGETDURATION:[1-9][0-9]{0,4}").matches(line) && line.substringAfter(':').toLong() <= 86400)
                "#EXT-X-MEDIA-SEQUENCE" -> demand(!master && Regex("#EXT-X-MEDIA-SEQUENCE:[0-9]{1,10}").matches(line))
                "#EXT-X-PLAYLIST-TYPE" -> demand(!master && line in setOf("$tag:EVENT", "$tag:VOD"))
                else -> demand(line in setOf("#EXTM3U", "#EXT-X-INDEPENDENT-SEGMENTS"))
            }
        }
        demand(!awaitingSegment && !awaitingVariant && if (master) variants > 0 else mapped && refs.size > 1 && "#EXT-X-TARGETDURATION" in tags)
        demand(!text.contains("#EXT-X-PLAYLIST-TYPE:VOD") || ended)
        val next = references.toMutableMap()
        next[path] = refs.toSet()
        if (root) next.keys.filter { it != path && it !in refs }.forEach(next::remove)
        demand(next.values.sumOf { it.size } <= 8192, "playlist_limit")
        demand(current() && !closed, "stale_output")
        references.clear(); references.putAll(next)
        if (root) revision++
        return bytes
    }
    companion object {
        val binary = Regex("[A-Za-z0-9_-]{1,64}/(?:init\\.mp4|seg_[0-9]{1,50}\\.m4s)")
        val playlist = Regex("[A-Za-z0-9_-]{1,64}/index\\.m3u8")
        fun variant(source: String) {
            val fields = mutableMapOf<String, String>()
            val pattern = Regex("([A-Z-]+)=(\"[^\"\\r\\n]*\"|[^,\\r\\n]+)(,|$)")
            var offset = 0
            while (offset < source.length) {
                val match = pattern.find(source, offset) ?: throw AmbientFailure("invalid_playlist")
                demand(match.range.first == offset && !fields.containsKey(match.groupValues[1]))
                fields[match.groupValues[1]] = match.groupValues[2]
                offset = match.range.last + 1
                demand(offset != source.length || match.groupValues[3].isEmpty())
            }
            demand(fields.keys.all { it in setOf("BANDWIDTH", "RESOLUTION", "CODECS") })
            demand(Regex("[1-9][0-9]{0,9}").matches(fields["BANDWIDTH"].orEmpty()))
            fields["RESOLUTION"]?.let { value ->
                demand(Regex("[1-9][0-9]{0,4}x[1-9][0-9]{0,4}").matches(value))
                demand(value.substringBefore('x').toInt() <= 1920 && value.substringAfter('x').toInt() <= 1080, "unqualified_output")
            }
            fields["CODECS"]?.let { demand(Regex("\"avc1\\.[0-9a-fA-F]{6},mp4a\\.40\\.2\"").matches(it), "unqualified_output") }
        }
    }
}
