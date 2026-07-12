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
  String get downloads => 'Downloads';
  String get notifications => 'Notifications';
  String get settings => 'Settings';
  String get mediaDetails => 'Details';
  String get play => 'Play';
  String get playOffline => 'Play offline';
  String get saveOffline => 'Save for offline playback';
  String get offline => 'Offline';
  String get browseLibraries => 'Browse libraries';
  String get recentlyAdded => 'Recently added';
  String get noServerSelected => 'No server selected';
  String get emptyLibraries => 'No libraries are available for this account.';
  String get emptyItems => 'No media items found.';
  String get emptyCollections => 'No collections found.';
  String get emptyDownloads => 'No downloads on this device.';
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
  String get quality => 'Quality';
  String get noSubtitle => 'No subtitle';
  String get restartPlayback => 'Restart playback';
  String get stop => 'Stop';
  String get pause => 'Pause';
  String get resume => 'Resume';
  String get cancel => 'Cancel';
  String get delete => 'Delete';
  String get deleteAll => 'Delete all';
  String get buffering => 'Buffering';
  String get playbackFailed => 'Playback failed. Try another quality or stream option.';
  String skipSegment(String segmentType) => 'Skip $segmentType';
  String get markAllRead => 'Mark all read';
  String get unread => 'Unread';
  String get read => 'Read';
  String get download => 'Download';
  String get downloadQueued => 'Download queued.';
  String get downloadSettings => 'Download settings';
  String get wifiOnly => 'Wi-Fi only';
  String get allowCellular => 'Allow cellular';
  String get chargingOnly => 'Charging only';
  String get pauseOnLowStorage => 'Pause on low storage';
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
