import 'package:flutter/material.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/peer.dart';
import '../../core/utils/formatters.dart';

class SwarmPeersTab extends StatelessWidget {
  final List<Peer> peers;
  final bool isLoading;

  const SwarmPeersTab({
    super.key,
    required this.peers,
    this.isLoading = false,
  });

  @override
  Widget build(BuildContext context) {
    if (isLoading) {
      return const Center(
        child: CircularProgressIndicator(color: ConduitColors.brandLight),
      );
    }

    if (peers.isEmpty) {
      return Container(
        padding: const EdgeInsets.all(24),
        alignment: Alignment.center,
        child: const Text(
          'No active peer connections in swarm',
          style: TextStyle(color: ConduitColors.textMuted, fontSize: 13),
        ),
      );
    }

    return ListView.separated(
      padding: const EdgeInsets.symmetric(vertical: 8),
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      itemCount: peers.length,
      separatorBuilder: (_, _) => const Divider(color: ConduitColors.borderSubtle),
      itemBuilder: (context, index) {
        final p = peers[index];
        final percent = (p.progress * 100).toStringAsFixed(0);

        return Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              // Client Icon & IP
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Text(
                          p.address,
                          style: const TextStyle(
                            color: ConduitColors.textPrimary,
                            fontSize: 12.5,
                            fontFamily: 'monospace',
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                        if (p.isEncrypted) ...[
                          const SizedBox(width: 4),
                          const Icon(Icons.lock, size: 12, color: ConduitColors.successLight),
                        ],
                        if (p.isUtp) ...[
                          const SizedBox(width: 4),
                          Container(
                            padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 1),
                            decoration: BoxDecoration(
                              color: ConduitColors.purpleBg,
                              borderRadius: BorderRadius.circular(3),
                            ),
                            child: const Text(
                              'uTP',
                              style: TextStyle(
                                color: ConduitColors.purpleLight,
                                fontSize: 8,
                                fontWeight: FontWeight.bold,
                              ),
                            ),
                          ),
                        ],
                      ],
                    ),
                    const SizedBox(height: 2),
                    Text(
                      '${p.clientName} ${p.flagStr.isNotEmpty ? "(${p.flagStr})" : ""}',
                      style: const TextStyle(
                        color: ConduitColors.textMuted,
                        fontSize: 11,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),

              // Progress %
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: ConduitColors.surface,
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  '$percent%',
                  style: const TextStyle(
                    color: ConduitColors.textSecondary,
                    fontSize: 11,
                    fontFamily: 'monospace',
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),

              const SizedBox(width: 12),

              // DL / UL Speeds
              Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  if (p.rateToClient > 0)
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const Icon(Icons.arrow_downward, size: 10, color: ConduitColors.brandLight),
                        const SizedBox(width: 2),
                        Text(
                          Formatters.formatSpeed(p.rateToClient),
                          style: const TextStyle(
                            color: ConduitColors.brandLight,
                            fontSize: 10.5,
                            fontFamily: 'monospace',
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ],
                    ),
                  if (p.rateToPeer > 0)
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const Icon(Icons.arrow_upward, size: 10, color: ConduitColors.successLight),
                        const SizedBox(width: 2),
                        Text(
                          Formatters.formatSpeed(p.rateToPeer),
                          style: const TextStyle(
                            color: ConduitColors.successLight,
                            fontSize: 10.5,
                            fontFamily: 'monospace',
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ],
                    ),
                  if (p.rateToClient == 0 && p.rateToPeer == 0)
                    const Text(
                      'Idle',
                      style: TextStyle(
                        color: ConduitColors.textMuted,
                        fontSize: 10.5,
                      ),
                    ),
                ],
              ),
            ],
          ),
        );
      },
    );
  }
}
