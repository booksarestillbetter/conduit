import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:http/http.dart' as http;
import '../storage/session_manager.dart';
import '../models/torrent.dart';
import '../models/node.dart';
import '../models/media_grab.dart';
import '../models/circuit_breaker.dart';
import '../models/peer.dart';

class ReleaseCandidate {
  final String guid;
  final String title;
  final String? indexer;
  final int indexerId;
  final int size;
  final int seeders;
  final int leechers;
  final String? quality;
  final int ageMinutes;
  final String? protocol;
  final String? downloadUrl;

  ReleaseCandidate({
    required this.guid,
    required this.title,
    this.indexer,
    required this.indexerId,
    required this.size,
    this.seeders = 0,
    this.leechers = 0,
    this.quality,
    this.ageMinutes = 0,
    this.protocol,
    this.downloadUrl,
  });

  factory ReleaseCandidate.fromJson(Map<String, dynamic> json) {
    return ReleaseCandidate(
      guid: json['guid']?.toString() ?? '',
      title: json['title']?.toString() ?? 'Release',
      indexer: json['indexer']?.toString(),
      indexerId: (json['indexerId'] as num?)?.toInt() ?? (json['indexer_id'] as num?)?.toInt() ?? 0,
      size: (json['size'] as num?)?.toInt() ?? 0,
      seeders: (json['seeders'] as num?)?.toInt() ?? 0,
      leechers: (json['leechers'] as num?)?.toInt() ?? 0,
      quality: json['quality'] is Map ? (json['quality']['quality']?['name']?.toString() ?? json['quality']?['name']?.toString()) : json['quality']?.toString(),
      ageMinutes: (json['ageMinutes'] as num?)?.toInt() ?? (json['age_minutes'] as num?)?.toInt() ?? 0,
      protocol: json['protocol']?.toString(),
      downloadUrl: json['downloadUrl']?.toString() ?? json['download_url']?.toString(),
    );
  }
}

class ApiClient {
  static const Duration _timeout = Duration(seconds: 10);

  static Future<Map<String, String>> _getHeaders({bool requireAuth = true}) async {
    final headers = {
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    };
    if (requireAuth) {
      final token = await SessionManager.getAuthToken();
      if (token != null && token.isNotEmpty) {
        headers['Authorization'] = 'Bearer $token';
      }
    }
    return headers;
  }

  // --- Authentication ---

  static Future<Map<String, dynamic>> pairWithQr({
    required String serverUrl,
    required String pairCode,
    String? deviceName,
  }) async {
    var baseUrl = serverUrl.trim();
    if (baseUrl.endsWith('/')) baseUrl = baseUrl.substring(0, baseUrl.length - 1);

    final uri = Uri.parse('$baseUrl/api/auth/mobile/pair');
    try {
      final response = await http.post(
        uri,
        headers: {'Content-Type': 'application/json'},
        body: jsonEncode({
          'pair_code': pairCode,
          'device_name': deviceName ?? (Platform.isIOS ? 'Conduit iOS' : 'Conduit Android'),
        }),
      ).timeout(_timeout, onTimeout: () {
        throw TimeoutException('Connection timed out connecting to $baseUrl. Verify server address and network connectivity.');
      });

      if (response.statusCode >= 200 && response.statusCode < 300) {
        final data = jsonDecode(response.body) as Map<String, dynamic>;
        final token = data['token'] as String;
        final user = data['user'] as Map<String, dynamic>?;
        await SessionManager.saveSession(
          serverUrl: baseUrl,
          token: token,
          username: user?['username']?.toString(),
          deviceName: deviceName,
        );
        return data;
      } else {
        try {
          final error = jsonDecode(response.body);
          throw Exception(error['error'] ?? error['message'] ?? 'Failed to pair device (${response.statusCode})');
        } catch (e) {
          if (e is Exception && !e.toString().contains('FormatException')) rethrow;
          throw Exception('Pairing failed: HTTP ${response.statusCode}');
        }
      }
    } on TimeoutException {
      rethrow;
    } catch (e) {
      if (e.toString().contains('Failed host lookup') ||
          e.toString().contains('Connection refused') ||
          e.toString().contains('Network is unreachable') ||
          e.toString().contains('OS Error')) {
        throw Exception('Cannot reach server at $baseUrl. Check network connection or server address.');
      }
      rethrow;
    }
  }

