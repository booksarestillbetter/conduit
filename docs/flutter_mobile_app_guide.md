# Conduit Flutter Mobile App Client & Architecture Guide

This document outlines the complete architectural design, API integration protocols, and implementation blueprint for building the native **Conduit Mobile Application** using **Flutter** (iOS & Android) with 100% feature parity to the Conduit Web Conduit.

---

## 1. Architectural Overview & System Stack

The Conduit Mobile App is designed as an autonomous, high-performance client connecting to one or multiple Conduit daemon instances.

```
┌──────────────────────────────────────────────────────────────┐
│                    Conduit Flutter Mobile App                  │
│                                                              │
│  ┌────────────────────────┐      ┌────────────────────────┐  │
│  │   UI & Presentation    │      │  State Management      │  │
│  │  (Material 3 / Cupertino)│    │  (Riverpod 2.x Async)  │  │
│  └───────────┬────────────┘      └───────────┬────────────┘  │
│              │                               │               │
│  ┌───────────▼───────────────────────────────▼────────────┐  │
│  │                     Repository Layer                   │  │
│  │    • TorrentRepository    • ArrPipelineRepository     │  │
│  │    • TelemetryRepository  • NodeControlRepository     │  │
│  └───────────┬───────────────────────────────┬────────────┘  │
│              │                               │               │
│  ┌───────────▼────────────┐      ┌───────────▼────────────┐  │
│  │  REST API (Dio + Auth) │      │  WebSocket Telemetry   │  │
│  │  • Short-lived Ticket  │      │  • Live 1s Bandwidth   │  │
│  │  • JWT Interceptor     │      │  • Auto-Reconnect WS   │  │
│  └───────────┬────────────┘      └───────────┬────────────┘  │
└──────────────┼───────────────────────────────┼───────────────┘
               │ HTTP / JSON                   │ WS Frame
┌──────────────▼───────────────────────────────▼───────────────┐
│                     Conduit Rust Server Backend                │
│              (Pacoli / Conduit Cluster Daemon)                 │
└──────────────────────────────────────────────────────────────┘
```

### Core Technologies
- **Framework**: Flutter 3.24+ (Dart 3.5+)
- **State Management**: `flutter_riverpod` (AsyncNotifier with code generation)
- **Networking**: `dio` (with Bearer Token interceptors) + `web_socket_channel`
- **Security & Storage**: `flutter_secure_storage` (iOS Keychain / Android EncryptedSharedPreferences) + `hive_flutter` for offline cache
- **QR Code Scanning**: `mobile_scanner` (hardware camera feed with MLKit barcode recognition)
- **Visualizations**: `fl_chart` or custom canvas `CustomPainter` for the 5-minute rolling bandwidth curves and piece map heatmaps
- **Haptics & Native Polish**: `flutter_vibrate` for interactive fetcher controls and swipe gestures

---

## 2. Zero-Touch Mobile Authentication & QR Code Pairing

To eliminate typing lengthy passwords or manual TOTP codes on mobile devices, Conduit provides a secure, single-use **QR Code Pairing Protocol**:

### A. Pairing Protocol Flow

```
Web Dashboard (Admin)                               Mobile App (Flutter)
     │                                                      │
     │ 1. Clicks "Pair Mobile App"                          │
     │ ──── POST /api/auth/mobile/pair-token ────►          │
     │ ◄─── Returns { pair_code, qr_payload } ───           │
     │                                                      │
     │ 2. Displays QR Code on screen                        │
     │                                  3. Scans QR code with camera
     │                                  4. Extracts conduit_url & pair_code
     │                                  5. POST /api/auth/mobile/pair
     │                                     { pair_code, device_name }
     │                                     │
     │                                     ▼
     │                          ◄── Returns 90-Day Mobile JWT Token
     │                          6. Saves token in SecureStorage
     │                          7. Establishes Authenticated WebSocket
```

### B. QR Code Payload Schema
When decoded by the mobile scanner, the QR payload contains JSON:
```json
{
  "conduit_url": "https://conduit.example.com:4242",
  "pair_code": "conduit_pair_9f31a7c0d58849688bc653a1a9e7019f",
  "user_id": "usr_99a812b",
  "username": "admin",
  "expires_at": "2026-08-28T02:00:00Z"
}
```

### C. Redeeming Mobile Pairing Code in Dart
```dart
// lib/features/auth/services/auth_service.dart
import 'package:dio/dio.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

class MobileAuthService {
  final Dio _dio = Dio();
  final FlutterSecureStorage _storage = const FlutterSecureStorage();

  Future<bool> redeemQrPairing({
    required String serverUrl,
    required String pairCode,
    required String deviceName,
  }) async {
    try {
      final response = await _dio.post(
        '$serverUrl/api/auth/mobile/pair',
        data: {
          'pair_code': pairCode,
          'device_name': deviceName,
        },
      );

      if (response.statusCode == 200) {
        final token = response.data['token'] as String;
        await _storage.write(key: 'conduit_server_url', value: serverUrl);
        await _storage.write(key: 'conduit_jwt_token', value: token);
        return true;
      }
      return false;
    } catch (e) {
      return false;
    }
  }
}
```

