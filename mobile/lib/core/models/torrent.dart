import 'media_grab.dart';
import 'peer.dart';

class TorrentFile {
  final String name;
  final int bytesCompleted;
  final int length;
  final int priority; // -1: low/off, 0: normal, 1: high
  final bool wanted;

  TorrentFile({
    required this.name,
    required this.bytesCompleted,
    required this.length,
    this.priority = 0,
    this.wanted = true,
  });

  factory TorrentFile.fromJson(Map<String, dynamic> json) {
    return TorrentFile(
      name: json['name']?.toString() ?? 'File',
      bytesCompleted: (json['bytesCompleted'] as num?)?.toInt() ?? (json['bytes_completed'] as num?)?.toInt() ?? 0,
      length: (json['length'] as num?)?.toInt() ?? (json['size'] as num?)?.toInt() ?? 0,
      priority: (json['priority'] as num?)?.toInt() ?? 0,
      wanted: json['wanted'] != false,
    );
  }

  double get progress => length > 0 ? (bytesCompleted / length).clamp(0.0, 1.0) : 0.0;
}

class TrackerStat {
  final int id;
  final String host;
  final String announce;
  final int tier;
  final String lastAnnounceResult;
  final bool lastAnnounceSucceeded;
  final String lastScrapeResult;
  final bool lastScrapeSucceeded;
  final int seederCount;
  final int leecherCount;
  final int downloadCount;

  TrackerStat({
    required this.id,
    required this.host,
    required this.announce,
    this.tier = 0,
    this.lastAnnounceResult = '',
    this.lastAnnounceSucceeded = true,
    this.lastScrapeResult = '',
    this.lastScrapeSucceeded = true,
    this.seederCount = 0,
    this.leecherCount = 0,
    this.downloadCount = 0,
  });

  factory TrackerStat.fromJson(Map<String, dynamic> json) {
    return TrackerStat(
      id: (json['id'] as num?)?.toInt() ?? 0,
      host: json['host']?.toString() ?? 'Tracker',
      announce: json['announce']?.toString() ?? '',
      tier: (json['tier'] as num?)?.toInt() ?? 0,
      lastAnnounceResult: json['last_announce_result']?.toString() ?? json['lastAnnounceResult']?.toString() ?? '',
      lastAnnounceSucceeded: json['last_announce_succeeded'] == true || json['lastAnnounceSucceeded'] == true,
      lastScrapeResult: json['last_scrape_result']?.toString() ?? json['lastScrapeResult']?.toString() ?? '',
      lastScrapeSucceeded: json['last_scrape_succeeded'] == true || json['lastScrapeSucceeded'] == true,
      seederCount: (json['seeder_count'] as num?)?.toInt() ?? (json['seederCount'] as num?)?.toInt() ?? 0,
      leecherCount: (json['leecher_count'] as num?)?.toInt() ?? (json['leecherCount'] as num?)?.toInt() ?? 0,
      downloadCount: (json['download_count'] as num?)?.toInt() ?? (json['downloadCount'] as num?)?.toInt() ?? 0,
    );
  }
}

class TorrentTimelineEvent {
  final String stage;
  final String title;
  final String description;
  final int timestamp;
  final String formattedTime;
  final String status;

  TorrentTimelineEvent({
    required this.stage,
    required this.title,
    required this.description,
    required this.timestamp,
    required this.formattedTime,
    required this.status,
  });

  factory TorrentTimelineEvent.fromJson(Map<String, dynamic> json) {
    return TorrentTimelineEvent(
      stage: json['stage']?.toString() ?? 'info',
      title: json['title']?.toString() ?? '',
      description: json['description']?.toString() ?? '',
      timestamp: (json['timestamp'] as num?)?.toInt() ?? 0,
      formattedTime: json['formatted_time']?.toString() ?? '',
      status: json['status']?.toString() ?? 'success',
    );
  }
}

class Torrent {
  final String id;
  final String compoundId;
  final String node;
  final String name;
  final String status;
  final int totalSize;
  final int downloadedBytes;
  final int uploadedBytes;
  final double progress;
  final int rateDownload;
  final int rateUpload;
  final int eta;
  final double uploadRatio;
  final int peersSendingToUs;
  final int peersGettingFromUs;
  final int peersConnected;
  final String? downloadDir;
  final String? hashString;
  final String? errorString;
  final bool isSequential;
  final int queuePosition;
  final bool isCircuitBroken;
  final String? circuitBreakerReason;
  final bool isCanaryProbe;
  final String? pieces;
  final int pieceCount;
  final int pieceSize;
  final List<int>? availability;
  final int secondsDownloading;
  final int secondsSeeding;
  final List<Peer> peers;
  final List<TorrentFile> files;
  final List<TrackerStat> trackerStats;
  final List<TorrentTimelineEvent> timeline;
  final MediaGrab? arrGrab;

  Torrent({
    required this.id,
    required this.compoundId,
    required this.node,
    required this.name,
    required this.status,
    required this.totalSize,
    required this.downloadedBytes,
    required this.uploadedBytes,
    required this.progress,
    required this.rateDownload,
    required this.rateUpload,
    required this.eta,
    required this.uploadRatio,
    required this.peersSendingToUs,
    required this.peersGettingFromUs,
    required this.peersConnected,
    this.downloadDir,
    this.hashString,
    this.errorString,
    this.isSequential = false,
    this.queuePosition = 0,
    this.isCircuitBroken = false,
    this.circuitBreakerReason,
    this.isCanaryProbe = false,
    this.pieces,
    this.pieceCount = 0,
    this.pieceSize = 0,
    this.availability,
    this.secondsDownloading = 0,
    this.secondsSeeding = 0,
    this.peers = const [],
    this.files = const [],
    this.trackerStats = const [],
    this.timeline = const [],
    this.arrGrab,
  });

