import 'package:flutter/material.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/media_grab.dart';
import '../../core/network/api_client.dart';
import '../../core/utils/formatters.dart';

class MediaDetailBottomSheet extends StatefulWidget {
  final MediaGrab item;
  final VoidCallback? onRefresh;

  const MediaDetailBottomSheet({
    super.key,
    required this.item,
    this.onRefresh,
  });

  @override
  State<MediaDetailBottomSheet> createState() => _MediaDetailBottomSheetState();
}

class _MediaDetailBottomSheetState extends State<MediaDetailBottomSheet> with SingleTickerProviderStateMixin {
  late TabController _tabController;
  List<ReleaseCandidate> _releases = [];
  bool _isLoadingReleases = false;
  String? _releasesError;
  String? _grabbingGuid;
  String? _actionMessage;
  bool _isReSearching = false;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 2, vsync: this);
  }

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  Future<void> _fetchReleases() async {
    setState(() {
      _isLoadingReleases = true;
      _releasesError = null;
    });

    try {
      final results = await ApiClient.searchReleases(
        itemType: widget.item.itemType,
        movieId: widget.item.movieId,
        seriesId: widget.item.seriesId,
        episodeId: widget.item.episodeId,
        albumId: widget.item.albumId,
      );
      if (mounted) {
        setState(() {
          _releases = results;
          _isLoadingReleases = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _releasesError = e.toString().replaceAll('Exception: ', '');
          _isLoadingReleases = false;
        });
      }
    }
  }

  Future<void> _handleGrab(ReleaseCandidate rel) async {
    setState(() => _grabbingGuid = rel.guid);
    try {
      final success = await ApiClient.grabRelease(
        itemType: widget.item.itemType,
        guid: rel.guid,
        indexerId: rel.indexerId,
      );
      if (mounted) {
        setState(() {
          _actionMessage = success ? '⚡ Grabbed release: ${rel.title}' : 'Failed to grab release';
          _grabbingGuid = null;
        });
        widget.onRefresh?.call();
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _actionMessage = 'Grab error: $e';
          _grabbingGuid = null;
        });
      }
    }
  }

  Future<void> _handleReSearch() async {
    setState(() => _isReSearching = true);
    try {
      final ok = await ApiClient.triggerResearch(widget.item.id);
      if (mounted) {
        setState(() {
          _actionMessage = ok ? '🔍 Re-search triggered in Arr stack' : 'Failed to trigger re-search';
          _isReSearching = false;
        });
        widget.onRefresh?.call();
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _actionMessage = 'Re-search error: $e';
          _isReSearching = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      initialChildSize: 0.85,
      minChildSize: 0.5,
      maxChildSize: 0.95,
      builder: (context, scrollController) {
        return Container(
          decoration: const BoxDecoration(
            color: ConduitColors.surface,
            borderRadius: BorderRadius.vertical(top: Radius.circular(24)),
          ),
          child: Column(
            children: [
              // Drag Handle
              Center(
                child: Container(
                  margin: const EdgeInsets.symmetric(vertical: 10),
                  height: 4,
                  width: 40,
                  decoration: BoxDecoration(
                    color: ConduitColors.border,
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
              ),

              // Media Header
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (widget.item.posterUrl != null && widget.item.posterUrl!.isNotEmpty)
                      ClipRRect(
                        borderRadius: BorderRadius.circular(10),
                        child: Image.network(
                          widget.item.posterUrl!,
                          width: 60,
                          height: 90,
                          fit: BoxFit.cover,
                          errorBuilder: (ctx, err, stack) => _fallbackIcon(),
                        ),
                      )
                    else
                      _fallbackIcon(),
                    const SizedBox(width: 14),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            widget.item.title,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 16,
                              fontWeight: FontWeight.bold,
                              color: ConduitColors.textPrimary,
                            ),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            widget.item.releaseTitle,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 11,
                              color: ConduitColors.textMuted,
                              fontFamily: 'monospace',
                            ),
                          ),
                          const SizedBox(height: 8),
                          Wrap(
                            spacing: 6,
                            runSpacing: 4,
                            children: [
                              _buildPill(widget.item.status.toUpperCase(), ConduitColors.brandLight),
                              if (widget.item.quality != null)
                                _buildPill(widget.item.quality!, ConduitColors.purpleLight),
                              if (widget.item.indexer != null)
                                _buildPill(widget.item.indexer!, Colors.amber),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),

              if (_actionMessage != null)
                Container(
                  margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color: ConduitColors.card,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: ConduitColors.brandLight.withValues(alpha: 0.3)),
                  ),
                  child: Row(
                    children: [
                      const Icon(Icons.info_outline, size: 16, color: ConduitColors.brandLight),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          _actionMessage!,
                          style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 12),
                        ),
                      ),
                    ],
                  ),
                ),

              // Tab bar
              TabBar(
                controller: _tabController,
                indicatorColor: ConduitColors.brandLight,
                labelColor: ConduitColors.brandLight,
                unselectedLabelColor: ConduitColors.textMuted,
                tabs: const [
                  Tab(text: 'Details & Timeline'),
                  Tab(text: 'Interactive Search'),
                ],
              ),

              Expanded(
                child: TabBarView(
                  controller: _tabController,
                  children: [
                    // Tab 1: Details & Overview
                    _buildDetailsTab(scrollController),
                    // Tab 2: Interactive Release Search
                    _buildSearchTab(scrollController),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _fallbackIcon() {
    return Container(
      width: 60,
      height: 90,
      decoration: BoxDecoration(
        color: ConduitColors.card,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: ConduitColors.border),
      ),
      child: const Center(
        child: Icon(Icons.movie_outlined, color: ConduitColors.textMuted, size: 28),
      ),
    );
  }

  Widget _buildPill(String label, Color color) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: color.withValues(alpha: 0.3)),
      ),
      child: Text(
        label,
        style: TextStyle(color: color, fontSize: 10, fontWeight: FontWeight.bold),
      ),
    );
  }

  Widget _buildDetailsTab(ScrollController scrollController) {
    return ListView(
      controller: scrollController,
      padding: const EdgeInsets.all(16),
      children: [
        if (widget.item.overview != null && widget.item.overview!.isNotEmpty) ...[
          const Text(
            'SYNOPSIS',
            style: TextStyle(color: ConduitColors.textMuted, fontSize: 11, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 6),
          Text(
            widget.item.overview!,
            style: const TextStyle(color: ConduitColors.textSecondary, fontSize: 13, height: 1.4),
          ),
          const SizedBox(height: 16),
        ],

        // Quick Stats Row
        Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: ConduitColors.card,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: ConduitColors.border),
          ),
          child: Column(
            children: [
              _buildStatRow('Type', widget.item.itemType.toUpperCase()),
              if (widget.item.sizeBytes != null)
                _buildStatRow('Size', Formatters.formatBytes(widget.item.sizeBytes!)),
              if (widget.item.rating != null && widget.item.rating! > 0)
                _buildStatRow('Rating', '★ ${widget.item.rating!.toStringAsFixed(1)} / 10'),
              if (widget.item.runtimeMins != null && widget.item.runtimeMins! > 0)
                _buildStatRow('Runtime', '${widget.item.runtimeMins} mins'),
              _buildStatRow('Grabbed', Formatters.formatDate(widget.item.createdAt)),
            ],
          ),
        ),

        const SizedBox(height: 20),

        // Action Buttons
        Row(
          children: [
            Expanded(
              child: ElevatedButton.icon(
                onPressed: _isReSearching ? null : _handleReSearch,
                icon: _isReSearching
                    ? const SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.refresh, size: 18),
                label: Text(_isReSearching ? 'Requesting...' : 'Trigger Arr Re-Search'),
                style: ElevatedButton.styleFrom(
                  backgroundColor: ConduitColors.card,
                  foregroundColor: ConduitColors.brandLight,
                  side: const BorderSide(color: ConduitColors.brandLight),
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildStatRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(color: ConduitColors.textMuted, fontSize: 12)),
          Text(value, style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 12, fontWeight: FontWeight.w600)),
        ],
      ),
    );
  }

  Widget _buildSearchTab(ScrollController scrollController) {
    if (_isLoadingReleases) {
      return const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            CircularProgressIndicator(color: ConduitColors.brandLight),
            SizedBox(height: 12),
            Text('Searching configured indexers...', style: TextStyle(color: ConduitColors.textSecondary, fontSize: 13)),
          ],
        ),
      );
    }

    if (_releases.isEmpty && _releasesError == null) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.travel_explore, size: 48, color: ConduitColors.textMuted),
            const SizedBox(height: 12),
            const Text(
              'Interactive Indexer Search',
              style: TextStyle(color: ConduitColors.textPrimary, fontSize: 15, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 6),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 32),
              child: Text(
                'Directly query Sonarr, Radarr, or Lidarr indexers and grab alternative releases in 1-click.',
                textAlign: TextAlign.center,
                style: TextStyle(color: ConduitColors.textMuted, fontSize: 12),
              ),
            ),
            const SizedBox(height: 20),
            ElevatedButton.icon(
              onPressed: _fetchReleases,
              icon: const Icon(Icons.search),
              label: const Text('Search Indexers Now'),
              style: ElevatedButton.styleFrom(backgroundColor: ConduitColors.brandLight),
            ),
          ],
        ),
      );
    }

    if (_releasesError != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.error_outline, size: 40, color: ConduitColors.error),
              const SizedBox(height: 12),
              Text(_releasesError!, style: const TextStyle(color: ConduitColors.error, fontSize: 13), textAlign: TextAlign.center),
              const SizedBox(height: 16),
              ElevatedButton(onPressed: _fetchReleases, child: const Text('Retry Search')),
            ],
          ),
        ),
      );
    }

    return ListView.builder(
      controller: scrollController,
      padding: const EdgeInsets.all(16),
      itemCount: _releases.length,
      itemBuilder: (context, index) {
        final rel = _releases[index];
        final isGrabbing = _grabbingGuid == rel.guid;

        return Card(
          margin: const EdgeInsets.only(bottom: 10),
          color: ConduitColors.card,
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  rel.title,
                  style: const TextStyle(fontSize: 13, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    if (rel.indexer != null) _buildPill(rel.indexer!, Colors.amber),
                    const SizedBox(width: 6),
                    if (rel.quality != null) _buildPill(rel.quality!, ConduitColors.purpleLight),
                    const SizedBox(width: 6),
                    Text(
                      Formatters.formatBytes(rel.size),
                      style: const TextStyle(color: ConduitColors.textMuted, fontSize: 11),
                    ),
                    const Spacer(),
                    Text(
                      '↑${rel.seeders}  ↓${rel.leechers}',
                      style: const TextStyle(color: ConduitColors.success, fontSize: 11, fontWeight: FontWeight.bold),
                    ),
                  ],
                ),
                const SizedBox(height: 10),
                SizedBox(
                  width: double.infinity,
                  child: ElevatedButton.icon(
                    onPressed: isGrabbing ? null : () => _handleGrab(rel),
                    icon: isGrabbing
                        ? const SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2))
                        : const Icon(Icons.download, size: 16),
                    label: Text(isGrabbing ? 'Grabbing...' : '1-Click Grab'),
                    style: ElevatedButton.styleFrom(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      backgroundColor: ConduitColors.brandLight,
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
