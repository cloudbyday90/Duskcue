/*
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 */

package com.duskcue.mobile

import android.content.Context
import androidx.media3.ui.PlayerView
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.StandardMessageCodec
import io.flutter.plugin.platform.PlatformView
import io.flutter.plugin.platform.PlatformViewFactory

object AmbientPlaybackBridge {
    private const val CHANNEL = "duskcue/ambient_player"
    private const val VIEW_TYPE = "duskcue/ambient_player_view"

    fun register(context: Context, flutterEngine: FlutterEngine) {
        flutterEngine.platformViewsController.registry.registerViewFactory(VIEW_TYPE, AmbientPlayerViewFactory())
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL).setMethodCallHandler { call, result ->
            when (call.method) {
                "start" -> {
                    val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                    AmbientPlaybackService.start(context.applicationContext, args)
                    result.success(null)
                }
                "stop", "clear" -> {
                    AmbientPlaybackService.stop(context.applicationContext)
                    result.success(null)
                }
                "status" -> result.success(AmbientPlaybackService.status())
                else -> result.notImplemented()
            }
        }
    }
}

private class AmbientPlayerViewFactory : PlatformViewFactory(StandardMessageCodec.INSTANCE) {
    override fun create(context: Context, viewId: Int, args: Any?): PlatformView {
        return AmbientPlayerPlatformView(context)
    }
}

private class AmbientPlayerPlatformView(context: Context) : PlatformView {
    private val playerView = PlayerView(context)

    init {
        playerView.useController = true
        AmbientPlaybackService.attach(playerView)
    }

    override fun getView() = playerView

    override fun dispose() {
        AmbientPlaybackService.detach(playerView)
    }
}
