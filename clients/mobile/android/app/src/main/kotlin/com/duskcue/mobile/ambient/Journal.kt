// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import android.content.Context
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File

internal data class AmbientRecord(
    val scope: AmbientScope,
    val start: String,
    val output: String? = null,
    val operation: String? = null,
    val phase: String = "uncertain",
) {
    fun json() = JSONObject().put("version", 1).put("scope", scope.json()).put("start", start)
        .put("output", output ?: JSONObject.NULL).put("operation", operation ?: JSONObject.NULL).put("phase", phase)
    companion object {
        fun parse(value: JSONObject): AmbientRecord {
            value.exact("version", "scope", "start", "output", "operation", "phase")
            demand(value.number("version") == 1L && value.text("phase") in setOf("active", "uncertain", "cleanup"), "journal_invalid")
            return AmbientRecord(AmbientScope.parse(value.getJSONObject("scope")), value.uuid("start", true),
                value.optionalUuid("output", true), value.optionalUuid("operation", true), value.text("phase"))
        }
    }
}

internal class AmbientJournal(private val path: File, timers: AmbientTimers) {
    constructor(context: Context, timers: AmbientTimers) : this(File(context.noBackupFilesDir, "ambient-command-v1.json"), timers)
    private val lane = NativeLane("ambient-journal", 1, timers)
    private val file = AtomicFile(path)
    @Volatile var failed = false
        private set
    fun pending() = lane.count() > 0
    private fun readActual(): AmbientRecord? {
        val stream = try { file.openRead() } catch (error: java.io.FileNotFoundException) {
            demand(!file.baseFile.exists(), "journal_unavailable")
            return null
        }
        val bytes = stream.use { readLimited(it, 4096) }
        return AmbientRecord.parse(Wire.parse(bytes, 4096))
    }
    fun read() = lane.run {
        try { readActual() } catch (_: Exception) { failed = true; throw AmbientFailure("journal_unavailable") }
    }
    fun write(record: AmbientRecord) = lane.run {
        try {
            val bytes = Wire.bytes(record.json(), 4096)
            AmbientRecord.parse(Wire.parse(bytes, 4096))
            val stream = file.startWrite()
            try {
                stream.write(bytes)
                stream.fd.sync()
                file.finishWrite(stream)
            } catch (error: Exception) { file.failWrite(stream); throw error }
            demand(readActual() == record, "journal_unavailable")
            failed = false
        } catch (_: Exception) { failed = true; throw AmbientFailure("journal_unavailable") }
    }
    fun clear() = lane.run {
        try {
            file.delete()
            demand(!file.baseFile.exists() && readActual() == null, "journal_unavailable")
            failed = false
        } catch (_: Exception) { failed = true; throw AmbientFailure("journal_unavailable") }
    }
    fun drain() = lane.drain()
    fun close() = lane.close()
}
