// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile

import java.io.File
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterShellArgs
import io.flutter.plugin.common.MethodChannel
import org.json.JSONObject

class QualificationActivity : MainActivity() {
    private fun privateBootstrap(): String {
        val bootstrap = File(noBackupFilesDir, "playback-bootstrap.json")
        check(bootstrap.isFile && bootstrap.length() in 1..4096)
        val data = bootstrap.inputStream().use { it.readNBytes(4097) }
        check(data.size <= 4096)
        return String(data, Charsets.UTF_8)
    }

    override fun getFlutterShellArgs(): FlutterShellArgs {
        val args = super.getFlutterShellArgs()
        val port = JSONObject(privateBootstrap()).getInt("vm_service_port")
        check(port in 1024..65535)
        args.add("--vm-service-port=$port")
        return args
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        check(applicationContext.packageName == "com.duskcue.mobile.qualification")
        val bootstrap = File(noBackupFilesDir, "playback-bootstrap.json")
        val storage = QualificationStorage(applicationContext)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "duskcue/qualification").setMethodCallHandler { call, result ->
            try {
                when (call.method) {
                    "bootstrap" -> {
                        result.success(privateBootstrap())
                    }
                    "clearBootstrap" -> {
                        check(!bootstrap.exists() || bootstrap.delete())
                        check(!bootstrap.exists())
                        result.success(null)
                    }
                    "storageProbe" -> result.success(storage.probe())
                    "storageCorrupt" -> result.success(storage.corrupt())
                    "storageRestore" -> { storage.restore(); result.success(null) }
                    else -> result.notImplemented()
                }
            } catch (_: Exception) { result.error("QUALIFICATION_BOOTSTRAP", "Private qualification bootstrap unavailable", null) }
        }
    }
}
