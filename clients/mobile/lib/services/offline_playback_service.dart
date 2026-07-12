// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

import 'dart:io';

import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/models/download_models.dart';
import 'package:duskcue_mobile/models/playback_models.dart';
import 'package:duskcue_mobile/services/protected_download_storage_service.dart';
import 'package:video_player/video_player.dart';

class OfflinePlaybackStart {
  const OfflinePlaybackStart({
    required this.sessionId,
    required this.item,
    required this.manifest,
    required this.playbackPath,
    required this.audioTracks,
    required this.subtitleTracks,
    required this.resumePositionMs,
  });

  final String sessionId;
  final MediaItemSummary item;
  final DownloadPackageManifest manifest;
  final String playbackPath;
  final List<AudioTrack> audioTracks;
  final List<SubtitleTrack> subtitleTracks;
  final int resumePositionMs;

  String get streamDecision => 'offline_${manifest.packageFormat}';
}

class OfflinePlaybackService {
  const OfflinePlaybackService(this._protectedStorage);

  final ProtectedDownloadStorageService _protectedStorage;

  Future<OfflinePlaybackStart> startPlayback({
    required DownloadInventoryScope scope,
    required DownloadItem item,
  }) async {
    if (!item.canPlayOffline) {
      throw StateError('Download is not locally playable.');
    }
    final manifest = await _protectedStorage.readPackageManifest(scope, item);
    if (manifest == null) {
      throw StateError('Offline package manifest is missing.');
    }
    if (manifest.expiresAt != null && manifest.expiresAt!.isBefore(DateTime.now())) {
      throw StateError('Offline package has expired.');
    }
    final playbackFile = manifest.primaryPlaybackFile;
    if (playbackFile == null) {
      throw StateError('Offline package has no playable media file.');
    }
    final playbackPath = await _protectedStorage.packageFilePath(scope, item, playbackFile.relativePath);
    if (!await File(playbackPath).exists()) {
      throw StateError('Offline media file is missing.');
    }
    return OfflinePlaybackStart(
      sessionId: 'offline:${manifest.packageId}:${DateTime.now().microsecondsSinceEpoch}',
      item: MediaItemSummary(
        id: item.mediaItemId,
        title: item.title,
        mediaType: item.mediaType,
      ),
      manifest: manifest,
      playbackPath: playbackPath,
      audioTracks: _audioTracks(manifest),
      subtitleTracks: _subtitleTracks(manifest),
      resumePositionMs: item.localResumePositionMs,
    );
  }

  Future<VideoPlayerController> createLocalController(String path) async {
    final controller = VideoPlayerController.file(File(path));
    await controller.initialize();
    return controller;
  }

  List<AudioTrack> _audioTracks(DownloadPackageManifest manifest) {
    if (manifest.selectedAudio.isEmpty) {
      return const [AudioTrack(index: 0, label: 'Default')];
    }
    return [AudioTrack.fromJson(manifest.selectedAudio)];
  }

  List<SubtitleTrack> _subtitleTracks(DownloadPackageManifest manifest) {
    return manifest.selectedSubtitles
        .map((track) => SubtitleTrack.fromJson(track))
        .where((track) => track.id.isNotEmpty || track.streamIndex != null)
        .toList(growable: false);
  }
}
