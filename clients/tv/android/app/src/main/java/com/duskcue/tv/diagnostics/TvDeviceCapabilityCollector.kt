package com.duskcue.tv.diagnostics

import android.content.Context
import android.hardware.display.DisplayManager
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioManager
import android.media.MediaCodecList
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.view.Display

internal object TvDeviceCapabilityCollector {
    fun collect(context: Context, appVersion: String, currentRoute: String): TvDeviceCapabilityReport {
        val display = runCatching {
            context.getSystemService(DisplayManager::class.java)?.getDisplay(Display.DEFAULT_DISPLAY)
        }.getOrNull()
        val connectivity = runCatching {
            context.getSystemService(ConnectivityManager::class.java)
        }.getOrNull()
        val networkCapabilities = connectivity?.activeNetwork?.let { network ->
            runCatching { connectivity.getNetworkCapabilities(network) }.getOrNull()
        }
        val audioDevices = runCatching {
            context.getSystemService(AudioManager::class.java)
                ?.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
                ?.toList()
                .orEmpty()
        }.getOrDefault(emptyList())
        val videoDecoders = advertisedVideoDecoders()
        val hdrTypes = reportedHdrTypes(display)
        val audioEncodings = audioDevices
            .flatMap { device -> device.encodings.toList() }
            .mapNotNull(TvDeviceCapabilityClassifier::audioEncoding)
            .distinct()
            .sorted()
        return TvDeviceCapabilityReport(
            app_version = appVersion,
            codec_support_summary = videoDecoders.joinToString(separator = ",").ifBlank { "not_reported" },
            hdr_audio_subtitle_summary = listOf(
                hdrTypes.joinToString(separator = ",").ifBlank { "hdr_not_reported" },
                audioEncodings.joinToString(separator = ",").ifBlank { "audio_not_reported" },
                "subtitle_selection_in_player",
            ).joinToString(separator = ";"),
            current_route_or_screen = currentRoute,
            device_family = TvDeviceCapabilityClassifier.deviceFamily(Build.MANUFACTURER, Build.MODEL),
            device_model = TvDeviceCapabilityClassifier.safeLabel(Build.MODEL),
            android_release = TvDeviceCapabilityClassifier.safeLabel(Build.VERSION.RELEASE),
            android_api_level = Build.VERSION.SDK_INT,
            display_mode = display?.mode?.let { mode ->
                "${mode.physicalWidth}x${mode.physicalHeight}@${mode.refreshRate.toInt()}hz"
            } ?: "unknown",
            display_hdr_types = hdrTypes,
            video_decoder_mime_types = videoDecoders,
            audio_output_types = audioDevices
                .map { device -> TvDeviceCapabilityClassifier.audioOutputType(device.type) }
                .distinct()
                .sorted(),
            audio_output_encodings = audioEncodings,
            network_connection_class = TvDeviceCapabilityClassifier.networkConnectionClass(
                hasActiveNetwork = networkCapabilities != null,
                hasEthernet = networkCapabilities?.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) == true,
                hasWifi = networkCapabilities?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true,
            ),
            network_metered_if_known = networkCapabilities?.let {
                connectivity?.isActiveNetworkMetered
            },
            vpn_or_proxy_hint = if (networkCapabilities?.hasTransport(NetworkCapabilities.TRANSPORT_VPN) == true) "vpn" else "unknown",
        )
    }

    private fun advertisedVideoDecoders(): List<String> = runCatching {
        VIDEO_MIME_TYPES.filter { (mimeType, _) ->
            MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos.any { codec ->
                !codec.isEncoder && codec.supportedTypes.any { it.equals(mimeType, ignoreCase = true) }
            }
        }.map { (_, label) -> label }
    }.getOrDefault(emptyList())

    @Suppress("DEPRECATION")
    private fun reportedHdrTypes(display: Display?): List<String> = display?.hdrCapabilities?.supportedHdrTypes
        ?.toList()
        ?.mapNotNull(TvDeviceCapabilityClassifier::hdrType)
        ?.distinct()
        .orEmpty()

    private val VIDEO_MIME_TYPES = listOf(
        "video/avc" to "h264",
        "video/hevc" to "hevc",
        "video/x-vnd.on2.vp9" to "vp9",
        "video/av01" to "av1",
    )
}

internal object TvDeviceCapabilityClassifier {
    fun deviceFamily(manufacturer: String?, model: String?): String = when {
        manufacturer.orEmpty().contains("nvidia", ignoreCase = true) && model.orEmpty().contains("shield", ignoreCase = true) -> "nvidia_shield"
        manufacturer.orEmpty().contains("sony", ignoreCase = true) -> "sony_bravia"
        else -> "android_tv"
    }

    fun safeLabel(value: String?): String = value.orEmpty()
        .replace(Regex("[^A-Za-z0-9._-]"), "_")
        .take(64)
        .ifBlank { "unknown" }

    fun hdrType(value: Int): String? = when (value) {
        Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION -> "dolby_vision"
        Display.HdrCapabilities.HDR_TYPE_HDR10 -> "hdr10"
        Display.HdrCapabilities.HDR_TYPE_HLG -> "hlg"
        Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS -> "hdr10_plus"
        else -> null
    }

    fun audioOutputType(type: Int): String = when (type) {
        AudioDeviceInfo.TYPE_HDMI -> "hdmi"
        AudioDeviceInfo.TYPE_HDMI_ARC -> "hdmi_arc"
        AudioDeviceInfo.TYPE_HDMI_EARC -> "hdmi_earc"
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "built_in_speaker"
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "bluetooth"
        else -> "other"
    }

    fun audioEncoding(value: Int): String? = when (value) {
        AudioFormat.ENCODING_PCM_16BIT -> "pcm"
        AudioFormat.ENCODING_AC3 -> "ac3"
        AudioFormat.ENCODING_E_AC3 -> "eac3"
        AudioFormat.ENCODING_E_AC3_JOC -> "eac3_joc"
        AudioFormat.ENCODING_DTS -> "dts"
        AudioFormat.ENCODING_DTS_HD -> "dts_hd"
        AudioFormat.ENCODING_DOLBY_TRUEHD -> "dolby_truehd"
        else -> null
    }

    fun networkConnectionClass(hasActiveNetwork: Boolean, hasEthernet: Boolean, hasWifi: Boolean): String = when {
        !hasActiveNetwork -> "unknown"
        hasEthernet -> "ethernet"
        hasWifi -> "wifi"
        else -> "other"
    }
}
