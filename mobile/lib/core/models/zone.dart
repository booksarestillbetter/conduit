class ZoneConfig {
  final String id;
  final String name;
  final List<String> fetcherNodeNames;
  final List<String> plexNodeNames;

  ZoneConfig({
    required this.id,
    required this.name,
    this.fetcherNodeNames = const [],
    this.plexNodeNames = const [],
  });

  factory ZoneConfig.fromJson(Map<String, dynamic> json) {
    return ZoneConfig(
      id: json['id']?.toString() ?? '',
      name: json['name']?.toString() ?? 'Zone',
      fetcherNodeNames: ((json['fetcher_node_names'] ?? json['transmission_node_names']) as List<dynamic>?)
          ?.map((e) => e.toString())
          .toList() ?? [],
      plexNodeNames: (json['plex_node_names'] as List<dynamic>?)
          ?.map((e) => e.toString())
          .toList() ?? [],
    );
  }
}
