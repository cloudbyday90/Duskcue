package com.duskcue.fire.personalization

import android.content.Context
import com.amazon.tv.developer.sdk.content.personalization.AmazonPlaybackReceiver
import com.amazon.tv.developer.sdk.content.personalization.model.AmazonContentId
import com.amazon.tv.developer.sdk.content.personalization.model.AmazonPlaybackEvent
import com.amazon.tv.developer.sdk.content.personalization.model.AmazonPlaybackState
import com.amazon.tv.developer.sdk.content.personalization.model.AmazonProfileId

class FireDevicePlaybackSink(context: Context) : FirePlaybackSink {
    private val context = context.applicationContext

    override fun available(): Boolean = context.packageManager.hasSystemFeature(
        "com.amazon.tv.developer.sdk.content",
    )

    override fun sendActive(event: FirePlaybackEvent): Boolean {
        if (!available()) return false
        return try {
            FireSdkPlaybackSink.send(context, event)
            true
        } catch (_: RuntimeException) {
            false
        } catch (_: LinkageError) {
            false
        }
    }
}

private object FireSdkPlaybackSink {
    fun send(context: Context, event: FirePlaybackEvent) {
        val state = when (event.state) {
            FirePlaybackState.PLAYING -> AmazonPlaybackState.PLAYING
            FirePlaybackState.PAUSED -> AmazonPlaybackState.PAUSED
            FirePlaybackState.INTERSTITIAL -> AmazonPlaybackState.INTERSTITIAL
            FirePlaybackState.EXIT -> AmazonPlaybackState.EXIT
        }
        val payload = AmazonPlaybackEvent.builder()
            .contentId(AmazonContentId.builder()
                .id(event.catalogContentId)
                .namespace(AmazonContentId.NAMESPACE_CDF_ID)
                .build())
            .profileId(AmazonProfileId.builder()
                .id(event.opaqueProfileKey)
                .namespace(AmazonProfileId.NAMESPACE_APP_INTERNAL)
                .build())
            .durationMs(event.durationMs)
            .playbackPositionMs(event.positionMs)
            .state(state)
            .buildActiveEvent()
        AmazonPlaybackReceiver.getInstance(context).addPlaybackEvent(payload)
    }
}
