// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.util.JsonReader
import android.util.JsonToken
import org.json.JSONArray
import org.json.JSONObject
import java.io.StringReader
import java.io.InputStream
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.time.Instant
import java.util.UUID

internal const val PROTOCOL = "duskcue-vod-command/2"

internal class AmbientFailure(val code: String, val status: Int = 0, val retryMs: Long = 0, val problemCode: String? = null) : Exception(code)

internal fun ambientFailure(error: Throwable): AmbientFailure? {
    var cause: Throwable? = error
    var result: AmbientFailure? = null
    repeat(8) { if (cause is AmbientFailure) result = cause as AmbientFailure; cause = cause?.cause }
    return result
}

internal fun demand(value: Boolean, code: String = "invalid_response") {
    if (!value) throw AmbientFailure(code)
}

internal fun entity(value: String, v7: Boolean = false): String {
    demand(Regex("[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[89ab][a-f0-9]{3}-[a-f0-9]{12}").matches(value))
    demand(!v7 || UUID.fromString(value).version() == 7)
    return value
}

internal fun JSONObject.text(key: String, max: Int = 2048): String {
    val value = opt(key)
    demand(value is String && value.length in 1..max && value.none { it.code < 32 })
    return value as String
}

internal fun JSONObject.number(key: String, max: Long = 2147483647): Long {
    val value = opt(key)
    demand(value is Long || value is Int)
    val result = (value as Number).toLong()
    demand(result in 0..max)
    return result
}

internal fun JSONObject.flag(key: String): Boolean {
    demand(opt(key) is Boolean)
    return getBoolean(key)
}

internal fun JSONObject.uuid(key: String, v7: Boolean = false) = entity(text(key, 36), v7)
internal fun JSONObject.optionalUuid(key: String, v7: Boolean = false) = if (isNull(key)) null else uuid(key, v7)
internal fun JSONObject.instant(key: String): Long {
    val value = text(key, 64)
    demand(value.endsWith("Z"))
    return try { Instant.parse(value).toEpochMilli() } catch (_: Exception) { throw AmbientFailure("invalid_response") }
}

internal fun JSONObject.exact(vararg keys: String) {
    demand(keys().asSequence().toSet() == keys.toSet())
}

internal fun readLimited(input: InputStream, limit: Int): ByteArray {
    val result = ByteArrayOutputStream(minOf(limit, 8192))
    val chunk = ByteArray(minOf(limit + 1, 8192))
    while (true) {
        val size = input.read(chunk, 0, minOf(chunk.size, limit + 1 - result.size()))
        if (size == -1) return result.toByteArray()
        demand(size > 0 && result.size() + size <= limit, "body_limit")
        result.write(chunk, 0, size)
    }
}

internal object Wire {
    fun utf8(bytes: ByteArray): String = try {
        Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
    } catch (_: Exception) { throw AmbientFailure("invalid_encoding") }

    fun parse(bytes: ByteArray, limit: Int = 32768): JSONObject {
        demand(bytes.size <= limit, "body_limit")
        val reader = JsonReader(StringReader(utf8(bytes)))
        var nodes = 0
        fun value(depth: Int): Any {
            demand(depth <= 16 && ++nodes <= 4096, "json_limit")
            return when (reader.peek()) {
                JsonToken.BEGIN_OBJECT -> {
                    val result = JSONObject()
                    reader.beginObject()
                    while (reader.hasNext()) {
                        val key = reader.nextName()
                        demand(key.length <= 128 && !result.has(key), "invalid_json")
                        result.put(key, value(depth + 1))
                    }
                    reader.endObject()
                    result
                }
                JsonToken.BEGIN_ARRAY -> {
                    val result = JSONArray()
                    reader.beginArray()
                    while (reader.hasNext()) result.put(value(depth + 1))
                    reader.endArray()
                    result
                }
                JsonToken.STRING -> reader.nextString().also { demand(it.length <= 8192, "json_limit") }
                JsonToken.BOOLEAN -> reader.nextBoolean()
                JsonToken.NULL -> { reader.nextNull(); JSONObject.NULL }
                JsonToken.NUMBER -> {
                    val raw = reader.nextString()
                    demand(raw.length <= 128 && Regex("-?(0|[1-9][0-9]*)(\\.[0-9]+)?([eE][+-]?[0-9]+)?").matches(raw), "invalid_json")
                    raw.toLongOrNull() ?: raw.toDouble().also { demand(it.isFinite(), "invalid_json") }
                }
                else -> throw AmbientFailure("invalid_json")
            }
        }
        return try {
            reader.isLenient = false
            val result = value(0)
            demand(result is JSONObject && reader.peek() == JsonToken.END_DOCUMENT, "invalid_json")
            result as JSONObject
        } catch (error: AmbientFailure) { throw error }
        catch (_: Exception) { throw AmbientFailure("invalid_json") }
        finally { reader.close() }
    }

    fun bytes(value: JSONObject, limit: Int = 32768): ByteArray = value.toString().toByteArray(Charsets.UTF_8)
        .also { demand(it.size <= limit, "body_limit") }
}
