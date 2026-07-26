package com.duskcue.tv.ui

import android.view.KeyEvent

internal object TvQualityPolicy {
    const val HorizontalSafeAreaDp = 58
    const val VerticalSafeAreaDp = 28
    const val MinimumReadableTextSp = 20

    fun backDestination(route: TvRoute, hasDetail: Boolean): TvRoute? = when (route) {
        TvRoute.Home -> null
        TvRoute.Player -> if (hasDetail) TvRoute.Detail else TvRoute.Home
        TvRoute.Browse, TvRoute.Detail, TvRoute.Search, TvRoute.Settings, TvRoute.Profiles -> TvRoute.Home
    }

    fun playerRemoteShortcut(keyCode: Int): TvPlayerRemoteShortcut? = when (keyCode) {
        KeyEvent.KEYCODE_DPAD_LEFT,
        KeyEvent.KEYCODE_MEDIA_REWIND -> TvPlayerRemoteShortcut.SeekBackward
        KeyEvent.KEYCODE_DPAD_RIGHT,
        KeyEvent.KEYCODE_MEDIA_FAST_FORWARD -> TvPlayerRemoteShortcut.SeekForward
        KeyEvent.KEYCODE_DPAD_CENTER,
        KeyEvent.KEYCODE_ENTER,
        KeyEvent.KEYCODE_NUMPAD_ENTER,
        KeyEvent.KEYCODE_BUTTON_A,
        KeyEvent.KEYCODE_MEDIA_PLAY,
        KeyEvent.KEYCODE_MEDIA_PAUSE,
        KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE -> TvPlayerRemoteShortcut.TogglePlayback
        KeyEvent.KEYCODE_MENU,
        KeyEvent.KEYCODE_CAPTIONS -> TvPlayerRemoteShortcut.CycleCaptions
        else -> null
    }
}

internal enum class TvPlayerRemoteShortcut {
    SeekBackward,
    SeekForward,
    TogglePlayback,
    CycleCaptions,
}
