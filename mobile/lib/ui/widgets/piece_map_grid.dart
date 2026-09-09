import 'dart:convert';
import 'package:flutter/material.dart';
import '../../core/constants/app_colors.dart';

class PieceMapGrid extends StatelessWidget {
  final String? piecesBase64;
  final int pieceCount;
  final List<int>? availability;

  const PieceMapGrid({
    super.key,
    this.piecesBase64,
    required this.pieceCount,
    this.availability,
  });

  @override
  Widget build(BuildContext context) {
    if (pieceCount <= 0) {
      return Container(
        height: 100,
        alignment: Alignment.center,
        child: const Text(
          'Piece map telemetry unavailable',
          style: TextStyle(color: ConduitColors.textMuted, fontSize: 12),
        ),
      );
    }

    // Decode piece bitfield if available
    List<bool> downloadedPieces = [];
    if (piecesBase64 != null && piecesBase64!.isNotEmpty) {
      try {
        final bytes = base64Decode(piecesBase64!);
        for (final b in bytes) {
          for (int bit = 7; bit >= 0; bit--) {
            if (downloadedPieces.length < pieceCount) {
              downloadedPieces.add(((b >> bit) & 1) == 1);
            }
          }
        }
      } catch (e) {
        // Fallback
      }
    }

    // Ensure array length matches pieceCount
    while (downloadedPieces.length < pieceCount) {
      downloadedPieces.add(false);
    }

    final maxCells = pieceCount.clamp(0, 1000); // Visual ceiling for mobile rendering performance
    final sampleRatio = pieceCount > 1000 ? pieceCount / 1000.0 : 1.0;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              'Piece Map ($pieceCount pieces)',
              style: const TextStyle(
                color: ConduitColors.textSecondary,
                fontSize: 12,
                fontWeight: FontWeight.bold,
              ),
            ),
            Row(
              children: [
                _buildLegendItem('Done', ConduitColors.brandLight),
                const SizedBox(width: 8),
                _buildLegendItem('Missing', ConduitColors.cardElevated),
                if (availability != null && availability!.isNotEmpty) ...[
                  const SizedBox(width: 8),
                  _buildLegendItem('In Swarm', ConduitColors.successLight),
                ],
              ],
            ),
          ],
        ),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.all(8),
          decoration: BoxDecoration(
            color: ConduitColors.surface,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: ConduitColors.border),
          ),
          child: Wrap(
            spacing: 2.5,
            runSpacing: 2.5,
            children: List.generate(maxCells, (index) {
              final pieceIdx = (index * sampleRatio).floor().clamp(0, pieceCount - 1);
              final isDownloaded = pieceIdx < downloadedPieces.length && downloadedPieces[pieceIdx];

              Color cellColor = ConduitColors.cardElevated.withValues(alpha: 0.5);
              if (isDownloaded) {
                cellColor = ConduitColors.brandLight;
              } else if (availability != null &&
                  pieceIdx < availability!.length &&
                  availability![pieceIdx] > 0) {
                cellColor = ConduitColors.success.withValues(alpha: 0.4);
              }

              return Container(
                width: 7,
                height: 7,
                decoration: BoxDecoration(
                  color: cellColor,
                  borderRadius: BorderRadius.circular(1.5),
                ),
              );
            }),
          ),
        ),
      ],
    );
  }

  Widget _buildLegendItem(String label, Color color) {
    return Row(
      children: [
        Container(
          width: 8,
          height: 8,
          decoration: BoxDecoration(color: color, borderRadius: BorderRadius.circular(2)),
        ),
        const SizedBox(width: 4),
        Text(
          label,
          style: const TextStyle(color: ConduitColors.textMuted, fontSize: 10),
        ),
      ],
    );
  }
}
