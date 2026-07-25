package com.duskcue.tv.playback

import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.ui.PlayerView
import com.duskcue.tv.api.DuskcueApiClient
import com.duskcue.tv.api.MemoryEtagStore
import com.duskcue.tv.api.MutableBearerTokenProvider
import com.duskcue.tv.api.RetryingTransport
import com.duskcue.tv.api.ServerOrigin
import com.duskcue.tv.api.TvPlaybackHeartbeatRequest
import com.duskcue.tv.api.TvPlaybackSeekRequest
import com.duskcue.tv.api.TvPlaybackStopRequest
import com.duskcue.tv.api.TvQoeReportRequest
import com.duskcue.tv.api.UrlConnectionTransport
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.lang.ref.WeakReference
import java.time.Instant
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

data class TvInteractivePlayback(
    val serverOrigin: String,
    val bearerToken: String,
    val sessionId: String,
    val streamUrl: String,
    val mediaItemId: String,
    val title: String,
    val startPositionMs: Long,
    val qualityMode: String,
    val audioLanguage: String?,
    val subtitleLanguage: String?,
)

data class TvPlaybackUiState(
    val positionMs: Long = 0,
    val isPlaying: Boolean = false,
    val errorCode: String? = null,
    val completionVersion: Long = 0,
    val pauseRefreshVersion: Long = 0,
)

class TvPlaybackService : MediaSessionService() {
    private data class Runtime(
        val playback: TvInteractivePlayback,
        val api: DuskcueApiClient,
        val startedAtMs: Long,
        var firstFrameAtMs: Long? = null,
        var loadingStartedAtMs: Long? = null,
        var bufferedMs: Long = 0,
        var rebufferCount: Int = 0,
        var qualityChangeCount: Int = 0,
        var qualityDrops: Int = 0,
        var videoBitrateBps: Int? = null,
        var videoHeight: Int? = null,
    )

    private val mainHandler = Handler(Looper.getMainLooper())
    private val networkExecutor: ExecutorService = Executors.newSingleThreadExecutor()
    private var player: ExoPlayer? = null
    private var mediaSession: MediaSession? = null
    private var runtime: Runtime? = null
    private var stopping = false

    private val heartbeatRunnable = object : Runnable {
        override fun run() {
            reportHeartbeat()
            if (runtime != null) {
                mainHandler.postDelayed(this, HEARTBEAT_INTERVAL_MS)
            }
        }
    }

    private val playerStateRunnable = object : Runnable {
        override fun run() {
            publishPlayerState()
            if (runtime != null) {
                mainHandler.postDelayed(this, PLAYER_STATE_INTERVAL_MS)
            }
        }
    }

    private val pausedWatchNextRunnable = Runnable {
        val activePlayer = player
        if (runtime != null && activePlayer?.isPlaying == false && activePlayer.playbackState == Player.STATE_READY) {
            val current = playbackUiMutable.value
            playbackUiMutable.value = current.copy(pauseRefreshVersion = current.pauseRefreshVersion + 1)
        }
    }

