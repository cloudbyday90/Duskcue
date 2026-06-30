import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

class AppStrings {
  const AppStrings(this.locale);

  final Locale locale;

  static const supportedLocales = [Locale('en')];

  static AppStrings of(BuildContext context) {
    return Localizations.of<AppStrings>(context, AppStrings)!;
  }

  String get appName => 'Duskcue';
  String get dashboard => 'Dashboard';
  String get libraries => 'Libraries';
  String get search => 'Search';
  String get collections => 'Collections';
  String get notifications => 'Notifications';
  String get settings => 'Settings';
  String get mediaDetails => 'Details';
  String get play => 'Play';
  String get browseLibraries => 'Browse libraries';
  String get recentlyAdded => 'Recently added';
  String get noServerSelected => 'No server selected';
  String get emptyLibraries => 'No libraries are available for this account.';
  String get emptyItems => 'No media items found.';
  String get emptyCollections => 'No collections found.';
  String get emptyNotifications => 'No notifications found.';
  String get pullToRefresh => 'Pull to refresh';
  String get loadMore => 'Load more';
  String get retry => 'Retry';
  String get serverUnavailable => 'The server is unavailable. Check your connection and try again.';
  String get signedOut => 'Your session expired. Sign in again.';
  String get searchHint => 'Movies, episodes, people, genres';
  String get searchEmpty => 'Search your Duskcue libraries.';
  String get searchNoResults => 'No results found.';
  String get playbackEntry => 'Playback entry';
  String get playbackTaskNotice => 'Playback is not ready for this item.';
  String get loadingPlayback => 'Starting playback...';
  String get audio => 'Audio';
  String get subtitles => 'Subtitles';
  String get noSubtitle => 'No subtitle';
  String get restartPlayback => 'Restart playback';
  String get stop => 'Stop';
  String get buffering => 'Buffering';
  String get playbackFailed => 'Playback failed. Try another quality or stream option.';
  String skipSegment(String segmentType) => 'Skip $segmentType';
  String get markAllRead => 'Mark all read';
  String get unread => 'Unread';
  String get read => 'Read';
}

class AppStringsDelegate extends LocalizationsDelegate<AppStrings> {
  const AppStringsDelegate();

  @override
  bool isSupported(Locale locale) {
    return AppStrings.supportedLocales.any((supported) => supported.languageCode == locale.languageCode);
  }

  @override
  Future<AppStrings> load(Locale locale) {
    return SynchronousFuture(AppStrings(locale));
  }

  @override
  bool shouldReload(AppStringsDelegate old) => false;
}
