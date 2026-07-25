package com.duskcue.tv.watchnext

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.tvprovider.media.tv.TvContractCompat
import com.duskcue.tv.TvApplicationRuntime

class WatchNextProgramReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != TvContractCompat.ACTION_WATCH_NEXT_PROGRAM_BROWSABLE_DISABLED) {
            return
        }
        val programId = intent.getLongExtra(TvContractCompat.EXTRA_WATCH_NEXT_PROGRAM_ID, -1)
        if (programId < 0) {
            return
        }
        val pendingResult = goAsync()
        TvApplicationRuntime(context.applicationContext).handleWatchNextProgramDisabled(programId) {
            pendingResult.finish()
        }
    }
}
