// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile

import java.io.File
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class QualificationActivity : MainActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        check(applicationContext.packageName == "com.duskcue.mobile.qualification")
        val bootstrap = File(noBackupFilesDir, "playback-bootstrap.json")
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "duskcue/qualification").setMethodCallHandler { call, result ->
            try {
                when (call.method) {
                    "bootstrap" -> {
                        check(bootstrap.isFile && bootstrap.length() in 1..4096)
                        val data = bootstrap.inputStream().use { it.readNBytes(4097) }
                        check(data.size <= 4096)
                        result.success(String(data, Charsets.UTF_8))
                    }
                    "clearBootstrap" -> {
                        check(!bootstrap.exists() || bootstrap.delete())
                        check(!bootstrap.exists())
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            } catch (_: Exception) { result.error("QUALIFICATION_BOOTSTRAP", "Private qualification bootstrap unavailable", null) }
        }
    }
}
