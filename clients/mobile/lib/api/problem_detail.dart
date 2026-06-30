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
