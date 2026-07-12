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

import 'package:duskcue_mobile/api/problem_detail.dart';

enum ClientErrorKind {
  network,
  authExpired,
  permissionDenied,
  rateLimited,
  notFound,
  conflict,
  validation,
  serverUnavailable,
  transcodeUnavailable,
  playbackPolicy,
  unknown,
}

class ClientError implements Exception {
  const ClientError({
    required this.problem,
    this.retryAfter,
  });

  final ProblemDetail problem;
  final Duration? retryAfter;

  ClientErrorKind get kind {
    if (problem.status == 0) return ClientErrorKind.network;
    if (problem.title.startsWith('PLAY_') && problem.status == 503) {
      return ClientErrorKind.transcodeUnavailable;
    }
    if (problem.title.startsWith('PLAY_') && problem.status == 403) {
      return ClientErrorKind.playbackPolicy;
    }
    if (problem.status == 401) return ClientErrorKind.authExpired;
    if (problem.status == 403) return ClientErrorKind.permissionDenied;
    if (problem.status == 404) return ClientErrorKind.notFound;
    if (problem.status == 409) return ClientErrorKind.conflict;
    if (problem.status == 429) return ClientErrorKind.rateLimited;
    if (problem.title == 'VALID_001' || problem.status == 422 || problem.errors != null) {
      return ClientErrorKind.validation;
    }
    if (problem.status == 503 || problem.status == 504) {
      return ClientErrorKind.serverUnavailable;
    }
    return ClientErrorKind.unknown;
  }

  @override
  String toString() {
    return problem.detail.isEmpty ? problem.title : problem.detail;
  }
}
