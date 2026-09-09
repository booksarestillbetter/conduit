import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/node.dart';
import '../../core/models/circuit_breaker.dart';
import '../../core/utils/formatters.dart';
import '../../providers/nodes_provider.dart';

class NodesScreen extends StatelessWidget {
  const NodesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final nodesProv = context.watch<NodesProvider>();
    final nodes = nodesProv.nodes;
    final breakers = nodesProv.circuitBreakers;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Retriever Nodes & Health'),
      ),
      body: RefreshIndicator(
        color: ConduitColors.brandLight,
        backgroundColor: ConduitColors.surface,
        onRefresh: () => nodesProv.fetchAll(),
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            // Circuit Breakers Section
            if (breakers.isNotEmpty) ...[
              const Text(
                'TRACKER CIRCUIT BREAKERS',
                style: TextStyle(
                  color: ConduitColors.textSecondary,
                  fontSize: 11,
                  fontWeight: FontWeight.bold,
                  letterSpacing: 0.5,
                ),
              ),
              const SizedBox(height: 8),
              ...breakers.map((b) => _buildBreakerCard(b)),
              const SizedBox(height: 16),
            ],

            // Nodes List Section
            const Text(
              'RETRIEVER DAEMONS',
              style: TextStyle(
                color: ConduitColors.textSecondary,
                fontSize: 11,
                fontWeight: FontWeight.bold,
                letterSpacing: 0.5,
              ),
            ),
            const SizedBox(height: 8),
            if (nodes.isEmpty)
              const Center(
                child: Padding(
                  padding: EdgeInsets.all(24),
                  child: Text(
                    'No Retriever daemons connected',
                    style: TextStyle(color: ConduitColors.textMuted),
                  ),
                ),
              )
            else
              ...nodes.map((n) => _buildNodeCard(n)),
          ],
        ),
      ),
    );
  }

  Widget _buildBreakerCard(CircuitBreakerStatus breaker) {
    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      color: ConduitColors.card,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          children: [
            Container(
              padding: const EdgeInsets.all(8),
              decoration: BoxDecoration(
                color: breaker.isBroken ? ConduitColors.warningBg : ConduitColors.successBg,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Icon(
                breaker.isBroken ? Icons.bolt : Icons.check_circle_outline,
                size: 18,
                color: breaker.isBroken ? ConduitColors.warningLight : ConduitColors.successLight,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    breaker.trackerHost,
                    style: const TextStyle(
                      color: ConduitColors.textPrimary,
                      fontSize: 13,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    breaker.isBroken
                        ? 'Breaker Tripped: ${breaker.failureCount} / ${breaker.totalSwarms} swarms failing (${(breaker.failureRatio * 100).toStringAsFixed(0)}%)'
                        : 'Tracker Swarm Nominal (${breaker.totalSwarms} swarms)',
                    style: TextStyle(
                      color: breaker.isBroken ? ConduitColors.warningLight : ConduitColors.textMuted,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildNodeCard(TransmissionNode node) {
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      color: ConduitColors.card,
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Node Name & Online Status
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Row(
                  children: [
                    Container(
                      width: 8,
                      height: 8,
                      decoration: BoxDecoration(
                        color: node.online ? ConduitColors.success : ConduitColors.error,
                        shape: BoxShape.circle,
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      node.name,
                      style: const TextStyle(
                        color: ConduitColors.textPrimary,
                        fontSize: 15,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(width: 8),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                      decoration: BoxDecoration(
                        color: ConduitColors.brandLight.withValues(alpha: 0.15),
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(color: ConduitColors.brandLight.withValues(alpha: 0.3)),
                      ),
                      child: Text(
                        node.clientDisplayName.toUpperCase(),
                        style: const TextStyle(color: ConduitColors.brandLight, fontSize: 9.5, fontWeight: FontWeight.bold),
                      ),
                    ),
                  ],
                ),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                  decoration: BoxDecoration(
                    color: ConduitColors.surface,
                    borderRadius: BorderRadius.circular(4),
                    border: Border.all(color: ConduitColors.border),
                  ),
                  child: Text(
                    '${node.rpcLatencyMs}ms',
                    style: const TextStyle(
                      color: ConduitColors.textMuted,
                      fontSize: 10,
                      fontFamily: 'monospace',
                    ),
                  ),
                ),
              ],
            ),

            const SizedBox(height: 12),

            // Node Stats Grid
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                _buildStatItem('Free Disk', Formatters.formatBytes(node.freeSpaceBytes)),
                _buildStatItem('Swarms', '${node.torrentCount} (${node.activeTorrents} active)'),
                _buildStatItem('DL Rate', Formatters.formatSpeed(node.downloadRate)),
                _buildStatItem('UL Rate', Formatters.formatSpeed(node.uploadRate)),
              ],
            ),

            if (node.altSpeedEnabled) ...[
              const SizedBox(height: 10),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                decoration: BoxDecoration(
                  color: Colors.amber.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(6),
                ),
                child: const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.speed, size: 14, color: Colors.amber),
                    SizedBox(width: 6),
                    Text(
                      'Turtle Mode (Alt-Speeds) Active',
                      style: TextStyle(color: Colors.amber, fontSize: 11, fontWeight: FontWeight.w600),
                    ),
                  ],
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildStatItem(String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: const TextStyle(color: ConduitColors.textMuted, fontSize: 10),
        ),
        const SizedBox(height: 2),
        Text(
          value,
          style: const TextStyle(
            color: ConduitColors.textPrimary,
            fontSize: 12,
            fontWeight: FontWeight.w600,
            fontFamily: 'monospace',
          ),
        ),
      ],
    );
  }
}