    @UnstableApi
    override fun onCreate() {
        super.onCreate()
        activeService = this
        player = ExoPlayer.Builder(this)
            .setSeekBackIncrementMs(SEEK_BACK_INCREMENT_MS)
            .setSeekForwardIncrementMs(SEEK_FORWARD_INCREMENT_MS)
            .build()
            .also { activePlayer ->
            activePlayer.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                    .build(),
                true,
            )
            activePlayer.addListener(PlayerEvents())
            activePlayer.addAnalyticsListener(QoeAnalytics())
        }
        mediaSession = MediaSession.Builder(this, requireNotNull(player)).build()
    }

    @UnstableApi
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_PLAY -> startPlayback(intent)
            ACTION_STOP -> {
                stopPlayback(notifyServer = true)
                stopSelf(startId)
            }
        }
        return Service.START_NOT_STICKY
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        player?.pause()
        stopPlayback(notifyServer = true)
        stopSelf()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = mediaSession

    override fun onDestroy() {
        stopPlayback(notifyServer = true)
        mediaSession?.release()
        mediaSession = null
        player?.release()
        player = null
        activeService = null
        networkExecutor.shutdown()
        super.onDestroy()
    }

    @UnstableApi
    private fun startPlayback(intent: Intent) {
        val launch = intent.toPlayback() ?: run {
            stopSelf()
            return
        }
        stopPlayback(notifyServer = true)
        val origin = ServerOrigin.parse(launch.serverOrigin).getOrNull() ?: run {
            stopSelf()
            return
        }
        val tokenProvider = MutableBearerTokenProvider(launch.bearerToken)
        val api = DuskcueApiClient(
            origin = origin,
            transport = RetryingTransport(UrlConnectionTransport()),
            tokenProvider = tokenProvider,
            etagStore = MemoryEtagStore(),
        )
        val activeRuntime = Runtime(launch, api, System.currentTimeMillis())
        runtime = activeRuntime
        playbackUiMutable.value = TvPlaybackUiState()
        val activePlayer = player ?: return
        activePlayer.trackSelectionParameters = activePlayer.trackSelectionParameters
            .buildUpon()
            .setPreferredAudioLanguage(launch.audioLanguage)
            .setPreferredTextLanguage(launch.subtitleLanguage)
            .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, launch.subtitleLanguage == null)
            .build()
        val mediaItem = MediaItem.Builder()
            .setMediaId(launch.mediaItemId)
            .setUri(resolveUrl(launch.serverOrigin, launch.streamUrl))
            .setMediaMetadata(MediaMetadata.Builder().setTitle(launch.title).build())
            .build()
        val dataSourceFactory = DefaultDataSource.Factory(
            this,
            DefaultHttpDataSource.Factory().setDefaultRequestProperties(
                mapOf("Authorization" to "Bearer ${launch.bearerToken}"),
            ),
        )
        activePlayer.setMediaSource(DefaultMediaSourceFactory(dataSourceFactory).createMediaSource(mediaItem))
        activePlayer.seekTo(launch.startPositionMs.coerceAtLeast(0))
        activePlayer.prepare()
        activePlayer.play()
        attachPlayerToView(activePlayer)
        mainHandler.removeCallbacks(heartbeatRunnable)
        mainHandler.postDelayed(heartbeatRunnable, HEARTBEAT_INTERVAL_MS)
        mainHandler.removeCallbacks(playerStateRunnable)
        mainHandler.post(playerStateRunnable)
        mainHandler.removeCallbacks(pausedWatchNextRunnable)
    }

    private fun reportHeartbeat() {
        val activeRuntime = runtime ?: return
        val activePlayer = player ?: return
        val state = when {
            activePlayer.isLoading -> "buffering"
            activePlayer.isPlaying -> "playing"
            else -> "paused"
        }
        val request = TvPlaybackHeartbeatRequest(
            session_id = activeRuntime.playback.sessionId,
            position_ms = activePlayer.currentPosition.coerceAtLeast(0),
            state = state,
            is_paused = !activePlayer.isPlaying,
            is_buffering = activePlayer.isLoading,
        )
        networkExecutor.execute { activeRuntime.api.playbackHeartbeat(request) }
    }

    private fun publishPlayerState() {
        val activePlayer = player
        val current = playbackUiMutable.value
        playbackUiMutable.value = if (runtime == null || activePlayer == null) {
            TvPlaybackUiState(
                completionVersion = current.completionVersion,
                pauseRefreshVersion = current.pauseRefreshVersion,
            )
        } else {
            TvPlaybackUiState(
                positionMs = activePlayer.currentPosition.coerceAtLeast(0),
                isPlaying = activePlayer.isPlaying,
                completionVersion = current.completionVersion,
                pauseRefreshVersion = current.pauseRefreshVersion,
            )
        }
    }

    private fun reportSeek(positionMs: Long) {
        val activeRuntime = runtime ?: return
        networkExecutor.execute {
            activeRuntime.api.playbackSeek(
                TvPlaybackSeekRequest(
                    session_id = activeRuntime.playback.sessionId,
                    position_ms = positionMs.coerceAtLeast(0),
                ),
            )
        }
    }

    private fun reportQoe(
        activeRuntime: Runtime,
        failureCode: String? = null,
        failureMessage: String? = null,
    ) {
        val elapsedMs = (System.currentTimeMillis() - activeRuntime.startedAtMs).coerceAtLeast(1)
        val startupMs = activeRuntime.firstFrameAtMs?.minus(activeRuntime.startedAtMs)?.toInt()
        val request = TvQoeReportRequest(
            session_id = activeRuntime.playback.sessionId,
            startup_time_ms = startupMs,
            rebuffer_count = activeRuntime.rebufferCount,
            rebuffer_duration_ms = activeRuntime.bufferedMs,
            rebuffer_ratio = activeRuntime.bufferedMs.toDouble() / elapsedMs,
            average_bitrate_bps = activeRuntime.videoBitrateBps?.toLong(),
            switches_per_minute = activeRuntime.qualityChangeCount * 60_000.0 / elapsedMs,
            quality_drops = activeRuntime.qualityDrops,
            quality_change_count = activeRuntime.qualityChangeCount,
            selected_quality_mode = activeRuntime.playback.qualityMode,
            current_rung = currentRung(activeRuntime),
            current_buffer_seconds = player?.let { activePlayer ->
                (activePlayer.bufferedPosition - activePlayer.currentPosition)
                    .coerceAtLeast(0)
                    .toDouble()
                    .div(1_000)
            },
            playback_failure_code = failureCode,
            playback_failure_message = failureMessage,
            recorded_at = Instant.now().toString(),
        )
        networkExecutor.execute { activeRuntime.api.reportPlaybackQoe(request) }
    }

    private fun currentRung(activeRuntime: Runtime): String? {
        val height = activeRuntime.videoHeight ?: return null
        val bitrate = activeRuntime.videoBitrateBps ?: return "${height}p"
        return "${height}p-${bitrate / 1_000_000}mbps"
    }

    private fun stopPlayback(
        notifyServer: Boolean,
        failureCode: String? = null,
        failureMessage: String? = null,
    ) {
        if (stopping) return
        val activeRuntime = runtime ?: return
        stopping = true
        mainHandler.removeCallbacks(heartbeatRunnable)
        mainHandler.removeCallbacks(playerStateRunnable)
        mainHandler.removeCallbacks(pausedWatchNextRunnable)
        runtime = null
        val positionMs = player?.currentPosition?.coerceAtLeast(0) ?: 0
        player?.stop()
        player?.clearMediaItems()
        detachPlayerFromView()
        playbackUiMutable.value = TvPlaybackUiState(
            errorCode = failureCode,
            completionVersion = playbackUiMutable.value.completionVersion,
            pauseRefreshVersion = playbackUiMutable.value.pauseRefreshVersion,
        )
        reportQoe(activeRuntime, failureCode, failureMessage)
        if (notifyServer) {
            networkExecutor.execute {
                activeRuntime.api.stopTvPlayback(
                    TvPlaybackStopRequest(
                        session_id = activeRuntime.playback.sessionId,
                        position_ms = positionMs,
                    ),
                )
            }
        }
        stopping = false
    }

    private fun resolveUrl(origin: String, streamUrl: String): String {
        val parsed = Uri.parse(streamUrl)
        return if (parsed.isAbsolute) streamUrl else "${origin.trimEnd('/')}/${streamUrl.trimStart('/')}"
    }

    private fun attachPlayerToView(activePlayer: ExoPlayer) {
        attachedPlayerView?.get()?.player = activePlayer
    }

    private fun detachPlayerFromView() {
        attachedPlayerView?.get()?.player = null
    }

    private inner class PlayerEvents : Player.Listener {
        override fun onRenderedFirstFrame() {
            val activeRuntime = runtime ?: return
            if (activeRuntime.firstFrameAtMs == null) {
                activeRuntime.firstFrameAtMs = System.currentTimeMillis()
                reportHeartbeat()
            }
        }

        override fun onIsLoadingChanged(isLoading: Boolean) {
            val activeRuntime = runtime ?: return
            if (isLoading) {
                if (activeRuntime.firstFrameAtMs != null && activeRuntime.loadingStartedAtMs == null) {
                    activeRuntime.rebufferCount += 1
                }
                activeRuntime.loadingStartedAtMs = System.currentTimeMillis()
            } else {
                activeRuntime.loadingStartedAtMs?.let { started ->
                    activeRuntime.bufferedMs += (System.currentTimeMillis() - started).coerceAtLeast(0)
                }
                activeRuntime.loadingStartedAtMs = null
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            if (runtime != null) {
                publishPlayerState()
                reportHeartbeat()
                mainHandler.removeCallbacks(pausedWatchNextRunnable)
                if (!isPlaying && player?.playbackState == Player.STATE_READY) {
                    mainHandler.postDelayed(pausedWatchNextRunnable, PAUSED_WATCH_NEXT_DELAY_MS)
                }
            }
        }

        override fun onPositionDiscontinuity(
            oldPosition: Player.PositionInfo,
            newPosition: Player.PositionInfo,
            reason: Int,
        ) {
            if (reason == Player.DISCONTINUITY_REASON_SEEK && runtime != null) {
                publishPlayerState()
                reportSeek(newPosition.positionMs)
                reportHeartbeat()
            }
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_ENDED) {
                val current = playbackUiMutable.value
                playbackUiMutable.value = current.copy(completionVersion = current.completionVersion + 1)
                stopPlayback(notifyServer = true)
                stopSelf()
            }
        }

        override fun onPlayerError(error: PlaybackException) {
            stopPlayback(
                notifyServer = true,
                failureCode = error.errorCodeName,
                failureMessage = error.message,
            )
            stopSelf()
        }
    }

    @UnstableApi
    private inner class QoeAnalytics : AnalyticsListener {
        override fun onDownstreamFormatChanged(
            eventTime: AnalyticsListener.EventTime,
            mediaLoadData: androidx.media3.exoplayer.source.MediaLoadData,
        ) {
            if (mediaLoadData.trackType != C.TRACK_TYPE_VIDEO) return
            val activeRuntime = runtime ?: return
            val format = mediaLoadData.trackFormat ?: return
            val nextBitrate = format.bitrate.takeIf { it > 0 }
            val previousBitrate = activeRuntime.videoBitrateBps
            if (previousBitrate != null && nextBitrate != null && previousBitrate != nextBitrate) {
                activeRuntime.qualityChangeCount += 1
                if (nextBitrate < previousBitrate) {
                    activeRuntime.qualityDrops += 1
                }
            }
            activeRuntime.videoBitrateBps = nextBitrate
            activeRuntime.videoHeight = format.height.takeIf { it > 0 }
        }
    }

    private fun Intent.toPlayback(): TvInteractivePlayback? {
        val serverOrigin = getStringExtra(EXTRA_SERVER_ORIGIN)
        val bearerToken = getStringExtra(EXTRA_BEARER_TOKEN)
        val sessionId = getStringExtra(EXTRA_SESSION_ID)
        val streamUrl = getStringExtra(EXTRA_STREAM_URL)
        val mediaItemId = getStringExtra(EXTRA_MEDIA_ITEM_ID)
        val title = getStringExtra(EXTRA_TITLE)
        val qualityMode = getStringExtra(EXTRA_QUALITY_MODE)
        if (
            serverOrigin.isNullOrBlank() || bearerToken.isNullOrBlank() || sessionId.isNullOrBlank() ||
            streamUrl.isNullOrBlank() || mediaItemId.isNullOrBlank() || title.isNullOrBlank() || qualityMode.isNullOrBlank()
        ) {
            return null
        }
        return TvInteractivePlayback(
            serverOrigin = serverOrigin,
            bearerToken = bearerToken,
            sessionId = sessionId,
            streamUrl = streamUrl,
            mediaItemId = mediaItemId,
            title = title,
            startPositionMs = getLongExtra(EXTRA_START_POSITION_MS, 0),
            qualityMode = qualityMode,
            audioLanguage = getStringExtra(EXTRA_AUDIO_LANGUAGE),
            subtitleLanguage = getStringExtra(EXTRA_SUBTITLE_LANGUAGE),
        )
    }

    companion object {
        private const val ACTION_PLAY = "com.duskcue.tv.action.PLAY"
        private const val ACTION_STOP = "com.duskcue.tv.action.STOP"
        private const val EXTRA_SERVER_ORIGIN = "server_origin"
        private const val EXTRA_BEARER_TOKEN = "bearer_token"
        private const val EXTRA_SESSION_ID = "session_id"
        private const val EXTRA_STREAM_URL = "stream_url"
        private const val EXTRA_MEDIA_ITEM_ID = "media_item_id"
        private const val EXTRA_TITLE = "title"
        private const val EXTRA_START_POSITION_MS = "start_position_ms"
        private const val EXTRA_QUALITY_MODE = "quality_mode"
        private const val EXTRA_AUDIO_LANGUAGE = "audio_language"
        private const val EXTRA_SUBTITLE_LANGUAGE = "subtitle_language"
        private const val HEARTBEAT_INTERVAL_MS = 15_000L
        private const val PLAYER_STATE_INTERVAL_MS = 1_000L
        private const val PAUSED_WATCH_NEXT_DELAY_MS = 5 * 60_000L
        private const val SEEK_BACK_INCREMENT_MS = 10_000L
        private const val SEEK_FORWARD_INCREMENT_MS = 30_000L

        @Volatile private var activeService: TvPlaybackService? = null
        @Volatile private var attachedPlayerView: WeakReference<PlayerView>? = null
        private val playbackUiMutable = MutableStateFlow(TvPlaybackUiState())
        val playbackUi: StateFlow<TvPlaybackUiState> = playbackUiMutable.asStateFlow()

        fun start(context: Context, playback: TvInteractivePlayback) {
            val intent = Intent(context, TvPlaybackService::class.java).apply {
                action = ACTION_PLAY
                putExtra(EXTRA_SERVER_ORIGIN, playback.serverOrigin)
                putExtra(EXTRA_BEARER_TOKEN, playback.bearerToken)
                putExtra(EXTRA_SESSION_ID, playback.sessionId)
                putExtra(EXTRA_STREAM_URL, playback.streamUrl)
                putExtra(EXTRA_MEDIA_ITEM_ID, playback.mediaItemId)
                putExtra(EXTRA_TITLE, playback.title)
                putExtra(EXTRA_START_POSITION_MS, playback.startPositionMs)
                putExtra(EXTRA_QUALITY_MODE, playback.qualityMode)
                putExtra(EXTRA_AUDIO_LANGUAGE, playback.audioLanguage)
                putExtra(EXTRA_SUBTITLE_LANGUAGE, playback.subtitleLanguage)
            }
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            activeService?.mainHandler?.post {
                activeService?.stopPlayback(notifyServer = true)
                activeService?.stopSelf()
            } ?: context.stopService(Intent(context, TvPlaybackService::class.java))
        }

        fun pause() {
            activeService?.mainHandler?.post {
                activeService?.player?.pause()
                activeService?.reportHeartbeat()
            }
        }

        fun seekTo(positionMs: Long) {
            activeService?.mainHandler?.post {
                activeService?.player?.seekTo(positionMs.coerceAtLeast(0))
            }
        }

        fun clearPlaybackUi() {
            playbackUiMutable.value = TvPlaybackUiState()
        }

        fun attach(view: PlayerView) {
            attachedPlayerView = WeakReference(view)
            activeService?.player?.let { view.player = it }
        }

        fun detach(view: PlayerView) {
            if (attachedPlayerView?.get() === view) {
                view.player = null
                attachedPlayerView = null
            }
        }
    }
}
