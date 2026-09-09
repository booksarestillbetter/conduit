import 'dart:async';
import 'package:flutter/material.dart';
import '../core/network/websocket_client.dart';
import '../core/network/api_client.dart';
import '../core/models/cluster_stats.dart';

class TelemetryProvider extends ChangeNotifier {
  final ConduitWebSocketClient _wsClient = ConduitWebSocketClient();
  StreamSubscription? _subscription;

  ClusterStats _stats = ClusterStats.empty();
  bool _isConnected = false;
  final List<BandwidthPoint> _rollingHistory = [];

  ClusterStats get stats => _stats;
  bool get isConnected => _isConnected;
  List<BandwidthPoint> get rollingHistory => List.unmodifiable(_rollingHistory);

  void startListening() {
    _subscription?.cancel();
    _wsClient.connect();

    _subscription = _wsClient.stream.listen((event) {
      final type = event['type']?.toString();

      if (type == 'connection_status') {
        _isConnected = event['connected'] == true;
        notifyListeners();
      } else if (type == 'telemetry' || type == 'metrics' || type == 'stats') {
        _isConnected = true;
        final rawStats = event['data'] ?? event;
        if (rawStats is Map<String, dynamic>) {
          _stats = ClusterStats.fromJson(rawStats);

          // Add point to rolling history (cap at 300 points = 5 minutes)
          _rollingHistory.add(BandwidthPoint(
            timestamp: DateTime.now(),
            downloadSpeed: _stats.totalDownloadRate,
            uploadSpeed: _stats.totalUploadRate,
          ));

          if (_rollingHistory.length > 300) {
            _rollingHistory.removeRange(0, _rollingHistory.length - 300);
          }

          notifyListeners();
        }
      }
    });
  }

  Future<void> toggleTurtleMode() async {
    final next = !_stats.altSpeedEnabled;
    final ok = await ApiClient.toggleTurtleMode(next);
    if (ok) {
      _stats = ClusterStats(
        totalDownloadRate: _stats.totalDownloadRate,
        totalUploadRate: _stats.totalUploadRate,
        activeTorrentCount: _stats.activeTorrentCount,
        totalTorrentCount: _stats.totalTorrentCount,
        downloadingCount: _stats.downloadingCount,
        seedingCount: _stats.seedingCount,
        pausedCount: _stats.pausedCount,
        errorCount: _stats.errorCount,
        altSpeedEnabled: next,
        bandwidthHistory: _stats.bandwidthHistory,
      );
      notifyListeners();
    }
  }

  void stopListening() {
    _subscription?.cancel();
    _subscription = null;
    _wsClient.disconnect();
    _isConnected = false;
    notifyListeners();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _wsClient.dispose();
    super.dispose();
  }
}
