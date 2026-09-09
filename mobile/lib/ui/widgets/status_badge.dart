import 'package:flutter/material.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/torrent.dart';

class StatusBadge extends StatelessWidget {
  final Torrent torrent;

  const StatusBadge({super.key, required this.torrent});

  @override
  Widget build(BuildContext context) {
    if (torrent.isCanaryProbe) {
      return _buildBadge(
        label: 'CANARY',
        icon: Icons.radar,
        color: ConduitColors.warning,
        bgColor: ConduitColors.warningBg,
      );
    }

    if (torrent.isCircuitBroken) {
      return _buildBadge(
        label: 'CIRCUIT BROKEN',
        icon: Icons.bolt,
        color: ConduitColors.warningLight,
        bgColor: ConduitColors.warningBg,
      );
    }

    if (torrent.hasError) {
      return _buildBadge(
        label: 'ERROR',
        icon: Icons.error_outline,
        color: ConduitColors.error,
        bgColor: ConduitColors.errorBg,
      );
    }

    if (torrent.isDownloading) {
      return _buildBadge(
        label: 'DOWNLOADING',
        icon: Icons.downloading,
        color: ConduitColors.brandLight,
        bgColor: ConduitColors.brand.withValues(alpha: 0.15),
      );
    }

    if (torrent.isSeeding) {
      return _buildBadge(
        label: 'SEEDING',
        icon: Icons.upload,
        color: ConduitColors.successLight,
        bgColor: ConduitColors.successBg,
      );
    }

    if (torrent.isPaused) {
      return _buildBadge(
        label: 'PAUSED',
        icon: Icons.pause,
        color: ConduitColors.textMuted,
        bgColor: ConduitColors.cardElevated.withValues(alpha: 0.5),
      );
    }

    return _buildBadge(
      label: torrent.status.toUpperCase(),
      icon: Icons.circle,
      color: ConduitColors.textSecondary,
      bgColor: ConduitColors.cardElevated,
    );
  }

  Widget _buildBadge({
    required String label,
    required IconData icon,
    required Color color,
    required Color bgColor,
  }) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      decoration: BoxDecoration(
        color: bgColor,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: color.withValues(alpha: 0.3), width: 1),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 11, color: color),
          const SizedBox(width: 4),
          Text(
            label,
            style: TextStyle(
              color: color,
              fontSize: 10,
              fontWeight: FontWeight.bold,
              letterSpacing: 0.5,
            ),
          ),
        ],
      ),
    );
  }
}
