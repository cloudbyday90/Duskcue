package com.duskcue.mobile

import java.io.File
import java.security.MessageDigest
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.android.FlutterActivity
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private val storageChannel = "duskcue/mobile_storage"

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, storageChannel).setMethodCallHandler { call, result ->
            try {
                when (call.method) {
                    "prepareDownloadScope" -> {
                        val scopeKey = call.argument<String>("scope_key") ?: ""
                        result.success(locationMap(scopeDir(scopeKey, create = true)))
                    }
                    "prepareDownloadPackage" -> {
                        val scopeKey = call.argument<String>("scope_key") ?: ""
                        val packageKey = call.argument<String>("package_key") ?: ""
                        result.success(locationMap(packageDir(scopeKey, packageKey, create = true)))
                    }
                    "deleteDownloadPackage" -> {
                        val scopeKey = call.argument<String>("scope_key") ?: ""
                        val packageKey = call.argument<String>("package_key") ?: ""
                        packageDir(scopeKey, packageKey, create = false).deleteRecursively()
                        result.success(null)
                    }
                    "deleteDownloadScope" -> {
                        val scopeKey = call.argument<String>("scope_key") ?: ""
                        scopeDir(scopeKey, create = false).deleteRecursively()
                        result.success(null)
                    }
                    "deleteAllDownloads" -> {
                        downloadsRoot().deleteRecursively()
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            } catch (error: Exception) {
                result.error("download_storage_failed", error.message, null)
            }
        }
    }

    private fun downloadsRoot(): File {
        return File(noBackupFilesDir, "duskcue_downloads")
    }

    private fun scopeDir(scopeKey: String, create: Boolean): File {
        val dir = File(downloadsRoot(), digest(scopeKey))
        if (create) dir.mkdirs()
        return dir
    }

    private fun packageDir(scopeKey: String, packageKey: String, create: Boolean): File {
        val dir = File(scopeDir(scopeKey, create), digest(packageKey))
        if (create) dir.mkdirs()
        return dir
    }

    private fun locationMap(dir: File): Map<String, Any> {
        return mapOf(
            "path" to dir.absolutePath,
            "platform" to "android",
            "backup_excluded" to true,
            "protection" to "app_private_no_backup"
        )
    }

    private fun digest(value: String): String {
        val bytes = MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8))
        return bytes.joinToString("") { "%02x".format(it) }
    }
}
