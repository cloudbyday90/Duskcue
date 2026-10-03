package com.duskcue.fire

internal object FireTvTargetPolicy {
    const val platform = "fire_tv"
    const val minimumApi = 28
    const val integrationState = "app_local_only"

    fun supportsApi(apiLevel: Int): Boolean = apiLevel >= minimumApi
}
