import 'package:shared_preferences/shared_preferences.dart';

class SessionManager {
  static const String _keyServerUrl = 'conduit_server_url';
  static const String _keyAuthToken = 'conduit_jwt_token';
  static const String _keyDeviceName = 'conduit_device_name';
  static const String _keyUsername = 'conduit_username';
  static const String _keySetupCompleted = 'conduit_setup_completed';

  static Future<void> saveSession({
    required String serverUrl,
    required String token,
    String? username,
    String? deviceName,
  }) async {
    final prefs = await SharedPreferences.getInstance();
    // Normalize URL: remove trailing slash
    var url = serverUrl.trim();
    if (url.endsWith('/')) {
      url = url.substring(0, url.length - 1);
    }
    await prefs.setString(_keyServerUrl, url);
    await prefs.setString(_keyAuthToken, token);
    if (username != null) await prefs.setString(_keyUsername, username);
    if (deviceName != null) await prefs.setString(_keyDeviceName, deviceName);
  }

  static Future<String?> getServerUrl() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_keyServerUrl);
  }

  static Future<String?> getAuthToken() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_keyAuthToken);
  }

  static Future<String?> getUsername() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_keyUsername);
  }

  static Future<bool> isAuthenticated() async {
    final token = await getAuthToken();
    final url = await getServerUrl();
    return token != null && token.isNotEmpty && url != null && url.isNotEmpty;
  }

  static Future<bool> isSetupCompleted() async {
    final prefs = await SharedPreferences.getInstance();
    final completed = prefs.getBool(_keySetupCompleted);
    if (completed != null) return completed;
    // Fallback: if already authenticated from a previous session, treat setup as completed
    return await isAuthenticated();
  }

  static Future<void> setSetupCompleted(bool completed) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_keySetupCompleted, completed);
  }

  static Future<void> resetSetupWizard() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_keySetupCompleted, false);
  }

  static Future<void> clearSession() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(_keyServerUrl);
    await prefs.remove(_keyAuthToken);
    await prefs.remove(_keyUsername);
    await prefs.remove(_keySetupCompleted);
  }
}
