import 'package:dio/dio.dart';
import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('maps RFC 9457 rate limit responses with retry-after', () {
    final requestOptions = RequestOptions(path: '/api/v1/playback/start');
    final error = DioException(
      requestOptions: requestOptions,
      response: Response<Map<String, Object?>>(
        requestOptions: requestOptions,
        statusCode: 429,
        headers: Headers.fromMap({'retry-after': ['12']}),
        data: {
          'type': '/errors/rate_limited',
          'title': 'RATE_LIMITED',
          'status': 429,
          'detail': 'Too many requests',
          'instance': '/api/v1/playback/start',
          'trace_id': 'trace-1',
        },
      ),
    );

    final mapped = clientErrorFromDioException(error);

    expect(mapped.kind, ClientErrorKind.rateLimited);
    expect(mapped.retryAfter, const Duration(seconds: 12));
    expect(mapped.toString(), 'Too many requests');
  });

  test('maps network failures without a response', () {
    final error = DioException(
      requestOptions: RequestOptions(path: '/health/ready'),
      message: 'Connection refused',
    );

    final mapped = clientErrorFromDioException(error);

    expect(mapped.kind, ClientErrorKind.network);
    expect(mapped.problem.status, 0);
    expect(mapped.toString(), 'Connection refused');
  });

  test('maps non-problem HTTP responses to fallback problem detail', () {
    final requestOptions = RequestOptions(path: '/api/v1/items/missing');
    final error = DioException(
      requestOptions: requestOptions,
      response: Response<String>(
        requestOptions: requestOptions,
        statusCode: 404,
        statusMessage: 'Not Found',
        data: 'missing',
      ),
    );

    final mapped = clientErrorFromDioException(error);

    expect(mapped.kind, ClientErrorKind.notFound);
    expect(mapped.problem.title, 'HTTP_404');
    expect(mapped.problem.instance, '/api/v1/items/missing');
  });
}