---

## 3. Real-Time Telemetry & WebSocket Connection

Conduit provides a high-throughput WebSocket stream at `/api/ws` broadcasting live bandwidth snapshots, torrent progress, piece completion events, and circuit breaker status.

### A. Ticketed Handshake Protocol
Before opening the WebSocket connection, the Flutter app issues a 60-second single-use ticket via REST, preventing exposing raw JWT tokens in WebSocket URL connection strings:

```dart
// lib/core/network/websocket_client.dart
import 'dart:async';
import 'dart:convert';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:dio/dio.dart';

class ConduitWebSocketClient {
  final String serverUrl;
  final String jwtToken;
  WebSocketChannel? _channel;
  final StreamController<Map<String, dynamic>> _streamController = StreamController.broadcast();

  ConduitWebSocketClient({required this.serverUrl, required this.jwtToken});

  Stream<Map<String, dynamic>> get stream => _streamController.stream;

  Future<void> connect() async {
    // 1. Issue a single-use WS ticket
    final dio = Dio();
    final res = await dio.post(
      '$serverUrl/api/auth/ws-ticket',
      options: Options(headers: {'Authorization': 'Bearer $jwtToken'}),
    );
    final ticket = res.data['ticket'] as String;

    // 2. Form ws:// or wss:// URL
    final wsBase = serverUrl.replaceFirst(RegExp(r'^http'), 'ws');
    final wsUri = Uri.parse('$wsBase/api/ws?ticket=$ticket');

    // 3. Connect and listen
    _channel = WebSocketChannel.connect(wsUri);
    _channel!.stream.listen(
      (data) {
        final decoded = jsonDecode(data as String) as Map<String, dynamic>;
        _streamController.add(decoded);
      },
      onError: (err) => _reconnect(),
      onDone: () => _reconnect(),
    );
  }

  void _reconnect() {
    Future.delayed(const Duration(seconds: 3), () {
      connect();
    });
  }

  void disconnect() {
    _channel?.sink.close();
  }
}
```

---

## 4. Feature Parity Implementation Matrix

The mobile app implements complete parity with the Conduit web interface:

| Web Conduit Feature | Mobile Implementation | Mobile Component / Pattern |
| :--- | :--- | :--- |
| **5m Cluster Bandwidth Chart** | Interactive Touch Bezier Chart | `fl_chart` with horizontal pan/zoom and scrub tooltips |
| **5m Per-Torrent Bandwidth** | Detailed Fetcher Modal | Live 1s download/upload curves for active seeding & downloading |
| **Piece Map Grid** | Interactive Heatmap | Virtualized 2D canvas grid with piece progress inspection |
| **Swarm Peers List** | Tabbed Swarm Sheet | Client flags (`U`, `D`, `E`, `?`), peer speed, IP geolocations |
| **Queue Prioritization** | Reorderable ListView | Drag-and-drop handles for reordering fetcher priorities |
| **Multi-Torrent Bulk Actions** | Long-Press Multi-Select | Bottom contextual action bar (Pause, Resume, Move, Delete) |
| **Conduit's Scent Trail** | Media Detail Modal | Poster viewer, intake timeline, Sonarr/Radarr/Lidarr re-search trigger |
| **Remote Node Controls** | Node Settings BottomSheet | Remote RPC port testing, blocklist updates, session speed limits |
| **Global Turtle Mode** | Quick Action AppBar Icon | Instant throttle toggle with persistent visual indicator badge |
| **Tracker Circuit Breakers** | Health Alert Banner | Canary probe indicator and automatic resume status |

---

## 5. Recommended Flutter Project Layout

