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

import AVFoundation
import Flutter
import MediaPlayer
import UIKit

final class AmbientPlaybackBridge {
  private let controller = AmbientPlaybackController.shared

  func register(messenger: FlutterBinaryMessenger, registrar: FlutterPluginRegistrar) {
    let channel = FlutterMethodChannel(name: "duskcue/ambient_player", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "ambient_unavailable", message: "Ambient playback is unavailable.", details: nil))
        return
      }
      switch call.method {
      case "start":
        let arguments = call.arguments as? [String: Any] ?? [:]
        self.controller.start(arguments: arguments) { outcome in
          switch outcome {
          case .success:
            result(nil)
          case .failure(let error):
            result(FlutterError(code: "ambient_start_failed", message: error.localizedDescription, details: nil))
          }
        }
      case "stop", "clear":
        self.controller.stop()
        result(nil)
      case "status":
        result(self.controller.status())
      default:
        result(FlutterMethodNotImplemented)
      }
    }
    registrar.register(AmbientPlayerViewFactory(), withId: "duskcue/ambient_player_view")
  }
}

private final class AmbientPlayerViewFactory: NSObject, FlutterPlatformViewFactory {
  func create(
    withFrame frame: CGRect,
    viewIdentifier viewId: Int64,
    arguments args: Any?
  ) -> FlutterPlatformView {
    AmbientPlayerPlatformView(frame: frame)
  }

  func createArgsCodec() -> FlutterMessageCodec & NSObjectProtocol {
    FlutterStandardMessageCodec.sharedInstance()
  }
}

private final class AmbientPlayerPlatformView: NSObject, FlutterPlatformView {
  private let surface: AmbientPlayerSurface

  init(frame: CGRect) {
    surface = AmbientPlayerSurface(frame: frame)
    super.init()
    AmbientPlaybackController.shared.attach(surface: surface)
  }

  func view() -> UIView {
    surface
  }

  deinit {
    AmbientPlaybackController.shared.detach(surface: surface)
  }
}

private final class AmbientPlayerSurface: UIView {
  private let playerLayer = AVPlayerLayer()

  override init(frame: CGRect) {
    super.init(frame: frame)
    backgroundColor = .black
    playerLayer.videoGravity = .resizeAspect
    layer.addSublayer(playerLayer)
  }

  required init?(coder: NSCoder) {
    nil
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    playerLayer.frame = bounds
  }

  func setPlayer(_ player: AVPlayer?) {
    playerLayer.player = player
  }
}

private final class AmbientPlaybackController: NSObject {
  private struct Runtime {
    let id: UUID
    let serverOrigin: String
    let bearerToken: String
    let channelId: String
    var channelName: String
    var mediaItemId: String?
    var sessionId: String?
  }

  private struct Selection {
    let channelName: String
    let mediaItemId: String
    let channelUpdatedAt: String
  }

  private struct AmbientRequestError: LocalizedError {
    let statusCode: Int
    let detail: String

    var errorDescription: String? {
      detail.isEmpty ? "Ambient playback request failed (\(statusCode))." : detail
    }
  }

  static let shared = AmbientPlaybackController()

  private let player = AVQueuePlayer()
  private weak var surface: AmbientPlayerSurface?
  private var runtime: Runtime?
  private var heartbeatTimer: Timer?
  private var periodicTimeObserver: Any?
  private var itemEndObserver: NSObjectProtocol?
  private var itemFailureObserver: NSObjectProtocol?
  private var lastError: String?
  private var advancing = false

  private override init() {
    super.init()
    configureRemoteCommands()
  }

