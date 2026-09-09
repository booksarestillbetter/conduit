import 'dart:async';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/torrent.dart';
import '../../core/models/peer.dart';
import '../../core/models/media_grab.dart';
import '../../core/network/api_client.dart';
import '../../core/utils/formatters.dart';
import '../../providers/torrents_provider.dart';
import '../widgets/status_badge.dart';
import '../widgets/piece_map_grid.dart';

class TorrentDetailScreen extends StatefulWidget {
  final String compoundId;

  const TorrentDetailScreen({super.key, required this.compoundId});

  @override
  State<TorrentDetailScreen> createState() => _TorrentDetailScreenState();
}

class _TorrentDetailScreenState extends State<TorrentDetailScreen> with SingleTickerProviderStateMixin {
  late TabController _tabController;
  Torrent? _torrent;
  bool _isLoading = true;
  Timer? _refreshTimer;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 5, vsync: this);
    _loadDetails();
    _refreshTimer = Timer.periodic(const Duration(seconds: 2), (_) => _loadDetails(silent: true));
  }

  @override
  void dispose() {
    _refreshTimer?.cancel();
    _tabController.dispose();
    super.dispose();
  }

  Future<void> _loadDetails({bool silent = false}) async {
    if (!silent && _torrent == null) {
      setState(() => _isLoading = true);
    }
    try {
      final t = await ApiClient.getTorrentDetails(widget.compoundId);
      if (mounted) {
        setState(() {
          _torrent = t;
          _isLoading = false;
        });
      }
    } catch (_) {
      if (mounted) {
        setState(() => _isLoading = false);
      }
    }
  }

  void _confirmDelete() {
    showDialog(
      context: context,
      builder: (ctx) {
        var deleteFiles = false;
        return StatefulBuilder(
          builder: (context, setDialogState) {
            return AlertDialog(
              backgroundColor: ConduitColors.surface,
              title: const Text('Delete Fetcher', style: TextStyle(color: ConduitColors.textPrimary)),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Are you sure you want to remove "${_torrent?.name ?? "this torrent"}" from ${_torrent?.node ?? "Retriever"}?',
                    style: const TextStyle(color: ConduitColors.textSecondary, fontSize: 13),
                  ),
                  const SizedBox(height: 16),
                  Row(
                    children: [
                      Checkbox(
                        value: deleteFiles,
                        activeColor: ConduitColors.error,
                        onChanged: (val) => setDialogState(() => deleteFiles = val ?? false),
                      ),
                      const Expanded(
                        child: Text(
                          'Also delete downloaded files from disk',
                          style: TextStyle(color: ConduitColors.errorLight, fontSize: 12),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(ctx),
                  child: const Text('Cancel', style: TextStyle(color: ConduitColors.textMuted)),
                ),
                ElevatedButton(
                  style: ElevatedButton.styleFrom(backgroundColor: ConduitColors.error),
                  onPressed: () async {
                    final nav = Navigator.of(context);
                    Navigator.pop(ctx);
                    final ok = await context.read<TorrentsProvider>().deleteTorrent(
                          widget.compoundId,
                          deleteFiles: deleteFiles,
                        );
                    if (ok && mounted) {
                      nav.pop();
                    }
                  },
                  child: const Text('Delete'),
                ),
              ],
            );
          },
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_isLoading) {
      return Scaffold(
        appBar: AppBar(title: const Text('Loading Swarm...')),
        body: const Center(child: CircularProgressIndicator(color: ConduitColors.brandLight)),
      );
    }

    if (_torrent == null) {
      return Scaffold(
        appBar: AppBar(title: const Text('Fetcher Not Found')),
        body: Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.error_outline, size: 48, color: ConduitColors.error),
              const SizedBox(height: 12),
              Text(
                'Could not load torrent "${widget.compoundId}"',
                style: const TextStyle(color: ConduitColors.textSecondary),
              ),
              const SizedBox(height: 16),
              ElevatedButton(
                onPressed: () => _loadDetails(),
                child: const Text('Retry'),
              ),
            ],
          ),
        ),
      );
    }

    final t = _torrent!;

    return Scaffold(
      appBar: AppBar(
        title: Text(
          t.name,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(fontSize: 15),
        ),
        actions: [
          IconButton(
            icon: Icon(
              t.isPaused ? Icons.play_arrow : Icons.pause,
              color: t.isPaused ? ConduitColors.successLight : ConduitColors.warningLight,
            ),
            tooltip: t.isPaused ? 'Resume' : 'Pause',
            onPressed: () async {
              final prov = context.read<TorrentsProvider>();
              if (t.isPaused) {
                await prov.startTorrent(t.compoundId);
              } else {
                await prov.stopTorrent(t.compoundId);
              }
              _loadDetails(silent: true);
            },
          ),
          IconButton(
            icon: const Icon(Icons.delete_outline, color: ConduitColors.errorLight),
            tooltip: 'Delete',
            onPressed: _confirmDelete,
          ),
        ],
        bottom: TabBar(
          controller: _tabController,
          isScrollable: true,
          tabAlignment: TabAlignment.start,
          indicatorColor: ConduitColors.brandLight,
          labelColor: ConduitColors.brandLight,
          unselectedLabelColor: ConduitColors.textMuted,
          tabs: [
            const Tab(text: 'Overview'),
            Tab(text: 'Files (${t.files.length})'),
            Tab(text: 'Trackers (${t.trackerStats.length})'),
            Tab(text: 'Peers (${t.peers.length})'),
            const Tab(text: 'Trail'),
          ],
        ),
      ),
      body: TabBarView(
        controller: _tabController,
        children: [
          _buildOverviewTab(t),
          _buildFilesTab(t),
          _buildTrackersTab(t),
          _buildPeersAndHeatmapTab(t),
          _buildTimelineTab(t),
        ],
      ),
    );
  }

  Widget _buildOverviewTab(Torrent t) {
    final grab = t.arrGrab;
    final hasPoster = grab?.posterUrl != null && grab!.posterUrl!.isNotEmpty;

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        // Media Hero Card (if Arr metadata is present)
        if (grab != null) ...[
          Container(
            padding: const EdgeInsets.all(14),
            margin: const EdgeInsets.only(bottom: 16),
            decoration: BoxDecoration(
              color: ConduitColors.card,
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: ConduitColors.border),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (hasPoster)
                  ClipRRect(
                    borderRadius: BorderRadius.circular(8),
                    child: Image.network(
                      grab.posterUrl!,
                      width: 56,
                      height: 84,
                      fit: BoxFit.cover,
                      errorBuilder: (ctx, err, stack) => _buildDetailFallbackIcon(grab),
                    ),
                  )
                else
                  _buildDetailFallbackIcon(grab),
                const SizedBox(width: 14),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        grab.title,
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
                        t.name,
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
                          if (grab.quality != null)
                            _buildPill(grab.quality!, ConduitColors.purpleLight),
                          if (grab.indexer != null)
                            _buildPill(grab.indexer!, Colors.amber),
                          _buildPill(grab.itemType.toUpperCase(), ConduitColors.brandLight),
                          if (grab.year != null && grab.year! > 0)
                            _buildPill('${grab.year}', ConduitColors.textSecondary),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],

        // Status & Progress Card
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: ConduitColors.card,
            borderRadius: BorderRadius.circular(16),
            border: Border.all(color: ConduitColors.border),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  StatusBadge(torrent: t),
                  Text(
                    '${(t.progress * 100).toStringAsFixed(1)}%',
                    style: const TextStyle(
                      fontSize: 18,
                      fontWeight: FontWeight.bold,
                      color: ConduitColors.textPrimary,
                      fontFamily: 'monospace',
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              ClipRRect(
                borderRadius: BorderRadius.circular(4),
                child: LinearProgressIndicator(
                  value: t.progress,
                  minHeight: 8,
                  backgroundColor: ConduitColors.surface,
                  valueColor: AlwaysStoppedAnimation<Color>(
                    t.isComplete ? ConduitColors.success : ConduitColors.brandLight,
                  ),
                ),
              ),
              const SizedBox(height: 12),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    '${Formatters.formatBytes(t.downloadedBytes)} of ${Formatters.formatBytes(t.totalSize)}',
                    style: const TextStyle(color: ConduitColors.textMuted, fontSize: 12),
                  ),
                  Text(
                    'Ratio: ${t.uploadRatio.toStringAsFixed(2)}',
                    style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 12, fontWeight: FontWeight.bold),
                  ),
                ],
              ),
            ],
          ),
        ),

        const SizedBox(height: 16),

        // Speed Metrics
        Row(
          children: [
            Expanded(
              child: _buildMetricCard(
                'Download Rate',
                Formatters.formatSpeed(t.rateDownload),
                Icons.arrow_downward,
                ConduitColors.brandLight,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: _buildMetricCard(
                'Upload Rate',
                Formatters.formatSpeed(t.rateUpload),
                Icons.arrow_upward,
                ConduitColors.purpleLight,
              ),
            ),
          ],
        ),

        const SizedBox(height: 12),

        Row(
          children: [
            Expanded(
              child: _buildMetricCard(
                'ETA',
                Formatters.formatEta(t.eta),
                Icons.timer_outlined,
                Colors.amber,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: _buildMetricCard(
                'Uploaded Total',
                Formatters.formatBytes(t.uploadedBytes),
                Icons.cloud_upload_outlined,
                ConduitColors.success,
              ),
            ),
          ],
        ),

        const SizedBox(height: 16),

        // Metadata Info Block
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: ConduitColors.card,
            borderRadius: BorderRadius.circular(16),
            border: Border.all(color: ConduitColors.border),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _buildInfoRow('Node', t.node),
              if (t.downloadDir != null) _buildInfoRow('Download Path', t.downloadDir!),
              if (t.hashString != null) _buildInfoRow('Info Hash', t.hashString!),
              _buildInfoRow('Peers Connected', '${t.peersConnected} (↓${t.peersSendingToUs} / ↑${t.peersGettingFromUs})'),
              _buildInfoRow('Queue Position', '#${t.queuePosition}'),
              const SizedBox(height: 8),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  const Text('Sequential Download', style: TextStyle(color: ConduitColors.textMuted, fontSize: 12)),
                  Switch(
                    value: t.isSequential,
                    activeThumbColor: ConduitColors.brandLight,
                    onChanged: (val) async {
                      await ApiClient.setSequential(t.compoundId, val);
                      _loadDetails(silent: true);
                    },
                  ),
                ],
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildMetricCard(String label, String value, IconData icon, Color color) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: ConduitColors.card,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: ConduitColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, size: 14, color: color),
              const SizedBox(width: 6),
              Text(label, style: const TextStyle(color: ConduitColors.textMuted, fontSize: 11)),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            value,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.bold,
              color: color,
              fontFamily: 'monospace',
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildInfoRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 120,
            child: Text(label, style: const TextStyle(color: ConduitColors.textMuted, fontSize: 12)),
          ),
          Expanded(
            child: Text(
              value,
              style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 12, fontWeight: FontWeight.w500),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildFilesTab(Torrent t) {
    if (t.files.isEmpty) {
      return const Center(
        child: Text('No individual files reported for this fetcher', style: TextStyle(color: ConduitColors.textMuted)),
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.all(16),
      itemCount: t.files.length,
      itemBuilder: (context, index) {
        final f = t.files[index];
        return Card(
          margin: const EdgeInsets.only(bottom: 8),
          color: ConduitColors.card,
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  f.name,
                  style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 13, fontWeight: FontWeight.w600),
                ),
                const SizedBox(height: 6),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text(
                      '${Formatters.formatBytes(f.bytesCompleted)} / ${Formatters.formatBytes(f.length)}',
                      style: const TextStyle(color: ConduitColors.textMuted, fontSize: 11),
                    ),
                    Text(
                      '${(f.progress * 100).toStringAsFixed(1)}%',
                      style: const TextStyle(color: ConduitColors.brandLight, fontSize: 11, fontWeight: FontWeight.bold),
                    ),
                  ],
                ),
                const SizedBox(height: 6),
                ClipRRect(
                  borderRadius: BorderRadius.circular(3),
                  child: LinearProgressIndicator(
                    value: f.progress,
                    minHeight: 4,
                    backgroundColor: ConduitColors.surface,
                    valueColor: AlwaysStoppedAnimation<Color>(
                      f.progress >= 1.0 ? ConduitColors.success : ConduitColors.brandLight,
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

  Widget _buildTrackersTab(Torrent t) {
    if (t.trackerStats.isEmpty) {
      return const Center(
        child: Text('No tracker scrape data available', style: TextStyle(color: ConduitColors.textMuted)),
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.all(16),
      itemCount: t.trackerStats.length,
      itemBuilder: (context, index) {
        final ts = t.trackerStats[index];
        final hasError = !ts.lastAnnounceSucceeded && ts.lastAnnounceResult.isNotEmpty;

        return Card(
          margin: const EdgeInsets.only(bottom: 8),
          color: ConduitColors.card,
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Expanded(
                      child: Text(
                        ts.host,
                        style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 13, fontWeight: FontWeight.bold),
                      ),
                    ),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                      decoration: BoxDecoration(
                        color: hasError ? ConduitColors.warningBg : ConduitColors.successBg,
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Text(
                        hasError ? 'ANNOUNCE ERROR' : 'ACTIVE',
                        style: TextStyle(
                          color: hasError ? ConduitColors.warningLight : ConduitColors.successLight,
                          fontSize: 9.5,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    Text('Tier ${ts.tier}', style: const TextStyle(color: ConduitColors.textMuted, fontSize: 11)),
                    const Spacer(),
                    Text('Seeds: ${ts.seederCount}', style: const TextStyle(color: ConduitColors.success, fontSize: 11)),
                    const SizedBox(width: 12),
                    Text('Leeches: ${ts.leecherCount}', style: const TextStyle(color: Colors.amber, fontSize: 11)),
                  ],
                ),
                if (hasError) ...[
                  const SizedBox(height: 6),
                  Text(
                    ts.lastAnnounceResult,
                    style: const TextStyle(color: ConduitColors.warningLight, fontSize: 11),
                  ),
                ],
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _buildPeersAndHeatmapTab(Torrent t) {
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        if (t.pieces != null && t.pieces!.isNotEmpty) ...[
          const Text(
            'PIECE MAP HEATMAP',
            style: TextStyle(color: ConduitColors.textMuted, fontSize: 11, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 8),
          PieceMapGrid(
            piecesBase64: t.pieces,
            pieceCount: t.pieceCount,
            availability: t.availability,
          ),
          const SizedBox(height: 20),
        ],

        const Text(
          'CONNECTED SWARM PEERS',
          style: TextStyle(color: ConduitColors.textMuted, fontSize: 11, fontWeight: FontWeight.bold),
        ),
        const SizedBox(height: 8),
        if (t.peers.isEmpty)
          const Center(
            child: Padding(
              padding: EdgeInsets.all(24),
              child: Text('No active peer connections in swarm', style: TextStyle(color: ConduitColors.textMuted)),
            ),
          )
        else
          ...t.peers.map((p) => _buildPeerTile(p)),
      ],
    );
  }

  Widget _buildPeerTile(Peer p) {
    return Card(
      margin: const EdgeInsets.only(bottom: 6),
      color: ConduitColors.card,
      child: Padding(
        padding: const EdgeInsets.all(10),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Text(
                        p.address,
                        style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 12, fontFamily: 'monospace', fontWeight: FontWeight.bold),
                      ),
                      if (p.countryCode != null) ...[
                        const SizedBox(width: 6),
                        Text('(${p.countryCode})', style: const TextStyle(color: ConduitColors.textMuted, fontSize: 10)),
                      ],
                    ],
                  ),
                  const SizedBox(height: 2),
                  Text(
                    p.clientName,
                    style: const TextStyle(color: ConduitColors.textMuted, fontSize: 11),
                  ),
                ],
              ),
            ),
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Text(
                  '↓ ${Formatters.formatSpeed(p.rateToClient)}',
                  style: const TextStyle(color: ConduitColors.brandLight, fontSize: 11, fontWeight: FontWeight.bold, fontFamily: 'monospace'),
                ),
                Text(
                  '↑ ${Formatters.formatSpeed(p.rateToPeer)}',
                  style: const TextStyle(color: ConduitColors.purpleLight, fontSize: 11, fontFamily: 'monospace'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildTimelineTab(Torrent t) {
    if (t.timeline.isEmpty) {
      return const Center(
        child: Text('No trail events recorded yet', style: TextStyle(color: ConduitColors.textMuted)),
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.all(16),
      itemCount: t.timeline.length,
      itemBuilder: (context, index) {
        final ev = t.timeline[index];
        return Padding(
          padding: const EdgeInsets.only(bottom: 12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 10,
                height: 10,
                margin: const EdgeInsets.only(top: 4, right: 12),
                decoration: const BoxDecoration(
                  color: ConduitColors.brandLight,
                  shape: BoxShape.circle,
                ),
              ),
              Expanded(
                child: Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: ConduitColors.card,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: ConduitColors.border),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        ev.title,
                        style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 13, fontWeight: FontWeight.bold),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        ev.description,
                        style: const TextStyle(color: ConduitColors.textSecondary, fontSize: 12),
                      ),
                      const SizedBox(height: 6),
                      Text(
                        ev.formattedTime,
                        style: const TextStyle(color: ConduitColors.textMuted, fontSize: 10),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _buildDetailFallbackIcon(MediaGrab grab) {
    IconData icon = Icons.movie;
    Color color = ConduitColors.brandLight;
    if (grab.isSeries) {
      icon = Icons.tv;
      color = ConduitColors.brandSecondary;
    } else if (grab.isMusic) {
      icon = Icons.music_note;
      color = ConduitColors.purpleLight;
    }

    return Container(
      width: 56,
      height: 84,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: color.withValues(alpha: 0.3)),
      ),
      child: Center(child: Icon(icon, size: 28, color: color)),
    );
  }

  Widget _buildPill(String label, Color color) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: color.withValues(alpha: 0.3)),
      ),
      child: Text(
        label,
        style: TextStyle(color: color, fontSize: 9.5, fontWeight: FontWeight.bold),
      ),
    );
  }
}