```
conduit_mobile/
├── android/
├── ios/
├── lib/
│   ├── main.dart
│   ├── app.dart
│   ├── core/
│   │   ├── constants/
│   │   │   └── api_constants.dart
│   │   ├── network/
│   │   │   ├── dio_client.dart
│   │   │   └── websocket_client.dart
│   │   ├── theme/
│   │   │   ├── app_colors.dart
│   │   │   └── app_theme.dart
│   │   └── utils/
│   │       ├── formatters.dart
│   │       └── media_parser.dart
│   ├── features/
│   │   ├── auth/
│   │   │   ├── presentation/
│   │   │   │   ├── qr_scanner_screen.dart
│   │   │   │   └── manual_login_screen.dart
│   │   │   └── providers/
│   │   │       └── auth_provider.dart
│   │   ├── dashboard/
│   │   │   ├── presentation/
│   │   │   │   ├── dashboard_screen.dart
│   │   │   │   ├── widgets/
│   │   │   │   │   ├── bandwidth_area_chart.dart
│   │   │   │   │   ├── cluster_health_card.dart
│   │   │   │   │   └── node_gauges_row.dart
│   │   │   └── providers/
│   │   │       └── dashboard_provider.dart
│   │   ├── fetchers/
│   │   │   ├── presentation/
│   │   │   │   ├── fetchers_list_screen.dart
│   │   │   │   ├── fetcher_detail_screen.dart
│   │   │   │   ├── widgets/
│   │   │   │   │   ├── piece_map_canvas.dart
│   │   │   │   │   ├── peers_table.dart
│   │   │   │   │   └── torrent_bandwidth_chart.dart
│   │   │   └── providers/
│   │   │       └── torrents_provider.dart
│   │   ├── pipeline/
│   │   │   ├── presentation/
│   │   │   │   ├── pipeline_screen.dart
│   │   │   │   ├── media_detail_sheet.dart
│   │   │   │   └── widgets/
│   │   │   │       └── scent_trail_stepper.dart
│   │   │   └── providers/
│   │   │       └── pipeline_provider.dart
│   │   └── settings/
│   │       ├── presentation/
│   │       │   ├── settings_screen.dart
│   │       │   └── tracker_replace_dialog.dart
│   │       └── providers/
│   │           └── settings_provider.dart
│   └── models/
│       ├── torrent.dart
│       ├── aggregate_stats.dart
│       ├── bandwidth_point.dart
│       ├── arr_grab_record.dart
│       └── node_stats.dart
└── pubspec.yaml
```

---

## 6. Key Data Models (Freezed / JSON)

### Bandwidth Point
```dart
// lib/models/bandwidth_point.dart
import 'package:freezed_annotation/freezed_annotation.dart';

part 'bandwidth_point.freezed.dart';
part 'bandwidth_point.g.dart';

@freezed
class BandwidthPoint with _$BandwidthPoint {
  const factory BandwidthPoint({
    required int time,
    @JsonKey(name: 'downloadSpeed') required double downloadSpeed,
    @JsonKey(name: 'uploadSpeed') required double uploadSpeed,
  }) = _BandwidthPoint;

  factory BandwidthPoint.fromJson(Map<String, dynamic> json) =>
      _$BandwidthPointFromJson(json);
}
```

### Aggregate Stats
```dart
// lib/models/aggregate_stats.dart
import 'package:freezed_annotation/freezed_annotation.dart';
import 'bandwidth_point.dart';
import 'node_stats.dart';

part 'aggregate_stats.freezed.dart';
part 'aggregate_stats.g.dart';

@freezed
class AggregateStats with _$AggregateStats {
  const factory AggregateStats({
    @JsonKey(name: 'total_nodes') required int totalNodes,
    @JsonKey(name: 'connected_nodes') required int connectedNodes,
    @JsonKey(name: 'total_torrents') required int totalTorrents,
    @JsonKey(name: 'total_download_speed') required double totalDownloadSpeed,
    @JsonKey(name: 'total_upload_speed') required double totalUploadSpeed,
    @JsonKey(name: 'total_size_bytes') required int totalSizeBytes,
    required List<NodeStats> nodes,
    @JsonKey(name: 'bandwidth_history') List<BandwidthPoint>? bandwidthHistory,
  }) = _AggregateStats;

  factory AggregateStats.fromJson(Map<String, dynamic> json) =>
      _$AggregateStatsFromJson(json);
}
```

---

## 7. Interactive 5-Minute Bandwidth Chart Widget

