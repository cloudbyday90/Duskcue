package com.duskcue.tv.diagnostics

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TvDeviceCapabilityCollectorTest {
    @Test
    fun classifiesShieldWithoutPersistingABuildFingerprint() {
        assertEquals("nvidia_shield", TvDeviceCapabilityClassifier.deviceFamily("NVIDIA", "SHIELD Android TV Pro"))
        assertEquals("sony_bravia", TvDeviceCapabilityClassifier.deviceFamily("Sony", "XR-65X90L"))
        assertEquals("android_tv", TvDeviceCapabilityClassifier.deviceFamily("Google", "Chromecast"))
        assertEquals("SHIELD_TV_Pro", TvDeviceCapabilityClassifier.safeLabel("SHIELD TV Pro"))
        assertEquals("unknown", TvDeviceCapabilityClassifier.safeLabel(null))
    }

    @Test
    fun mapsOnlyAllowedAudioAndDisplayCapabilityValues() {
        assertEquals("hdr10", TvDeviceCapabilityClassifier.hdrType(2))
        assertEquals("eac3", TvDeviceCapabilityClassifier.audioEncoding(6))
        assertNull(TvDeviceCapabilityClassifier.audioEncoding(-1))
    }

    @Test
    fun classifiesNetworkTransportConservatively() {
        assertEquals("unknown", TvDeviceCapabilityClassifier.networkConnectionClass(hasActiveNetwork = false, hasEthernet = false, hasWifi = false))
        assertEquals("ethernet", TvDeviceCapabilityClassifier.networkConnectionClass(hasActiveNetwork = true, hasEthernet = true, hasWifi = true))
        assertEquals("wifi", TvDeviceCapabilityClassifier.networkConnectionClass(hasActiveNetwork = true, hasEthernet = false, hasWifi = true))
    }
}