  static Future<Map<String, dynamic>> login({
    required String serverUrl,
    required String username,
    required String password,
    String? totpCode,
  }) async {
    var baseUrl = serverUrl.trim();
    if (baseUrl.endsWith('/')) baseUrl = baseUrl.substring(0, baseUrl.length - 1);

    final uri = Uri.parse('$baseUrl/api/auth/login');
    final response = await http.post(
      uri,
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'username': username,
        'password': password,
        if (totpCode != null && totpCode.isNotEmpty) 'totp_code': totpCode,
      }),
    ).timeout(_timeout);

    if (response.statusCode >= 200 && response.statusCode < 300) {
      final data = jsonDecode(response.body) as Map<String, dynamic>;
      final token = data['token'] as String;
      await SessionManager.saveSession(
        serverUrl: baseUrl,
        token: token,
        username: username,
      );
      return data;
    } else {
      final error = jsonDecode(response.body);
      throw Exception(error['error'] ?? error['message'] ?? 'Login failed (${response.statusCode})');
    }
  }

  static Future<String> issueWsTicket() async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) throw Exception('No server configured');

    final uri = Uri.parse('$baseUrl/api/auth/ws-ticket');
    final headers = await _getHeaders();
    final response = await http.post(uri, headers: headers).timeout(_timeout);

    if (response.statusCode == 200) {
      final data = jsonDecode(response.body) as Map<String, dynamic>;
      return data['ticket'] as String;
    } else {
      throw Exception('Failed to issue WebSocket ticket (${response.statusCode})');
    }
  }

  // --- Torrents ---

  static Future<List<Torrent>> getTorrents({String? node, String? zone}) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return [];

    final queryParams = <String, String>{};
    if (node != null && node != 'all') queryParams['node'] = node;
    if (zone != null && zone.isNotEmpty) queryParams['zone'] = zone;

    final uri = Uri.parse('$baseUrl/api/torrents').replace(queryParameters: queryParams.isNotEmpty ? queryParams : null);
    final response = await http.get(uri, headers: await _getHeaders()).timeout(_timeout);
    if (response.statusCode == 200) {
      final data = jsonDecode(response.body);
      if (data is List) {
        return data.map((e) => Torrent.fromJson(e as Map<String, dynamic>)).toList();
      } else if (data is Map && data['torrents'] is List) {
        return (data['torrents'] as List)
            .map((e) => Torrent.fromJson(e as Map<String, dynamic>))
            .toList();
      }
    }
    return [];
  }

  static Future<Torrent?> getTorrentDetails(String compoundId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return null;

    final response = await http.get(
      Uri.parse('$baseUrl/api/torrents/$compoundId'),
      headers: await _getHeaders(),
    ).timeout(_timeout);

    if (response.statusCode == 200) {
      final data = jsonDecode(response.body) as Map<String, dynamic>;
      return Torrent.fromJson(data);
    }
    return null;
  }

  static Future<List<Peer>> getTorrentPeers(String compoundId) async {
    final t = await getTorrentDetails(compoundId);
    return t?.peers ?? [];
  }

  static Future<bool> startTorrent(String compoundId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/torrents/$compoundId/start'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> stopTorrent(String compoundId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/torrents/$compoundId/stop'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> deleteTorrent(String compoundId, {bool deleteFiles = false}) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.delete(
      Uri.parse('$baseUrl/api/torrents/$compoundId?delete_files=$deleteFiles'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> moveQueue(String compoundId, String direction) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/torrents/$compoundId/queue-move'),
      headers: await _getHeaders(),
      body: jsonEncode({'action': direction}),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> setSequential(String compoundId, bool enabled) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/torrents/$compoundId/sequential-download'),
      headers: await _getHeaders(),
      body: jsonEncode({'enabled': enabled}),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> reannounce(String compoundId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/torrents/$compoundId/reannounce'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> recheck(String compoundId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/torrents/$compoundId/recheck'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  // --- Turtle Mode ---

  static Future<bool> toggleTurtleMode(bool enabled) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/nodes/turtle-mode'),
      headers: await _getHeaders(),
      body: jsonEncode({'enabled': enabled}),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  // --- Nodes & Health ---

  static Future<List<TransmissionNode>> getNodes() async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return [];

    try {
      final response = await http.get(
        Uri.parse('$baseUrl/api/nodes'),
        headers: await _getHeaders(),
      ).timeout(_timeout);

      if (response.statusCode == 200 && response.body.trim().startsWith('[')) {
        final data = jsonDecode(response.body);
        if (data is List) {
          return data.map((e) => TransmissionNode.fromJson(e as Map<String, dynamic>)).toList();
        }
      }
    } catch (_) {}
    return [];
  }

  static Future<List<CircuitBreakerStatus>> getCircuitBreakers() async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return [];

    try {
      final response = await http.get(
        Uri.parse('$baseUrl/api/system/health'),
        headers: await _getHeaders(),
      ).timeout(_timeout);

      if (response.statusCode == 200 && response.body.trim().startsWith('{')) {
        final data = jsonDecode(response.body);
        if (data is Map<String, dynamic> && data['trackers'] is List) {
          return (data['trackers'] as List)
              .map((e) => CircuitBreakerStatus.fromJson(e as Map<String, dynamic>))
              .toList();
        }
      }
    } catch (_) {}
    return [];
  }

  // --- Settings & Zones ---

  static Future<Map<String, dynamic>> getSettings() async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return {};

    try {
      final response = await http.get(
        Uri.parse('$baseUrl/api/settings'),
        headers: await _getHeaders(),
      ).timeout(_timeout);

      if (response.statusCode == 200 && response.body.trim().startsWith('{')) {
        return jsonDecode(response.body) as Map<String, dynamic>;
      }
    } catch (_) {}
    return {};
  }

  // --- Media Pipeline & Ghost Archive ---

  static Future<List<MediaGrab>> getMediaGrabs({String? zone, int limit = 50}) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return [];

    try {
      final queryParams = <String, String>{'limit': limit.toString()};
      if (zone != null && zone.isNotEmpty) queryParams['zone'] = zone;

      final uri = Uri.parse('$baseUrl/api/arr/pipeline').replace(queryParameters: queryParams);
      final response = await http.get(uri, headers: await _getHeaders()).timeout(_timeout);

      if (response.statusCode == 200 && response.body.trim().startsWith('[')) {
        final data = jsonDecode(response.body);
        if (data is List) {
          return data.map((e) => MediaGrab.fromJson(e as Map<String, dynamic>)).toList();
        }
      }
    } catch (_) {}
    return [];
  }

  static Future<List<MediaGrab>> getGhostArchive({
    String? status,
    String? query,
    String? itemType,
    String? zone,
    int limit = 50,
  }) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return [];

    try {
      final queryParams = <String, String>{'limit': limit.toString()};
      if (status != null && status.isNotEmpty && status != 'all') queryParams['status'] = status;
      if (query != null && query.isNotEmpty) queryParams['q'] = query;
      if (itemType != null && itemType.isNotEmpty && itemType != 'all') queryParams['item_type'] = itemType;
      if (zone != null && zone.isNotEmpty) queryParams['zone'] = zone;

      final uri = Uri.parse('$baseUrl/api/arr/grabs').replace(queryParameters: queryParams);
      final response = await http.get(uri, headers: await _getHeaders()).timeout(_timeout);

      if (response.statusCode == 200 && response.body.trim().startsWith('[')) {
        final data = jsonDecode(response.body);
        if (data is List) {
          return data.map((e) => MediaGrab.fromJson(e as Map<String, dynamic>)).toList();
        }
      }
    } catch (_) {}
    return [];
  }

  static Future<bool> triggerResearch(String grabId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/arr/pipeline/$grabId/re-search'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<bool> deletePipelineItem(String grabId) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.delete(
      Uri.parse('$baseUrl/api/arr/pipeline/$grabId'),
      headers: await _getHeaders(),
    ).timeout(_timeout);
    return response.statusCode == 200;
  }

  static Future<List<ReleaseCandidate>> searchReleases({
    required String itemType,
    int? movieId,
    int? seriesId,
    int? episodeId,
    int? albumId,
    String? zoneId,
  }) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return [];

    final queryParams = <String, String>{'item_type': itemType};
    if (movieId != null) queryParams['movie_id'] = movieId.toString();
    if (seriesId != null) queryParams['series_id'] = seriesId.toString();
    if (episodeId != null) queryParams['episode_id'] = episodeId.toString();
    if (albumId != null) queryParams['album_id'] = albumId.toString();
    if (zoneId != null && zoneId.isNotEmpty) queryParams['zone_id'] = zoneId;

    final uri = Uri.parse('$baseUrl/api/arr/search/releases').replace(queryParameters: queryParams);
    final response = await http.get(uri, headers: await _getHeaders()).timeout(const Duration(seconds: 25));

    if (response.statusCode == 200) {
      final data = jsonDecode(response.body);
      if (data is List) {
        return data.map((e) => ReleaseCandidate.fromJson(e as Map<String, dynamic>)).toList();
      }
    }
    return [];
  }

  static Future<bool> grabRelease({
    required String itemType,
    required String guid,
    required int indexerId,
    String? zoneId,
  }) async {
    final baseUrl = await SessionManager.getServerUrl();
    if (baseUrl == null) return false;

    final response = await http.post(
      Uri.parse('$baseUrl/api/arr/search/grab'),
      headers: await _getHeaders(),
      body: jsonEncode({
        'item_type': itemType,
        'guid': guid,
        'indexer_id': indexerId,
        ...?zoneId != null ? {'zone_id': zoneId} : null,
      }),
    ).timeout(const Duration(seconds: 25));

    return response.statusCode == 200;
  }
}
