import 'package:duskcue_mobile/app.dart';
import 'package:duskcue_mobile/services/push_registration_service.dart';
import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

void main() {
  FirebaseMessaging.onBackgroundMessage(duskcueFirebaseMessagingBackgroundHandler);
  runApp(const ProviderScope(child: DuskcueApp()));
}
