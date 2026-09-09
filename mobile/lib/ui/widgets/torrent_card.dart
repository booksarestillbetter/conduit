import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/torrent.dart';
import '../../core/utils/formatters.dart';
import '../../providers/torrents_provider.dart';
import 'status_badge.dart';

class TorrentCard extends StatelessWidget {
  final Torrent torrent;
  final VoidCallback onTap;

  const TorrentCard({
    super.key,
    required this.torrent,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final progressPercent = (torrent.progress * 100).toStringAsFixed(1);

    Color progressColor = ConduitColors.brand;
    if (torrent.isSeeding) {
      progressColor = ConduitColors.success;
    } else if (torrent.isCircuitBroken || torrent.isCanaryProbe) {
      progressColor = ConduitColors.warning;
    } else if (torrent.hasError) {
      progressColor = ConduitColors.error;
    } else if (torrent.isPaused) {
      progressColor = ConduitColors.textMuted;
    }

    final poster = torrent.posterUrl;
    final hasPoster = poster != null && poster.isNotEmpty;
    final hasEnrichedMeta = torrent.hasArrMetadata;

    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(16),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Top Row: StatusBadge, Node, SEQ, Queue #, Play/Pause
              Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  StatusBadge(torrent: torrent),
                  const SizedBox(width: 8),
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                    decoration: BoxDecoration(
                      color: ConduitColors.surface,
                      borderRadius: BorderRadius.circular(4),
                      border: Border.all(color: ConduitColors.border),
                    ),
                    child: Text(
                      torrent.node,
                      style: const TextStyle(
                        color: ConduitColors.textSecondary,
                        fontSize: 10,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  if (torrent.queuePosition > 0) ...[
                    const SizedBox(width: 6),
                    Text(
                      '#${torrent.queuePosition}',
                      style: const TextStyle(
                        color: ConduitColors.textMuted,
                        fontSize: 10,
                        fontFamily: 'monospace',
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ],
                  if (torrent.isSequential) ...[
                    const SizedBox(width: 6),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                      decoration: BoxDecoration(
                        color: ConduitColors.brandAccent.withValues(alpha: 0.2),
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: const Text(
                        'SEQ',
                        style: TextStyle(
                          color: ConduitColors.brandAccent,
                          fontSize: 9,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                    ),
                  ],
                  const Spacer(),
                  IconButton(
                    iconSize: 18,
                    visualDensity: VisualDensity.compact,
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                    icon: Icon(
                      torrent.isPaused ? Icons.play_arrow : Icons.pause,
                      color: torrent.isPaused ? ConduitColors.successLight : ConduitColors.warningLight,
                    ),
                    onPressed: () {
                      final provider = context.read<TorrentsProvider>();
                      if (torrent.isPaused) {
                        provider.startTorrent(torrent.compoundId);
                      } else {
                        provider.stopTorrent(torrent.compoundId);
                      }
                    },
                  ),
                ],
              ),

              const SizedBox(height: 10),

              // Content row: Poster on the left (if available), Title & Info on the right
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (hasPoster)
                    Padding(
                      padding: const EdgeInsets.only(right: 12),
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(8),
                        child: Image.network(
                          poster,
                          width: 48,
                          height: 72,
                          fit: BoxFit.cover,
                          errorBuilder: (ctx, err, stack) => _buildFallbackPosterIcon(),
                        ),
                      ),
                    )
                  else if (hasEnrichedMeta)
                    Padding(
                      padding: const EdgeInsets.only(right: 12),
                      child: _buildFallbackPosterIcon(),
                    ),

                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        // Display title (enriched clean title)
                        Text(
                          torrent.displayTitle,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            color: ConduitColors.textPrimary,
                            fontSize: 14,
                            fontWeight: FontWeight.bold,
                            height: 1.25,
                          ),
                        ),

                        // If enriched title differs from raw name, show raw name below in subtle monospace
                        if (hasEnrichedMeta && torrent.displayTitle != torrent.name) ...[
                          const SizedBox(height: 3),
                          Text(
                            torrent.name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              color: ConduitColors.textMuted,
                              fontSize: 10.5,
                              fontFamily: 'monospace',
                            ),
                          ),
                        ],

                        // Metadata Badges (Quality, Indexer, Media Type)
                        if (hasEnrichedMeta) ...[
                          const SizedBox(height: 6),
                          Wrap(
                            spacing: 6,
                            runSpacing: 4,
                            children: [
                              if (torrent.quality != null)
                                _buildBadge(torrent.quality!, ConduitColors.purpleLight),
                              if (torrent.indexer != null)
                                _buildBadge(torrent.indexer!, Colors.amber),
                              if (torrent.mediaType != null)
                                _buildBadge(torrent.mediaType!.toUpperCase(), ConduitColors.brandLight),
                            ],
                          ),
                        ],
                      ],
                    ),
                  ),
                ],
              ),

              const SizedBox(height: 12),

              // Progress Bar
              ClipRRect(
                borderRadius: BorderRadius.circular(4),
                child: LinearProgressIndicator(
                  value: torrent.progress.clamp(0.0, 1.0),
                  backgroundColor: ConduitColors.surface,
                  valueColor: AlwaysStoppedAnimation<Color>(progressColor),
                  minHeight: 6,
                ),
              ),

              const SizedBox(height: 10),

              // Stats Row 1: Sizes & Progress %
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    '${Formatters.formatBytes(torrent.downloadedBytes)} / ${Formatters.formatBytes(torrent.totalSize)}',
                    style: const TextStyle(
                      color: ConduitColors.textSecondary,
                      fontSize: 11,
                      fontFamily: 'monospace',
                    ),
                  ),
                  Text(
                    '$progressPercent%',
                    style: const TextStyle(
                      color: ConduitColors.textPrimary,
                      fontSize: 11,
                      fontWeight: FontWeight.bold,
                      fontFamily: 'monospace',
                    ),
                  ),
                ],
              ),

              const SizedBox(height: 4),

              // Stats Row 2: DL / UL Speeds, ETA, Ratio
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Row(
                    children: [
                      if (torrent.rateDownload > 0) ...[
                        const Icon(Icons.arrow_downward, size: 12, color: ConduitColors.brandLight),
                        const SizedBox(width: 2),
                        Text(
                          Formatters.formatSpeed(torrent.rateDownload),
                          style: const TextStyle(
                            color: ConduitColors.brandLight,
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            fontFamily: 'monospace',
                          ),
                        ),
                        const SizedBox(width: 8),
                      ],
                      if (torrent.rateUpload > 0) ...[
                        const Icon(Icons.arrow_upward, size: 12, color: ConduitColors.purpleLight),
                        const SizedBox(width: 2),
                        Text(
                          Formatters.formatSpeed(torrent.rateUpload),
                          style: const TextStyle(
                            color: ConduitColors.purpleLight,
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            fontFamily: 'monospace',
                          ),
                        ),
                      ],
                    ],
                  ),
                  Row(
                    children: [
                      if (torrent.eta > 0 && !torrent.isComplete) ...[
                        Text(
                          'ETA: ${Formatters.formatEta(torrent.eta)}',
                          style: const TextStyle(
                            color: ConduitColors.textMuted,
                            fontSize: 11,
                          ),
                        ),
                        const SizedBox(width: 8),
                      ],
                      Text(
                        'Ratio: ${Formatters.formatRatio(torrent.uploadRatio)}',
                        style: const TextStyle(
                          color: ConduitColors.textMuted,
                          fontSize: 11,
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildFallbackPosterIcon() {
    IconData icon = Icons.movie;
    Color color = ConduitColors.brandLight;
    if (torrent.mediaType == 'series' || torrent.mediaType == 'tv') {
      icon = Icons.tv;
      color = ConduitColors.brandSecondary;
    } else if (torrent.mediaType == 'music') {
      icon = Icons.music_note;
      color = ConduitColors.purpleLight;
    }

    return Container(
      width: 48,
      height: 72,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: color.withValues(alpha: 0.3)),
      ),
      child: Center(child: Icon(icon, size: 24, color: color)),
    );
  }

  Widget _buildBadge(String label, Color color) {
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
