class MediaGrab {
  final String id;
  final String title;
  final String releaseTitle;
  final String sceneName;
  final String itemType; // series, movie, music, tv
  final String status; // requested, grabbed, staging, imported, replaced, deleted
  final String? eventType;
  final int? seriesId;
  final int? episodeId;
  final int? movieId;
  final int? artistId;
  final int? albumId;
  final int? seasonNumber;
  final String? episodeNumbers;
  final String? posterUrl;
  final String? overview;
  final List<String> genres;
  final String? quality;
  final String? indexer;
  final int? sizeBytes;
  final String? downloadClient;
  final String? downloadId;
  final int? year;
  final double? rating;
  final int? runtimeMins;
  final String? imdbId;
  final int? tmdbId;
  final int? tvdbId;
  final bool reSearched;
  final int reSearchCount;
  final DateTime createdAt;
  final DateTime updatedAt;

  MediaGrab({
    required this.id,
    required this.title,
    required this.releaseTitle,
    this.sceneName = '',
    required this.itemType,
    required this.status,
    this.eventType,
    this.seriesId,
    this.episodeId,
    this.movieId,
    this.artistId,
    this.albumId,
    this.seasonNumber,
    this.episodeNumbers,
    this.posterUrl,
    this.overview,
    this.genres = const [],
    this.quality,
    this.indexer,
    this.sizeBytes,
    this.downloadClient,
    this.downloadId,
    this.year,
    this.rating,
    this.runtimeMins,
    this.imdbId,
    this.tmdbId,
    this.tvdbId,
    this.reSearched = false,
    this.reSearchCount = 0,
    required this.createdAt,
    required this.updatedAt,
  });

  factory MediaGrab.fromJson(Map<String, dynamic> json) {
    List<String> parsedGenres = [];
    if (json['genres'] != null) {
      if (json['genres'] is List) {
        parsedGenres = (json['genres'] as List).map((e) => e.toString()).toList();
      } else if (json['genres'] is String) {
        try {
          final gStr = json['genres'] as String;
          if (gStr.startsWith('[') && gStr.endsWith(']')) {
            parsedGenres = gStr
                .substring(1, gStr.length - 1)
                .split(',')
                .map((e) => e.replaceAll('"', '').trim())
                .where((e) => e.isNotEmpty)
                .toList();
          } else {
            parsedGenres = gStr.split(',').map((e) => e.trim()).where((e) => e.isNotEmpty).toList();
          }
        } catch (_) {}
      }
    }

    final rawTitle = json['title']?.toString();
    final rawRelease = json['release_title']?.toString() ?? json['scene_name']?.toString() ?? 'Unknown Release';
    final displayTitle = (rawTitle != null && rawTitle.isNotEmpty) ? rawTitle : rawRelease;

    return MediaGrab(
      id: json['id']?.toString() ?? '',
      title: displayTitle,
      releaseTitle: rawRelease,
      sceneName: json['scene_name']?.toString() ?? '',
      itemType: json['item_type']?.toString() ?? json['media_type']?.toString() ?? 'series',
      status: json['status']?.toString() ?? 'grabbed',
      eventType: json['event_type']?.toString(),
      seriesId: (json['series_id'] as num?)?.toInt(),
      episodeId: (json['episode_id'] as num?)?.toInt(),
      movieId: (json['movie_id'] as num?)?.toInt(),
      artistId: (json['artist_id'] as num?)?.toInt(),
      albumId: (json['album_id'] as num?)?.toInt(),
      seasonNumber: (json['season_number'] as num?)?.toInt(),
      episodeNumbers: json['episode_numbers']?.toString(),
      posterUrl: json['poster_url']?.toString(),
      overview: json['overview']?.toString(),
      genres: parsedGenres,
      quality: json['quality']?.toString(),
      indexer: json['indexer']?.toString(),
      sizeBytes: (json['size_bytes'] as num?)?.toInt(),
      downloadClient: json['download_client']?.toString(),
      downloadId: json['download_id']?.toString() ?? json['info_hash']?.toString(),
      year: (json['year'] as num?)?.toInt(),
      rating: (json['rating'] as num?)?.toDouble(),
      runtimeMins: (json['runtime_mins'] as num?)?.toInt(),
      imdbId: json['imdb_id']?.toString(),
      tmdbId: (json['tmdb_id'] as num?)?.toInt(),
      tvdbId: (json['tvdb_id'] as num?)?.toInt(),
      reSearched: json['re_searched'] == true,
      reSearchCount: (json['re_search_count'] as num?)?.toInt() ?? 0,
      createdAt: json['created_at'] != null
          ? (DateTime.tryParse(json['created_at'].toString()) ?? DateTime.now())
          : (json['grabbed_at'] != null ? DateTime.tryParse(json['grabbed_at'].toString()) ?? DateTime.now() : DateTime.now()),
      updatedAt: json['updated_at'] != null
          ? (DateTime.tryParse(json['updated_at'].toString()) ?? DateTime.now())
          : DateTime.now(),
    );
  }

  bool get isMovie => itemType == 'movie';
  bool get isMusic => itemType == 'music' || itemType == 'artist';
  bool get isSeries => itemType == 'series' || itemType == 'tv';
  bool get isImported => status == 'imported';
  bool get isDeleted => status == 'deleted';
  bool get isReplaced => status == 'replaced' || reSearched;
}
