package com.duskcue.fire

import android.content.Context
import com.duskcue.tv.diagnostics.TvDeviceCapabilityCollector
import com.duskcue.tv.diagnostics.TvDeviceCapabilityReport

internal object FireTvDeviceCapabilityCollector {
    fun collect(context: Context, appVersion: String, currentRoute: String): TvDeviceCapabilityReport =
        TvDeviceCapabilityCollector.collect(context, appVersion, currentRoute).copy(
            platform = "fire_tv",
            device_family = "fire_tv",
        )
}
