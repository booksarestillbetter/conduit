import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

class SessionManager {
  static const String _keyServerUrl = 'conduit_server_url';
  static const String _keyAuthToken = 'conduit_jwt_token';
  static const String _keyDeviceName = 'conduit_device_name';
  static const String _keyUsername = 'conduit_username';
  static const String _keySetupCompleted = 'conduit_setup_completed';

  // The auth token is a live credential granting full API access, so it lives in the
  // platform keystore (Android Keystore / iOS Keychain) instead of SharedPreferences,
  // which on both platforms is plaintext on disk and readable from an unencrypted
  // backup or a rooted/jailbroken device. Everything else here (server URL, username,
  // setup-completed flag) isn't sensitive and stays in SharedPreferences.
  static const FlutterSecureStorage _secureStorage = FlutterSecureStorage();

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
    await _secureStorage.write(key: _keyAuthToken, value: token);
    if (username != null) await prefs.setString(_keyUsername, username);
    if (deviceName != null) await prefs.setString(_keyDeviceName, deviceName);
  }

  static Future<String?> getServerUrl() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_keyServerUrl);
  }

  static Future<String?> getAuthToken() async {
    final secureToken = await _secureStorage.read(key: _keyAuthToken);
    if (secureToken != null) return secureToken;

    // One-time migration: a token saved by a version of this app before secure
    // storage was introduced is still sitting in plaintext SharedPreferences. Move it
    // over (and wipe the plaintext copy) instead of silently logging the user out on
    // upgrade.
    final prefs = await SharedPreferences.getInstance();
    final legacyToken = prefs.getString(_keyAuthToken);
    if (legacyToken != null && legacyToken.isNotEmpty) {
      await _secureStorage.write(key: _keyAuthToken, value: legacyToken);
      await prefs.remove(_keyAuthToken);
      return legacyToken;
    }
    return null;
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
    await prefs.remove(_keyUsername);
    await prefs.remove(_keySetupCompleted);
    await prefs.remove(_keyAuthToken); // pre-secure-storage legacy key, if never migrated
    await _secureStorage.delete(key: _keyAuthToken);
  }
}
