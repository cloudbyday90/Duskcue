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
import com.duskcue.mobile.ambient.AmbientApplication
import com.duskcue.mobile.ambient.AmbientIdentity
import com.duskcue.mobile.ambient.AmbientOwner
import com.duskcue.mobile.ambient.demand
import com.duskcue.mobile.ambient.ambientFailure
import java.util.concurrent.atomic.AtomicLong

object AmbientPlaybackBridge {
    private const val CHANNEL = "duskcue/ambient_player"
    private const val VIEW_TYPE = "duskcue/ambient_player_view"
    private val registrations = AtomicLong()
    private fun bound(owner: AmbientOwner, arguments: Any?) {
        val value = (arguments as? Map<*, *>)?.get("runtime") as? Map<*, *>
        val supplied = value?.let(::AmbientIdentity)
        demand(if (supplied == null) owner.identity == null else supplied.same(owner.identity), "scope_changed")
    }

    fun register(context: Context, flutterEngine: FlutterEngine) {
        val registration = registrations.incrementAndGet()
        flutterEngine.platformViewsController.registry.registerViewFactory(VIEW_TYPE, AmbientPlayerViewFactory { registration == registrations.get() })
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL).setMethodCallHandler { call, result ->
            try {
                demand(registration == registrations.get(), "scope_changed")
                when (call.method) {
                "start" -> {
                    val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                    AmbientApplication.start(context.applicationContext, args)
                    result.success(null)
                }
                "stop", "clear" -> {
                    val owner = AmbientApplication.owner(context)
                    bound(owner, call.arguments)
                    AmbientApplication.cancelPending()
                    if (owner.legacyMode) {
                        LegacyAmbientPlaybackService.stop(context)
                        result.success(null)
                    } else owner.stop().whenComplete { _, error ->
                        android.os.Handler(android.os.Looper.getMainLooper()).post {
                            if (error == null) result.success(null) else result.error("ambient_cleanup_pending", "Ambient cleanup is not confirmed", null)
                        }
                    }
                }
                "fence" -> {
                    AmbientApplication.fence(context, call.arguments as? Map<*, *>).whenComplete { _, _ -> Unit }
                    result.success(null)
                }
                "bind" -> AmbientApplication.owner(context).bind(call.arguments as? Map<*, *>).whenComplete { _, error ->
                    android.os.Handler(android.os.Looper.getMainLooper()).post {
                        if (error == null) result.success(null) else result.error(ambientFailure(error)?.code ?: "ambient_cleanup_pending", "Ambient reconciliation is not confirmed", null)
                    }
                }
                "status" -> {
                    val owner = AmbientApplication.owner(context)
                    bound(owner, call.arguments)
                    result.success(if (owner.legacyMode) LegacyAmbientPlaybackService.status() else owner.status())
                }
                "play", "pause", "check", "seek" -> {
                    val owner = AmbientApplication.owner(context)
                    bound(owner, call.arguments)
                    if (call.method == "pause") AmbientApplication.pausePending()
                    val position = (call.arguments as? Map<*, *>)?.get("position_ms") as? Number
                    owner.action(call.method, position?.toLong()).whenComplete { _, error ->
                        android.os.Handler(android.os.Looper.getMainLooper()).post {
                            if (error == null) result.success(null) else result.error("ambient_control_unavailable", "Check ambient playback status", null)
                        }
                    }
                }
                else -> result.notImplemented()
            } } catch (error: Exception) { result.error(ambientFailure(error)?.code ?: "ambient_unavailable", "Ambient playback is unavailable; check status", null) }
        }
    }
}

private class AmbientPlayerViewFactory(private val current: () -> Boolean) : PlatformViewFactory(StandardMessageCodec.INSTANCE) {
    override fun create(context: Context, viewId: Int, args: Any?): PlatformView {
        demand(current(), "scope_changed")
        return AmbientPlayerPlatformView(context)
    }
}

private class AmbientPlayerPlatformView(context: Context) : PlatformView {
    private val playerView = PlayerView(context)

    init {
        playerView.useController = true
        val owner = AmbientApplication.owner(context)
        if (owner.legacyMode) LegacyAmbientPlaybackService.attach(playerView) else owner.attachView(playerView)
    }

    override fun getView() = playerView

    override fun dispose() {
        AmbientApplication.owner(playerView.context).detachView(playerView)
        LegacyAmbientPlaybackService.detach(playerView)
    }
}
