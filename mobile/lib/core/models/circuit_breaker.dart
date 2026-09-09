class CircuitBreakerStatus {
  final String trackerHost;
  final bool isBroken;
  final int failureCount;
  final int totalSwarms;
  final double failureRatio;
  final String? reason;
  final DateTime? lastTrippedAt;
  final String? canaryTorrentId;

  CircuitBreakerStatus({
    required this.trackerHost,
    required this.isBroken,
    required this.failureCount,
    required this.totalSwarms,
    required this.failureRatio,
    this.reason,
    this.lastTrippedAt,
    this.canaryTorrentId,
  });

  factory CircuitBreakerStatus.fromJson(Map<String, dynamic> json) {
    return CircuitBreakerStatus(
      trackerHost: json['host']?.toString() ?? json['tracker_host']?.toString() ?? 'unknown',
      isBroken: json['is_circuit_broken'] == true || json['is_broken'] == true,
      failureCount: (json['failure_count'] as num?)?.toInt() ?? 0,
      totalSwarms: (json['total_swarms'] as num?)?.toInt() ?? (json['swarms_affected'] as num?)?.toInt() ?? 0,
      failureRatio: (json['failure_ratio'] as num?)?.toDouble() ?? 0.0,
      reason: json['reason']?.toString() ?? json['error']?.toString(),
      lastTrippedAt: json['last_tripped_at'] != null
          ? DateTime.tryParse(json['last_tripped_at'].toString())
          : null,
      canaryTorrentId: json['canary_torrent_id']?.toString(),
    );
  }
}
