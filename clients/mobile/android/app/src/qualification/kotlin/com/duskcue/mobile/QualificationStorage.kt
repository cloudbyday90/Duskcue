// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile

import android.content.Context
import java.io.File
import java.security.MessageDigest

class QualificationStorage(context: Context) {
    private val preferences = context.getSharedPreferences("duskcue.vod.journal.v1", Context.MODE_PRIVATE)
    private val backup = File(context.noBackupFilesDir, "journal-ciphertext.backup")
    private val key = "duskcue_vod_journal_v1_record"

    fun probe(): Map<String, Any> {
        val value = preferences.getString(key, null)
        check(value != null && value.length in 1..16384)
        val hash = MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8))
        return mapOf("sha256" to hash.joinToString("") { "%02x".format(it) }, "backed_up" to backup.isFile)
    }

    fun corrupt(): Map<String, Any> {
        check(!backup.exists())
        val value = preferences.getString(key, null)
        check(value != null && value.length in 1..16384)
        backup.outputStream().use { it.write(value.toByteArray(Charsets.UTF_8)) }
        check(backup.length() in 1..16384)
        check(preferences.edit().putString(key, "invalid-ciphertext*").commit())
        return probe()
    }

    fun restore() {
        check(backup.isFile && backup.length() in 1..16384)
        val value = backup.inputStream().use { it.readNBytes(16385) }
        check(value.size <= 16384)
        check(preferences.edit().putString(key, String(value, Charsets.UTF_8)).commit())
        check(backup.delete() && !backup.exists())
    }
}
