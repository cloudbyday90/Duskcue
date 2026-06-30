import 'package:firebase_messaging/firebase_messaging.dart';

class PushRegistrationService {
  PushRegistrationService({FirebaseMessaging? messaging})
      : _messaging = messaging ?? FirebaseMessaging.instance;

  final FirebaseMessaging _messaging;

  Future<String?> requestFcmToken() async {
    await _messaging.requestPermission();
    return _messaging.getToken();
  }
}
