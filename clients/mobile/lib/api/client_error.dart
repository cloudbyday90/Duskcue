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
