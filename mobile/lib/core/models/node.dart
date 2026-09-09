typedef FetcherNode = TransmissionNode;

class TransmissionNode {
  final String name;
  final String host;
  final int port;
  final String clientType;
  final bool online;
  final int freeSpaceBytes;
  final double freeSpaceGb;
  final int torrentCount;
  final int activeTorrents;
  final int pausedTorrents;
  final int errorTorrents;
  final int downloadRate;
  final int uploadRate;
  final int rpcLatencyMs;
  final bool altSpeedEnabled;
  final int blocklistSize;
  final bool blocklistEnabled;

  TransmissionNode({
    required this.name,
    required this.host,
    required this.port,
    this.clientType = 'transmission',
    required this.online,
    required this.freeSpaceBytes,
    this.freeSpaceGb = 0.0,
    required this.torrentCount,
    this.activeTorrents = 0,
    this.pausedTorrents = 0,
    this.errorTorrents = 0,
    required this.downloadRate,
    required this.uploadRate,
    required this.rpcLatencyMs,
    this.altSpeedEnabled = false,
    this.blocklistSize = 0,
    this.blocklistEnabled = false,
  });

  String get clientDisplayName {
    switch (clientType.toLowerCase()) {
      case 'qbittorrent':
      case 'qtorrent':
      case 'qbit':
        return 'qBittorrent';
      case 'deluge':
        return 'Deluge';
      case 'synapse':
        return 'Synapse';
      case 'transmission':
      default:
        return 'Transmission';
    }
  }

  factory TransmissionNode.fromJson(Map<String, dynamic> json) {
    final name = json['node']?.toString() ?? json['name']?.toString() ?? 'default';
    final isOnline = json['connected'] == true || json['online'] == true;
    final freeSpace = (json['free_space_bytes'] as num?)?.toInt() ?? 0;
    final freeGb = (json['free_space_gb'] as num?)?.toDouble() ?? (freeSpace / (1024 * 1024 * 1024));
    final clientType = json['client_type']?.toString() ?? 'transmission';

    return TransmissionNode(
      name: name,
      host: json['host']?.toString() ?? name,
      port: (json['port'] as num?)?.toInt() ?? 9091,
      clientType: clientType,
      online: isOnline,
      freeSpaceBytes: freeSpace,
      freeSpaceGb: freeGb,
      torrentCount: (json['total_torrents'] as num?)?.toInt() ?? (json['torrent_count'] as num?)?.toInt() ?? 0,
      activeTorrents: (json['active_torrents'] as num?)?.toInt() ?? 0,
      pausedTorrents: (json['paused_torrents'] as num?)?.toInt() ?? 0,
      errorTorrents: (json['error_torrents'] as num?)?.toInt() ?? 0,
      downloadRate: (json['rate_download'] as num?)?.toInt() ?? (json['download_rate'] as num?)?.toInt() ?? 0,
      uploadRate: (json['rate_upload'] as num?)?.toInt() ?? (json['upload_rate'] as num?)?.toInt() ?? 0,
      rpcLatencyMs: (json['latency_ms'] as num?)?.toInt() ?? (json['rpc_latency_ms'] as num?)?.toInt() ?? 0,
      altSpeedEnabled: json['alt_speed_enabled'] == true,
      blocklistSize: (json['blocklist_size'] as num?)?.toInt() ?? 0,
      blocklistEnabled: json['blocklist_enabled'] == true,
    );
  }
}
