class TorrentBandwidthContributor {
  final String id;
  final String name;
  final String node;
  final int downloadSpeed;
  final int uploadSpeed;

  TorrentBandwidthContributor({
    required this.id,
    required this.name,
    required this.node,
    required this.downloadSpeed,
    required this.uploadSpeed,
  });

  factory TorrentBandwidthContributor.fromJson(Map<String, dynamic> json) {
    return TorrentBandwidthContributor(
      id: json['id']?.toString() ?? '',
      name: json['name']?.toString() ?? 'Unknown',
      node: json['node']?.toString() ?? '',
      downloadSpeed: (json['downloadSpeed'] as num?)?.toInt() ?? (json['download_speed'] as num?)?.toInt() ?? 0,
      uploadSpeed: (json['uploadSpeed'] as num?)?.toInt() ?? (json['upload_speed'] as num?)?.toInt() ?? 0,
    );
  }
}

class BandwidthPoint {
  final DateTime timestamp;
  final int downloadSpeed;
  final int uploadSpeed;
  final List<TorrentBandwidthContributor> topTorrents;

  BandwidthPoint({
    required this.timestamp,
    required this.downloadSpeed,
    required this.uploadSpeed,
    this.topTorrents = const [],
  });

  factory BandwidthPoint.fromJson(Map<String, dynamic> json) {
    var contributors = <TorrentBandwidthContributor>[];
    if (json['topTorrents'] is List) {
      contributors = (json['topTorrents'] as List)
          .map((c) => TorrentBandwidthContributor.fromJson(c as Map<String, dynamic>))
          .toList();
    } else if (json['top_torrents'] is List) {
      contributors = (json['top_torrents'] as List)
          .map((c) => TorrentBandwidthContributor.fromJson(c as Map<String, dynamic>))
          .toList();
    }

    return BandwidthPoint(
      timestamp: json['time'] != null
          ? DateTime.fromMillisecondsSinceEpoch((json['time'] as num).toInt())
          : json['timestamp'] != null
              ? DateTime.tryParse(json['timestamp'].toString()) ?? DateTime.now()
              : DateTime.now(),
      downloadSpeed: (json['downloadSpeed'] as num?)?.toInt() ?? (json['download_speed'] as num?)?.toInt() ?? 0,
      uploadSpeed: (json['uploadSpeed'] as num?)?.toInt() ?? (json['upload_speed'] as num?)?.toInt() ?? 0,
      topTorrents: contributors,
    );
  }
}

class ClusterStats {
  final int totalDownloadRate;
  final int totalUploadRate;
  final int activeTorrentCount;
  final int totalTorrentCount;
  final int downloadingCount;
  final int seedingCount;
  final int pausedCount;
  final int errorCount;
  final bool altSpeedEnabled;
  final List<BandwidthPoint> bandwidthHistory;

  ClusterStats({
    required this.totalDownloadRate,
    required this.totalUploadRate,
    required this.activeTorrentCount,
    required this.totalTorrentCount,
    required this.downloadingCount,
    required this.seedingCount,
    required this.pausedCount,
    required this.errorCount,
    required this.altSpeedEnabled,
    required this.bandwidthHistory,
  });

  factory ClusterStats.empty() {
    return ClusterStats(
      totalDownloadRate: 0,
      totalUploadRate: 0,
      activeTorrentCount: 0,
      totalTorrentCount: 0,
      downloadingCount: 0,
      seedingCount: 0,
      pausedCount: 0,
      errorCount: 0,
      altSpeedEnabled: false,
      bandwidthHistory: [],
    );
  }

  factory ClusterStats.fromJson(Map<String, dynamic> json) {
    var historyList = <BandwidthPoint>[];
    if (json['bandwidth_history'] is List) {
      historyList = (json['bandwidth_history'] as List)
          .map((item) => BandwidthPoint.fromJson(item as Map<String, dynamic>))
          .toList();
    }

    return ClusterStats(
      totalDownloadRate: (json['total_download_rate'] as num?)?.toInt() ?? (json['download_speed'] as num?)?.toInt() ?? (json['total_download_speed'] as num?)?.toInt() ?? 0,
      totalUploadRate: (json['total_upload_rate'] as num?)?.toInt() ?? (json['upload_speed'] as num?)?.toInt() ?? (json['total_upload_speed'] as num?)?.toInt() ?? 0,
      activeTorrentCount: (json['active_torrent_count'] as num?)?.toInt() ?? 0,
      totalTorrentCount: (json['total_torrent_count'] as num?)?.toInt() ?? 0,
      downloadingCount: (json['downloading_count'] as num?)?.toInt() ?? 0,
      seedingCount: (json['seeding_count'] as num?)?.toInt() ?? 0,
      pausedCount: (json['paused_count'] as num?)?.toInt() ?? 0,
      errorCount: (json['error_count'] as num?)?.toInt() ?? 0,
      altSpeedEnabled: json['alt_speed_enabled'] == true,
      bandwidthHistory: historyList,
    );
  }
}