```dart
// lib/features/dashboard/presentation/widgets/bandwidth_area_chart.dart
import 'package:flutter/material.dart';
import 'package:fl_chart/fl_chart.dart';
import '../../../../models/bandwidth_point.dart';

class BandwidthAreaChart extends StatelessWidget {
  final List<BandwidthPoint> history;
  final double height;

  const BandwidthAreaChart({
    super.key,
    required this.history,
    this.height = 160,
  });

  @override
  Widget build(BuildContext context) {
    if (history.length < 2) {
      return SizedBox(
        height: height,
        child: const Center(
          child: Text('Collecting cluster telemetry...', style: TextStyle(color: Colors.white38, fontSize: 12)),
        ),
      );
    }

    final double maxSpeed = history.fold(
      1024 * 1024.0,
      (max, p) => p.downloadSpeed > max ? p.downloadSpeed : (p.uploadSpeed > max ? p.uploadSpeed : max),
    ) * 1.1;

    final downSpots = <FlSpot>[];
    final upSpots = <FlSpot>[];

    for (int i = 0; i < history.length; i++) {
      downSpots.add(FlSpot(i.toDouble(), history[i].downloadSpeed));
      upSpots.add(FlSpot(i.toDouble(), history[i].uploadSpeed));
    }

    return Container(
      height: height,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: const Color(0xFF0F172A),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: const Color(0xFF1E293B)),
      ),
      child: LineChart(
        LineChartData(
          gridData: FlGridData(
            show: true,
            drawVerticalLine: true,
            getDrawingHorizontalLine: (val) => FlLine(color: const Color(0xFF334155), strokeWidth: 0.5, dashArray: [3, 3]),
            getDrawingVerticalLine: (val) => FlLine(color: const Color(0xFF334155), strokeWidth: 0.5, dashArray: [3, 3]),
          ),
          titlesData: FlTitlesData(
            leftTitles: AxisTitles(
              sideTitles: SideTitles(
                showTitles: true,
                reservedSize: 45,
                getTitlesWidget: (value, meta) => Text(
                  _formatSpeed(value),
                  style: const TextStyle(color: Color(0xFF64748B), fontSize: 9, fontFamily: 'monospace'),
                ),
              ),
            ),
            bottomTitles: AxisTitles(
              sideTitles: SideTitles(
                showTitles: true,
                getTitlesWidget: (value, meta) {
                  final ratio = value / (history.length - 1);
                  if (ratio == 0) return const Text('-5m', style: TextStyle(color: Color(0xFF64748B), fontSize: 9));
                  if ((ratio - 0.5).abs() < 0.05) return const Text('-2.5m', style: TextStyle(color: Color(0xFF64748B), fontSize: 9));
                  if (ratio >= 0.95) return const Text('Now', style: TextStyle(color: Color(0xFF64748B), fontSize: 9));
                  return const SizedBox.shrink();
                },
              ),
            ),
            rightTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
            topTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
          ),
          borderData: FlBorderData(show: false),
          minY: 0,
          maxY: maxSpeed,
          lineBarsData: [
            // Download speed curve (Emerald)
            LineChartBarData(
              spots: downSpots,
              isCurved: true,
              color: const Color(0xFF10B981),
              barWidth: 2,
              dotData: const FlDotData(show: false),
              belowBarData: BarAreaData(
                show: true,
                gradient: LinearGradient(
                  colors: [const Color(0xFF10B981).withOpacity(0.35), const Color(0xFF10B981).withOpacity(0.0)],
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                ),
              ),
            ),
            // Upload speed curve (Sky)
            LineChartBarData(
              spots: upSpots,
              isCurved: true,
              color: const Color(0xFF0EA5E9),
              barWidth: 2,
              dotData: const FlDotData(show: false),
              belowBarData: BarAreaData(
                show: true,
                gradient: LinearGradient(
                  colors: [const Color(0xFF0EA5E9).withOpacity(0.3), const Color(0xFF0EA5E9).withOpacity(0.0)],
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  static String _formatSpeed(double bytesPerSec) {
    if (bytesPerSec <= 0) return '0 B/s';
    if (bytesPerSec >= 1024 * 1024 * 1024) return '${(bytesPerSec / (1024 * 1024 * 1024)).toFixed(1)} GB/s';
    if (bytesPerSec >= 1024 * 1024) return '${(bytesPerSec / (1024 * 1024)).toStringAsFixed(1)} MB/s';
    if (bytesPerSec >= 1024) return '${(bytesPerSec / 1024).toStringAsFixed(0)} KB/s';
    return '${bytesPerSec.toStringAsFixed(0)} B/s';
  }
}
```

---

## 8. Summary of API Endpoints for Mobile Client

| Endpoint | Method | Purpose |
| :--- | :--- | :--- |
| `/api/auth/mobile/pair` | `POST` | Redeem QR pairing token for 90-day mobile session JWT |
| `/api/auth/ws-ticket` | `POST` | Issue ticket for WebSocket telemetry connection |
| `/api/torrents/stats` | `GET` | Fetch cluster aggregate throughput & 5-minute history |
| `/api/torrents` | `GET` | List all fetchers across nodes with live rates & progress |
| `/api/torrents/{compound_id}` | `GET` | Detailed fetcher specs, piece bitfield, and tracker health |
| `/api/torrents/{compound_id}/start` | `POST` | Resume or force start fetcher |
| `/api/torrents/{compound_id}/stop` | `POST` | Pause fetcher |
| `/api/torrents/bulk` | `POST` | Bulk start, stop, delete, verify, or reannounce |
| `/api/pipeline` | `GET` | Query Conduit's Scent Trail intake history (Sonarr/Radarr/Lidarr) |
| `/api/pipeline/{id}/research` | `POST` | Dispatch interactive re-search in upstream Arr client |
| `/api/nodes/turtle-mode` | `POST` | Toggle global speed throttle across all nodes |
| `/api/nodes/{name}/blocklist-update` | `POST` | Trigger immediate blocklist reload on Transmission node |

