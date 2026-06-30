import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/api/problem_detail.dart';
import 'package:dio/dio.dart';

class DuskcueApiClient {
  DuskcueApiClient({Dio? dio}) : _dio = dio ?? Dio();

  final Dio _dio;
  Uri? _serverOrigin;
  String? _bearerToken;

  void configure(Uri serverOrigin, {String? bearerToken}) {
    _serverOrigin = serverOrigin;
    _bearerToken = bearerToken;
    _dio.options = BaseOptions(
      baseUrl: serverOrigin.toString(),
      connectTimeout: const Duration(seconds: 10),
      receiveTimeout: const Duration(seconds: 30),
      headers: {
        if (bearerToken != null) 'Authorization': 'Bearer $bearerToken',
      },
    );
  }

  void setBearerToken(String token) {
    _bearerToken = token;
    _dio.options.headers['Authorization'] = 'Bearer $token';
  }

  void clearBearerToken() {
    _bearerToken = null;
    _dio.options.headers.remove('Authorization');
  }

  bool get isConfigured => _serverOrigin != null;

  Uri? get serverOrigin => _serverOrigin;

  String? get bearerToken => _bearerToken;

  Uri absoluteUri(String path, {Map<String, Object?>? query}) {
    final origin = _serverOrigin;
    if (origin == null) {
      throw StateError('DuskcueApiClient is not configured.');
    }
    return origin.replace(
      path: path,
      queryParameters: query == null
          ? null
          : {
              for (final entry in query.entries)
                if (entry.value != null) entry.key: entry.value.toString(),
            },
    );
  }

  Future<bool> ready() async {
    final response = await get('/health/ready');
    return response.statusCode == 200;
  }

  Future<Response<T>> get<T>(String path, {Map<String, Object?>? query, Map<String, Object?>? headers}) {
    return _request(() => _dio.get<T>(path, queryParameters: query, options: _options(headers)));
  }

  Future<Response<List<int>>> getBytes(String path, {Map<String, Object?>? query, Map<String, Object?>? headers}) {
    return _request(
      () => _dio.get<List<int>>(
        path,
        queryParameters: query,
        options: Options(
          responseType: ResponseType.bytes,
          headers: headers == null ? null : Map<String, dynamic>.from(headers),
        ),
      ),
    );
  }

  Future<Response<T>> post<T>(String path, {Object? body, Map<String, Object?>? headers}) {
    return _request(() => _dio.post<T>(path, data: body, options: _options(headers)));
  }

  Future<Response<T>> put<T>(String path, {Object? body, Map<String, Object?>? headers}) {
    return _request(() => _dio.put<T>(path, data: body, options: _options(headers)));
  }

  Future<Response<T>> patch<T>(String path, {Object? body, Map<String, Object?>? headers}) {
    return _request(() => _dio.patch<T>(path, data: body, options: _options(headers)));
  }

  Future<Response<T>> delete<T>(String path, {Map<String, Object?>? headers}) {
    return _request(() => _dio.delete<T>(path, options: _options(headers)));
  }

  Future<Response<ResponseBody>> stream(
    String path, {
    Map<String, Object?>? query,
    Map<String, Object?>? headers,
  }) {
    return _request(
      () => _dio.get<ResponseBody>(
        path,
        queryParameters: query,
        options: Options(
          responseType: ResponseType.stream,
          headers: {
            'Accept': 'text/event-stream',
            if (headers != null) ...Map<String, dynamic>.from(headers),
          },
        ),
      ),
    );
  }

  Options? _options(Map<String, Object?>? headers) {
    return headers == null ? null : Options(headers: Map<String, dynamic>.from(headers));
  }

  Future<Response<T>> _request<T>(Future<Response<T>> Function() send) async {
    try {
      return await send();
    } on DioException catch (error) {
      throw _toClientError(error);
    }
  }

  ClientError _toClientError(DioException error) {
    final response = error.response;
    if (response == null) {
      return ClientError(
        problem: ProblemDetail.network(error.message ?? 'Network request failed'),
      );
    }

    final data = response.data;
    final problem = data is Map
        ? ProblemDetail.fromJson(Map<String, Object?>.from(data))
        : ProblemDetail(
            type: '/errors/http_${response.statusCode ?? 0}',
            title: 'HTTP_${response.statusCode ?? 0}',
            status: response.statusCode ?? 0,
            detail: response.statusMessage ?? 'Request failed',
            instance: response.requestOptions.path,
            traceId: '',
            errors: null,
          );
    final retryAfterHeader = response.headers.value('retry-after');
    final retryAfterSeconds = retryAfterHeader == null ? null : int.tryParse(retryAfterHeader);

    return ClientError(
      problem: problem,
      retryAfter: retryAfterSeconds == null ? null : Duration(seconds: retryAfterSeconds),
    );
  }
}
