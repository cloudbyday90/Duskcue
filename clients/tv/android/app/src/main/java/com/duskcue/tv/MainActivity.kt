package com.duskcue.tv

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.duskcue.tv.ui.DuskcueTvApp

class MainActivity : ComponentActivity() {
    private lateinit var runtime: TvApplicationRuntime
    private var deepLinkRequest by mutableStateOf(TvDeepLinkRequest(sequence = 0, uri = null))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        runtime = TvApplicationRuntime(applicationContext)
        updateDeepLinkRequest(intent.dataString)
        setContent {
            DuskcueTvApp(runtime, deepLinkRequest)
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        updateDeepLinkRequest(intent.dataString)
    }

    override fun onStop() {
        if (!isChangingConfigurations) {
            runtime.pausePlayback()
        }
        super.onStop()
    }

    private fun updateDeepLinkRequest(uri: String?) {
        deepLinkRequest = TvDeepLinkRequest(
            sequence = deepLinkRequest.sequence + 1,
            uri = uri,
        )
    }
}
