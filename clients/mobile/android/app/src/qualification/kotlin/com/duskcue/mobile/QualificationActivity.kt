// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile

import java.io.File
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterShellArgs
import io.flutter.plugin.common.MethodChannel
import org.json.JSONObject
import com.duskcue.mobile.ambient.AmbientNativeProbe
import com.duskcue.mobile.ambient.AmbientFailure
import com.duskcue.mobile.ambient.AmbientNotificationProbe
import android.os.Handler
import android.os.Looper
import java.util.concurrent.atomic.AtomicBoolean

class QualificationActivity : MainActivity() {
    private val ambientProbePending = AtomicBoolean()
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
        val notifications = AmbientNotificationProbe(applicationContext)
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
                    "ambientFixtureCa" -> result.success(resources.openRawResource(R.raw.ambient_fixture_ca).use { it.readNBytes(4096) })
                    "ambientNotificationCapture" -> {
                        val args = call.arguments as Map<*, *>
                        result.success(notifications.capture(args["start"] as String, args["output"] as String))
                    }
                    "ambientNotificationSend" -> {
                        val args = call.arguments as Map<*, *>
                        notifications.send(args["index"] as Int, args["dismiss"] as Boolean).whenComplete { _, error ->
                            if (error == null) result.success(null)
                            else result.error((error as? AmbientFailure)?.code ?: "NOTIFICATION_DISPATCH", "Native notification dispatch unconfirmed", null)
                        }
                    }
                    "ambientNotificationClear" -> { notifications.clear(); result.success(null) }
                    "ambientProbe" -> {
                        check(ambientProbePending.compareAndSet(false, true))
                        Thread({
                            val outcome = runCatching { AmbientNativeProbe(applicationContext).run(JSONObject(privateBootstrap())) }
                            Handler(Looper.getMainLooper()).post {
                                ambientProbePending.set(false)
                                outcome.fold({ result.success(it) }, { result.error((it as? AmbientFailure)?.code ?: "AMBIENT_PROBE", "Native ambient probe failed", null) })
                            }
                        }, "ambient-probe-owner").start()
                    }
                    else -> result.notImplemented()
                }
            } catch (_: Exception) { result.error("QUALIFICATION_BOOTSTRAP", "Private qualification bootstrap unavailable", null) }
        }
    }
}
