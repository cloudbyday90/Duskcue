package com.duskcue.fire

import com.duskcue.tv.api.TvDeviceProfile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FireTvTargetPolicyTest {
    @Test
    fun supportsOnlyFireOsSevenAndNewer() {
        assertEquals("fire_tv", FireTvTargetPolicy.platform)
        assertEquals("app_local_only", FireTvTargetPolicy.integrationState)
        assertFalse(FireTvTargetPolicy.supportsApi(25))
        assertFalse(FireTvTargetPolicy.supportsApi(27))
        assertTrue(FireTvTargetPolicy.supportsApi(28))
        assertTrue(FireTvTargetPolicy.supportsApi(36))
        assertEquals("fire_tv", TvDeviceProfile.fireTv().platform)
        assertEquals("duskcue_fire_tv", TvDeviceProfile.fireTv().client)
    }
}
