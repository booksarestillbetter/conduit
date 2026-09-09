import 'dart:async';
import 'package:flutter/material.dart';
import '../core/models/node.dart';
import '../core/models/circuit_breaker.dart';
import '../core/models/media_grab.dart';
import '../core/models/zone.dart';
import '../core/network/api_client.dart';

class NodesProvider extends ChangeNotifier {
  List<TransmissionNode> _nodes = [];
  List<CircuitBreakerStatus> _circuitBreakers = [];
  List<MediaGrab> _mediaGrabs = [];
  List<MediaGrab> _ghostGrabs = [];
  List<ZoneConfig> _zones = [];
  String? _activeZone;
  bool _isLoading = false;
  Timer? _refreshTimer;

  List<TransmissionNode> get nodes => _nodes;
  List<CircuitBreakerStatus> get circuitBreakers => _circuitBreakers;
  List<MediaGrab> get mediaGrabs => _mediaGrabs;
  List<MediaGrab> get ghostGrabs => _ghostGrabs;
  List<ZoneConfig> get zones => _zones;
  String? get activeZone => _activeZone;
  bool get isLoading => _isLoading;

  void setActiveZone(String? zoneId) {
    _activeZone = zoneId;
    notifyListeners();
    fetchAll();
  }

  void startRefreshing() {
    _refreshTimer?.cancel();
    fetchAll();
    _refreshTimer = Timer.periodic(const Duration(seconds: 4), (_) {
      fetchAll(silent: true);
    });
  }

  void stopRefreshing() {
    _refreshTimer?.cancel();
    _refreshTimer = null;
  }

  Future<void> fetchAll({bool silent = false}) async {
    if (!silent && _nodes.isEmpty && _mediaGrabs.isEmpty) {
      _isLoading = true;
      notifyListeners();
    }

    // Fetch each independently so a failure or empty state in one doesn't wipe the others
    try {
      final fetchedNodes = await ApiClient.getNodes();
      _nodes = fetchedNodes;
    } catch (e) {
      debugPrint('[NodesProvider] Error fetching nodes: $e');
    }

    try {
      final fetchedBreakers = await ApiClient.getCircuitBreakers();
      _circuitBreakers = fetchedBreakers;
    } catch (e) {
      debugPrint('[NodesProvider] Error fetching circuit breakers: $e');
    }

    try {
      final fetchedGrabs = await ApiClient.getMediaGrabs(zone: _activeZone);
      _mediaGrabs = fetchedGrabs;
    } catch (e) {
      debugPrint('[NodesProvider] Error fetching mediaGrabs: $e');
    }

    try {
      final fetchedGhost = await ApiClient.getGhostArchive(zone: _activeZone, limit: 100);
      _ghostGrabs = fetchedGhost;
    } catch (e) {
      debugPrint('[NodesProvider] Error fetching ghostGrabs: $e');
    }

    try {
      final settings = await ApiClient.getSettings();
      if (settings.containsKey('zones') && settings['zones'] is List) {
        _zones = (settings['zones'] as List)
            .map((e) => ZoneConfig.fromJson(e as Map<String, dynamic>))
            .toList();
      }
    } catch (e) {
      debugPrint('[NodesProvider] Error fetching settings: $e');
    }

    _isLoading = false;
    notifyListeners();
  }

  Future<bool> triggerResearch(String grabId) async {
    final ok = await ApiClient.triggerResearch(grabId);
    if (ok) fetchAll(silent: true);
    return ok;
  }

  Future<bool> deletePipelineItem(String grabId) async {
    final ok = await ApiClient.deletePipelineItem(grabId);
    if (ok) fetchAll(silent: true);
    return ok;
  }
}
