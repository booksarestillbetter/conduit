import 'package:flutter/material.dart';
import '../core/storage/session_manager.dart';
import '../core/network/api_client.dart';

enum AuthStatus { unknown, unauthenticated, authenticating, authenticated }

class AuthProvider extends ChangeNotifier {
  AuthStatus _status = AuthStatus.unknown;
  String? _serverUrl;
  String? _username;
  String? _errorMessage;
  bool _setupCompleted = false;

  AuthStatus get status => _status;
  String? get serverUrl => _serverUrl;
  String? get username => _username;
  String? get errorMessage => _errorMessage;
  bool get isAuthenticated => _status == AuthStatus.authenticated;
  bool get isSetupCompleted => _setupCompleted;

  Future<void> checkAuth() async {
    final authed = await SessionManager.isAuthenticated();
    _setupCompleted = await SessionManager.isSetupCompleted();
    if (authed) {
      _serverUrl = await SessionManager.getServerUrl();
      _username = await SessionManager.getUsername();
      _status = AuthStatus.authenticated;
    } else {
      _status = AuthStatus.unauthenticated;
    }
    notifyListeners();
  }

  Future<void> completeSetup() async {
    await SessionManager.setSetupCompleted(true);
    _setupCompleted = true;
    notifyListeners();
  }

  Future<void> restartSetupWizard() async {
    await SessionManager.setSetupCompleted(false);
    _setupCompleted = false;
    notifyListeners();
  }

  Future<bool> pairWithQr({
    required String serverUrl,
    required String pairCode,
    String? deviceName,
  }) async {
    _status = AuthStatus.authenticating;
    _errorMessage = null;
    notifyListeners();

    try {
      final result = await ApiClient.pairWithQr(
        serverUrl: serverUrl,
        pairCode: pairCode,
        deviceName: deviceName,
      );
      _serverUrl = serverUrl;
      final user = result['user'] as Map<String, dynamic>?;
      _username = user?['username']?.toString();
      _status = AuthStatus.authenticated;
      notifyListeners();
      return true;
    } catch (e) {
      _status = AuthStatus.unauthenticated;
      _errorMessage = e.toString().replaceAll('Exception: ', '');
      notifyListeners();
      return false;
    }
  }

  Future<bool> login({
    required String serverUrl,
    required String username,
    required String password,
    String? totpCode,
  }) async {
    _status = AuthStatus.authenticating;
    _errorMessage = null;
    notifyListeners();

    try {
      await ApiClient.login(
        serverUrl: serverUrl,
        username: username,
        password: password,
        totpCode: totpCode,
      );
      _serverUrl = serverUrl;
      _username = username;
      _status = AuthStatus.authenticated;
      notifyListeners();
      return true;
    } catch (e) {
      _status = AuthStatus.unauthenticated;
      _errorMessage = e.toString().replaceAll('Exception: ', '');
      notifyListeners();
      return false;
    }
  }

  Future<void> logout() async {
    await SessionManager.clearSession();
    _status = AuthStatus.unauthenticated;
    _setupCompleted = false;
    _serverUrl = null;
    _username = null;
    _errorMessage = null;
    notifyListeners();
  }
}
