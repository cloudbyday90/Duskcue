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

class FieldError {
  const FieldError({
    required this.field,
    required this.code,
    required this.message,
  });

  final String field;
  final String code;
  final String message;

  factory FieldError.fromJson(Map<String, Object?> json) {
    return FieldError(
      field: json['field'] as String? ?? '',
      code: json['code'] as String? ?? '',
      message: json['message'] as String? ?? '',
    );
  }
}

class ProblemDetail {
  const ProblemDetail({
    required this.type,
    required this.title,
    required this.status,
    required this.detail,
    required this.instance,
    required this.traceId,
    required this.errors,
  });

  final String type;
  final String title;
  final int status;
  final String detail;
  final String instance;
  final String traceId;
  final List<FieldError>? errors;

  factory ProblemDetail.fromJson(Map<String, Object?> json) {
    final errors = json['errors'];

    return ProblemDetail(
      type: json['type'] as String? ?? '',
      title: json['title'] as String? ?? '',
      status: json['status'] as int? ?? 0,
      detail: json['detail'] as String? ?? '',
      instance: json['instance'] as String? ?? '',
      traceId: json['trace_id'] as String? ?? '',
      errors: errors is List
          ? errors
              .whereType<Map>()
              .map((item) => FieldError.fromJson(Map<String, Object?>.from(item)))
              .toList(growable: false)
          : null,
    );
  }

  factory ProblemDetail.network(String message) {
    return ProblemDetail(
      type: '/errors/network',
      title: 'NETWORK_ERROR',
      status: 0,
      detail: message,
      instance: '',
      traceId: '',
      errors: null,
    );
  }
}
