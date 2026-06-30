import 'package:connectivity_plus/connectivity_plus.dart';

class ConnectivityService {
  ConnectivityService({Connectivity? connectivity})
      : _connectivity = connectivity ?? Connectivity();

  final Connectivity _connectivity;

  Stream<List<ConnectivityResult>> get changes => _connectivity.onConnectivityChanged;

  Future<List<ConnectivityResult>> current() {
    return _connectivity.checkConnectivity();
  }
}
