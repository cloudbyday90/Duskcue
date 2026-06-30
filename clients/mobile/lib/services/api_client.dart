import 'package:dio/dio.dart';

class DuskcueApiClient {
  DuskcueApiClient({Dio? dio}) : _dio = dio ?? Dio();

  final Dio _dio;

  void configure(Uri serverOrigin, {String? bearerToken}) {
    _dio.options = BaseOptions(
      baseUrl: serverOrigin.toString(),
      connectTimeout: const Duration(seconds: 10),
      receiveTimeout: const Duration(seconds: 30),
      headers: {
        if (bearerToken != null) 'Authorization': 'Bearer $bearerToken',
      },
    );
  }

  Future<bool> ready() async {
    final response = await _dio.get<Object?>('/health/ready');
    return response.statusCode == 200;
  }
}
