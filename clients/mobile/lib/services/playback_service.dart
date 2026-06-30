import 'package:video_player/video_player.dart';

class PlaybackService {
  Future<VideoPlayerController> createNetworkController(Uri uri) async {
    final controller = VideoPlayerController.networkUrl(uri);
    await controller.initialize();
    return controller;
  }
}