  factory Torrent.fromJson(Map<String, dynamic> rawJson) {
    // If backend returns { "unified": { ... }, "peers": [...], "timeline": [...] }
    final isWrapped = rawJson.containsKey('unified') && rawJson['unified'] is Map<String, dynamic>;
    final json = isWrapped ? (rawJson['unified'] as Map<String, dynamic>) : rawJson;

    final peerList = (rawJson['peers'] as List<dynamic>?)
        ?.map((e) => Peer.fromJson(e as Map<String, dynamic>))
        .toList() ?? [];

    final fileList = (rawJson['files'] as List<dynamic>? ?? json['files'] as List<dynamic>?)
        ?.map((e) => TorrentFile.fromJson(e as Map<String, dynamic>))
        .toList() ?? [];

    final trackerList = (rawJson['tracker_stats'] as List<dynamic>? ?? json['tracker_stats'] as List<dynamic>?)
        ?.map((e) => TrackerStat.fromJson(e as Map<String, dynamic>))
        .toList() ?? [];

    final timelineList = (rawJson['timeline'] as List<dynamic>?)
        ?.map((e) => TorrentTimelineEvent.fromJson(e as Map<String, dynamic>))
        .toList() ?? [];

    MediaGrab? grab;
    if (rawJson['arr_grab'] is Map<String, dynamic>) {
      grab = MediaGrab.fromJson(rawJson['arr_grab'] as Map<String, dynamic>);
    } else if (json['arr_grab'] is Map<String, dynamic>) {
      grab = MediaGrab.fromJson(json['arr_grab'] as Map<String, dynamic>);
    }

    return Torrent(
      id: json['id']?.toString() ?? '',
      compoundId: json['compound_id']?.toString() ?? '${json['node']}:${json['id']}',
      node: json['node']?.toString() ?? 'default',
      name: json['name']?.toString() ?? 'Unknown Torrent',
      status: json['status']?.toString() ?? 'unknown',
      totalSize: (json['total_size'] as num?)?.toInt() ?? 0,
      downloadedBytes: (json['downloaded_bytes'] as num?)?.toInt() ?? (json['downloaded_ever'] as num?)?.toInt() ?? 0,
      uploadedBytes: (json['uploaded_bytes'] as num?)?.toInt() ?? (json['uploaded_ever'] as num?)?.toInt() ?? 0,
      progress: (json['progress'] as num?)?.toDouble() ?? (json['percent_done'] as num?)?.toDouble() ?? 0.0,
      rateDownload: (json['rate_download'] as num?)?.toInt() ?? 0,
      rateUpload: (json['rate_upload'] as num?)?.toInt() ?? 0,
      eta: (json['eta'] as num?)?.toInt() ?? -1,
      uploadRatio: (json['upload_ratio'] as num?)?.toDouble() ?? 0.0,
      peersSendingToUs: (json['peers_sending_to_us'] as num?)?.toInt() ?? 0,
      peersGettingFromUs: (json['peers_getting_from_us'] as num?)?.toInt() ?? 0,
      peersConnected: (json['peers_connected'] as num?)?.toInt() ?? 0,
      downloadDir: json['download_dir']?.toString(),
      hashString: json['hash_string']?.toString() ?? json['hash']?.toString(),
      errorString: json['error_string']?.toString(),
      isSequential: json['is_sequential'] == true || json['sequential_download'] == true,
      queuePosition: (json['queue_position'] as num?)?.toInt() ?? 0,
      isCircuitBroken: json['is_circuit_broken'] == true,
      circuitBreakerReason: json['circuit_breaker_reason']?.toString(),
      isCanaryProbe: json['is_canary_probe'] == true,
      pieces: (rawJson['pieces'] ?? json['pieces'])?.toString(),
      pieceCount: (rawJson['piece_count'] as num?)?.toInt() ?? (json['piece_count'] as num?)?.toInt() ?? 0,
      pieceSize: (rawJson['piece_size'] as num?)?.toInt() ?? (json['piece_size'] as num?)?.toInt() ?? 0,
      availability: ((rawJson['availability'] ?? json['availability']) as List<dynamic>?)
          ?.map((e) => (e as num).toInt())
          .toList(),
      secondsDownloading: (rawJson['seconds_downloading'] as num?)?.toInt() ?? (json['seconds_downloading'] as num?)?.toInt() ?? 0,
      secondsSeeding: (rawJson['seconds_seeding'] as num?)?.toInt() ?? (json['seconds_seeding'] as num?)?.toInt() ?? 0,
      peers: peerList,
      files: fileList,
      trackerStats: trackerList,
      timeline: timelineList,
      arrGrab: grab,
    );
  }

  bool get isDownloading => status == 'downloading' || status == 'download_wait';
  bool get isSeeding => status == 'seeding' || status == 'seed_wait';
  bool get isPaused => status == 'stopped' || status == 'paused';
  bool get hasError => (errorString != null && errorString!.isNotEmpty) || status == 'error';
  bool get isComplete => progress >= 1.0 || downloadedBytes >= totalSize;

  String get displayTitle => (arrGrab != null && arrGrab!.title.isNotEmpty) ? arrGrab!.title : name;
  String? get posterUrl => arrGrab?.posterUrl;
  String? get quality => arrGrab?.quality;
  String? get indexer => arrGrab?.indexer;
  String? get mediaType => arrGrab?.itemType;
  bool get hasArrMetadata => arrGrab != null;
}
