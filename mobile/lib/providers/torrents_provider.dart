import 'dart:async';
import 'package:flutter/material.dart';
import '../core/models/torrent.dart';
import '../core/network/api_client.dart';

class TorrentsProvider extends ChangeNotifier {
  List<Torrent> _torrents = [];
  bool _isLoading = false;
  String _selectedNode = 'all';
  String _selectedStatus = 'all'; // all, downloading, seeding, paused, circuit_broken, error
  String _searchQuery = '';
  Timer? _pollTimer;

  List<Torrent> get rawTorrents => _torrents;
  bool get isLoading => _isLoading;
  String get selectedNode => _selectedNode;
  String get selectedStatus => _selectedStatus;
  String get searchQuery => _searchQuery;

  List<Torrent> get filteredTorrents {
    return _torrents.where((t) {
      // Node filter
      if (_selectedNode != 'all' && t.node != _selectedNode) {
        return false;
      }

      // Status filter
      if (_selectedStatus == 'downloading' && !t.isDownloading) return false;
      if (_selectedStatus == 'seeding' && !t.isSeeding) return false;
      if (_selectedStatus == 'paused' && !t.isPaused) return false;
      if (_selectedStatus == 'circuit_broken' && !t.isCircuitBroken) return false;
      if (_selectedStatus == 'error' && !t.hasError) return false;

      // Search query
      if (_searchQuery.isNotEmpty) {
        final query = _searchQuery.toLowerCase();
        final nameMatches = t.name.toLowerCase().contains(query);
        final hashMatches = t.hashString?.toLowerCase().contains(query) ?? false;
        final dirMatches = t.downloadDir?.toLowerCase().contains(query) ?? false;
        return nameMatches || hashMatches || dirMatches;
      }

      return true;
    }).toList();
  }

  void setSelectedNode(String node) {
    _selectedNode = node;
    notifyListeners();
  }

  void setSelectedStatus(String status) {
    _selectedStatus = status;
    notifyListeners();
  }

  void setSearchQuery(String query) {
    _searchQuery = query;
    notifyListeners();
  }

  void startPolling() {
    _pollTimer?.cancel();
    fetchTorrents();
    _pollTimer = Timer.periodic(const Duration(seconds: 3), (_) {
      fetchTorrents(silent: true);
    });
  }

  void stopPolling() {
    _pollTimer?.cancel();
    _pollTimer = null;
  }

  Future<void> fetchTorrents({bool silent = false}) async {
    if (!silent) {
      _isLoading = true;
      notifyListeners();
    }

    try {
      final list = await ApiClient.getTorrents(node: _selectedNode);
      _torrents = list;
    } catch (e) {
      // Keep existing list on transient network error
    } finally {
      _isLoading = false;
      notifyListeners();
    }
  }

  Future<bool> startTorrent(String compoundId) async {
    final ok = await ApiClient.startTorrent(compoundId);
    if (ok) fetchTorrents(silent: true);
    return ok;
  }

  Future<bool> stopTorrent(String compoundId) async {
    final ok = await ApiClient.stopTorrent(compoundId);
    if (ok) fetchTorrents(silent: true);
    return ok;
  }

  Future<bool> deleteTorrent(String compoundId, {bool deleteFiles = false}) async {
    final ok = await ApiClient.deleteTorrent(compoundId, deleteFiles: deleteFiles);
    if (ok) fetchTorrents(silent: true);
    return ok;
  }

  Future<bool> moveQueue(String compoundId, String direction) async {
    final ok = await ApiClient.moveQueue(compoundId, direction);
    if (ok) fetchTorrents(silent: true);
    return ok;
  }

  Future<bool> toggleSequential(String compoundId, bool currentVal) async {
    final ok = await ApiClient.setSequential(compoundId, !currentVal);
    if (ok) fetchTorrents(silent: true);
    return ok;
  }

  Future<bool> reannounce(String compoundId) async {
    final ok = await ApiClient.reannounce(compoundId);
    if (ok) fetchTorrents(silent: true);
    return ok;
  }

  @override
  void dispose() {
    _pollTimer?.cancel();
    super.dispose();
  }
}