  func start(arguments: [String: Any], completion: @escaping (Result<Void, Error>) -> Void) {
    guard
      let serverOrigin = arguments["server_origin"] as? String,
      let bearerToken = arguments["bearer_token"] as? String,
      let channelId = arguments["channel_id"] as? String,
      let channelName = arguments["channel_name"] as? String,
      !serverOrigin.isEmpty,
      !bearerToken.isEmpty,
      !channelId.isEmpty,
      !channelName.isEmpty
    else {
      completion(.failure(AmbientRequestError(statusCode: 0, detail: "Ambient playback could not start.")))
      return
    }
    stop()
    runtime = Runtime(
      id: UUID(),
      serverOrigin: serverOrigin,
      bearerToken: bearerToken,
      channelId: channelId,
      channelName: channelName,
      mediaItemId: nil,
      sessionId: nil
    )
    lastError = nil
    advancing = false
    configureAudioSession { [weak self] outcome in
      switch outcome {
      case .success:
        self?.loadNext(afterMediaItemId: nil, staleRetries: 0, completion: completion)
      case .failure(let error):
        self?.lastError = error.localizedDescription
        self?.stop(clearError: false)
        completion(.failure(error))
      }
    }
  }

  func stop(clearError: Bool = true) {
    clearPlaybackObservers()
    let previousRuntime = runtime
    let sessionId = previousRuntime?.sessionId
    let positionMs = Int((player.currentTime().seconds.isFinite ? player.currentTime().seconds : 0) * 1000)
    runtime = nil
    advancing = false
    player.pause()
    player.removeAllItems()
    surface?.setPlayer(nil)
    MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    deactivateAudioSession()
    if clearError {
      lastError = nil
    }
    if let previousRuntime, let sessionId {
      postJSON(
        runtime: previousRuntime,
        path: "/api/v1/playback/stop",
        body: ["session_id": sessionId, "position_ms": max(positionMs, 0)]
      ) { _ in }
    }
  }

  func attach(surface: AmbientPlayerSurface) {
    self.surface = surface
    surface.setPlayer(runtime == nil ? nil : player)
  }

  func detach(surface: AmbientPlayerSurface) {
    if self.surface === surface {
      surface.setPlayer(nil)
      self.surface = nil
    }
  }

  func status() -> [String: Any] {
    let position = player.currentTime().seconds
    return [
      "is_active": runtime != nil,
      "channel_id": runtime?.channelId ?? NSNull(),
      "channel_name": runtime?.channelName ?? NSNull(),
      "media_item_id": runtime?.mediaItemId ?? NSNull(),
      "position_ms": Int((position.isFinite ? position : 0) * 1000),
      "is_playing": player.rate > 0,
      "error": lastError ?? NSNull()
    ]
  }

  private func loadNext(
    afterMediaItemId: String?,
    staleRetries: Int,
    completion: @escaping (Result<Void, Error>) -> Void
  ) {
    guard let currentRuntime = runtime else { return }
    requestNext(runtime: currentRuntime, afterMediaItemId: afterMediaItemId) { [weak self] selectionResult in
      guard let self else { return }
      switch selectionResult {
      case .success(let selection):
        self.requestPlaybackStart(runtime: currentRuntime, selection: selection) { startResult in
          DispatchQueue.main.async {
            guard self.runtime?.id == currentRuntime.id else {
              if case .success(let streamURL) = startResult {
                self.postJSON(
                  runtime: currentRuntime,
                  path: "/api/v1/playback/stop",
                  body: ["session_id": streamURL.sessionId, "position_ms": 0]
                ) { _ in }
              }
              return
            }
            switch startResult {
            case .success(let streamURL):
              self.runtime?.channelName = selection.channelName
              self.runtime?.mediaItemId = selection.mediaItemId
              self.runtime?.sessionId = streamURL.sessionId
              self.play(runtime: currentRuntime, streamURL: streamURL.url, selection: selection)
              completion(.success(()))
            case .failure(let error as AmbientRequestError) where error.statusCode == 409 && staleRetries < 1:
              self.loadNext(afterMediaItemId: nil, staleRetries: staleRetries + 1, completion: completion)
            case .failure(let error):
              self.lastError = error.localizedDescription
              self.stop(clearError: false)
              completion(.failure(error))
            }
          }
        }
      case .failure(let error):
        DispatchQueue.main.async {
          self.lastError = error.localizedDescription
          self.stop(clearError: false)
          completion(.failure(error))
        }
      }
    }
  }

  private func play(runtime: Runtime, streamURL: URL, selection: Selection) {
    let asset = AVURLAsset(
      url: streamURL,
      options: [AVURLAssetHTTPHeaderFieldsKey: ["Authorization": "Bearer \(runtime.bearerToken)"]]
    )
    let item = AVPlayerItem(asset: asset)
    player.removeAllItems()
    player.insert(item, after: nil)
    advancing = false
    surface?.setPlayer(player)
    player.play()
    configureNowPlaying(title: selection.channelName)
    itemEndObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemDidPlayToEndTime,
      object: item,
      queue: .main
    ) { [weak self] _ in
      guard let self else { return }
      self.advanceToNext(afterMediaItemId: selection.mediaItemId)
    }
    itemFailureObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemFailedToPlayToEndTime,
      object: item,
      queue: .main
    ) { [weak self] notification in
      guard let self else { return }
      let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
      self.lastError = error?.localizedDescription ?? "Ambient playback failed."
      self.stop(clearError: false)
    }
    heartbeatTimer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
      self?.sendHeartbeat()
    }
    periodicTimeObserver = player.addPeriodicTimeObserver(
      forInterval: CMTime(seconds: 1, preferredTimescale: 1),
      queue: .main
    ) { [weak self] time in
      self?.updateNowPlaying(position: time.seconds)
    }
  }

  private func advanceToNext(afterMediaItemId: String?) {
    guard let currentRuntime = runtime, !advancing else { return }
    let completedSessionId = currentRuntime.sessionId
    let seconds = player.currentTime().seconds
    let positionMs = Int((seconds.isFinite ? seconds : 0) * 1000)
    advancing = true
    runtime?.sessionId = nil
    clearPlaybackObservers()
    let load = { [weak self] in
      guard let self, self.runtime?.id == currentRuntime.id else { return }
      self.loadNext(afterMediaItemId: afterMediaItemId, staleRetries: 0) { _ in }
    }
    guard let completedSessionId else {
      load()
      return
    }
    postJSON(
      runtime: currentRuntime,
      path: "/api/v1/playback/stop",
      body: ["session_id": completedSessionId, "position_ms": max(positionMs, 0)]
    ) { _ in
      DispatchQueue.main.async(execute: load)
    }
  }

  private func clearPlaybackObservers() {
    heartbeatTimer?.invalidate()
    heartbeatTimer = nil
    if let periodicTimeObserver {
      player.removeTimeObserver(periodicTimeObserver)
      self.periodicTimeObserver = nil
    }
    if let itemEndObserver {
      NotificationCenter.default.removeObserver(itemEndObserver)
      self.itemEndObserver = nil
    }
    if let itemFailureObserver {
      NotificationCenter.default.removeObserver(itemFailureObserver)
      self.itemFailureObserver = nil
    }
  }

  private func sendHeartbeat() {
    guard let currentRuntime = runtime, let sessionId = currentRuntime.sessionId else { return }
    let seconds = player.currentTime().seconds
    let positionMs = Int((seconds.isFinite ? seconds : 0) * 1000)
    postJSON(
      runtime: currentRuntime,
      path: "/api/v1/playback/heartbeat",
      body: [
        "session_id": sessionId,
        "position_ms": max(positionMs, 0),
        "state": player.rate > 0 ? "playing" : "paused",
        "is_paused": player.rate == 0,
        "is_buffering": false
      ]
    ) { _ in }
  }

  private func requestNext(
    runtime: Runtime,
    afterMediaItemId: String?,
    completion: @escaping (Result<Selection, Error>) -> Void
  ) {
    postJSON(
      runtime: runtime,
      path: "/api/v1/ambient-channels/\(runtime.channelId)/next",
      body: ["after_media_item_id": afterMediaItemId ?? NSNull()]
    ) { result in
      completion(result.flatMap { data in
        guard
          let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let channelName = object["channel_name"] as? String,
          let mediaItemId = object["media_item_id"] as? String,
          let channelUpdatedAt = object["channel_updated_at"] as? String
        else {
          return .failure(AmbientRequestError(statusCode: 0, detail: "Ambient channel returned an invalid response."))
        }
        return .success(Selection(channelName: channelName, mediaItemId: mediaItemId, channelUpdatedAt: channelUpdatedAt))
      })
    }
  }

  private func requestPlaybackStart(
    runtime: Runtime,
    selection: Selection,
    completion: @escaping (Result<(sessionId: String, url: URL), Error>) -> Void
  ) {
    let profile: [String: Any] = [
      "client": "duskcue_mobile",
      "platform": "ios_native_ambient",
      "video_codecs": ["h264"],
      "audio_codecs": ["aac", "mp3", "opus"],
      "subtitle_formats": ["webvtt", "srt"],
      "max_resolution": "1080p",
      "hls_supported": true,
      "hdr_supported": false
    ]
    postJSON(
      runtime: runtime,
      path: "/api/v1/playback/start",
      body: [
        "media_item_id": selection.mediaItemId,
        "playback_mode": "ambient",
        "ambient_channel_id": runtime.channelId,
        "ambient_channel_updated_at": selection.channelUpdatedAt,
        "device_profile": profile
      ]
    ) { result in
      completion(result.flatMap { data in
        guard
          let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let sessionId = object["session_id"] as? String,
          let streamValue = object["stream_url"] as? String,
          let url = self.resolveURL(origin: runtime.serverOrigin, value: streamValue)
        else {
          return .failure(AmbientRequestError(statusCode: 0, detail: "Ambient playback returned an invalid stream response."))
        }
        return .success((sessionId, url))
      })
    }
  }

  private func postJSON(
    runtime: Runtime,
    path: String,
    body: [String: Any],
    completion: @escaping (Result<Data, Error>) -> Void
  ) {
    guard let url = resolveURL(origin: runtime.serverOrigin, value: path) else {
      completion(.failure(AmbientRequestError(statusCode: 0, detail: "Duskcue server URL is invalid.")))
      return
    }
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.timeoutInterval = 30
    request.setValue("Bearer \(runtime.bearerToken)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try? JSONSerialization.data(withJSONObject: body)
    URLSession.shared.dataTask(with: request) { data, response, error in
      if let error {
        completion(.failure(error))
        return
      }
      let statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
      let payload = data ?? Data()
      if !(200...299).contains(statusCode) {
        let detail = (try? JSONSerialization.jsonObject(with: payload) as? [String: Any])?["detail"] as? String ?? ""
        completion(.failure(AmbientRequestError(statusCode: statusCode, detail: detail)))
        return
      }
      completion(.success(payload))
    }.resume()
  }

  private func resolveURL(origin: String, value: String) -> URL? {
    if let absolute = URL(string: value), absolute.scheme != nil {
      return absolute
    }
    return URL(string: origin.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + "/" + value.trimmingCharacters(in: CharacterSet(charactersIn: "/")))
  }

  private func configureAudioSession(completion: @escaping (Result<Void, Error>) -> Void) {
    do {
      let session = AVAudioSession.sharedInstance()
      try session.setCategory(.playback, mode: .moviePlayback)
      try session.setActive(true)
      completion(.success(()))
    } catch {
      completion(.failure(error))
    }
  }

  private func deactivateAudioSession() {
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }

  private func configureRemoteCommands() {
    let commands = MPRemoteCommandCenter.shared()
    commands.playCommand.addTarget { [weak self] _ in
      self?.player.play()
      self?.sendHeartbeat()
      return .success
    }
    commands.pauseCommand.addTarget { [weak self] _ in
      self?.player.pause()
      self?.sendHeartbeat()
      return .success
    }
  }

  private func configureNowPlaying(title: String) {
    MPNowPlayingInfoCenter.default().nowPlayingInfo = [
      MPMediaItemPropertyTitle: title,
      MPNowPlayingInfoPropertyPlaybackRate: player.rate
    ]
  }

  private func updateNowPlaying(position: Double) {
    var info = MPNowPlayingInfoCenter.default().nowPlayingInfo ?? [:]
    info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = position.isFinite ? position : 0
    info[MPNowPlayingInfoPropertyPlaybackRate] = player.rate
    MPNowPlayingInfoCenter.default().nowPlayingInfo = info
  }
}
