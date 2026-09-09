import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../core/utils/formatters.dart';
import '../../core/models/zone.dart';
import '../../providers/telemetry_provider.dart';
import '../../providers/torrents_provider.dart';
import '../../providers/nodes_provider.dart';
import '../widgets/bandwidth_chart.dart';
import '../widgets/torrent_card.dart';
import '../widgets/turtle_mode_button.dart';
import 'torrent_detail_screen.dart';

class DashboardScreen extends StatefulWidget {
  const DashboardScreen({super.key});

  @override
  State<DashboardScreen> createState() => _DashboardScreenState();
}

class _DashboardScreenState extends State<DashboardScreen> {
  final _searchController = TextEditingController();

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final telemetry = context.watch<TelemetryProvider>();
    final torrentsProv = context.watch<TorrentsProvider>();
    final nodesProv = context.watch<NodesProvider>();

    final stats = telemetry.stats;
    final filtered = torrentsProv.filteredTorrents;
    final nodes = nodesProv.nodes;
    final zones = nodesProv.zones;
    final activeZone = nodesProv.activeZone;

    return Scaffold(
      appBar: AppBar(
        title: Row(
          children: [
            const Text('🐕', style: TextStyle(fontSize: 20)),
            const SizedBox(width: 8),
            const Text(
              'Conduit',
              style: TextStyle(
                fontWeight: FontWeight.w800,
                color: ConduitColors.brandLight,
                letterSpacing: -0.5,
              ),
            ),
            const SizedBox(width: 8),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: ConduitColors.surface,
                borderRadius: BorderRadius.circular(10),
                border: Border.all(color: ConduitColors.border),
              ),
              child: Row(
                children: [
                  Container(
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(
                      color: telemetry.isConnected ? ConduitColors.success : ConduitColors.error,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 4),
                  Text(
                    telemetry.isConnected ? 'LIVE' : 'OFFLINE',
                    style: const TextStyle(
                      color: ConduitColors.textSecondary,
                      fontSize: 9,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          if (zones.isNotEmpty)
            PopupMenuButton<String?>(
              initialValue: activeZone,
              tooltip: 'Filter by Zone',
              icon: Container(
                padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 4),
                decoration: BoxDecoration(
                  color: ConduitColors.surface,
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: ConduitColors.border),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.layers_outlined, size: 14, color: ConduitColors.brandLight),
                    const SizedBox(width: 4),
                    Text(
                      activeZone == null
                          ? 'All'
                          : (zones.firstWhere((z) => z.id == activeZone, orElse: () => ZoneConfig(id: '', name: activeZone)).name),
                      style: const TextStyle(fontSize: 11, color: ConduitColors.textPrimary),
                    ),
                  ],
                ),
              ),
              onSelected: (zid) => nodesProv.setActiveZone(zid),
              itemBuilder: (ctx) => [
                const PopupMenuItem(value: null, child: Text('All Zones (Global)')),
                ...zones.map((z) => PopupMenuItem(value: z.id, child: Text(z.name))),
              ],
            ),
          const TurtleModeButton(),
          const SizedBox(width: 8),
        ],
      ),
      body: RefreshIndicator(
        color: ConduitColors.brandLight,
        backgroundColor: ConduitColors.surface,
        onRefresh: () async {
          await Future.wait([
            torrentsProv.fetchTorrents(),
            nodesProv.fetchAll(),
          ]);
        },
        child: ListView(
          padding: const EdgeInsets.only(bottom: 24),
          children: [
            // Top Bandwidth Banner Card
            Container(
              margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: ConduitColors.surface,
                borderRadius: BorderRadius.circular(16),
                border: Border.all(color: ConduitColors.border),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Title & Live Rates
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      const Text(
                        'Cluster Bandwidth (5m Live)',
                        style: TextStyle(
                          color: ConduitColors.textSecondary,
                          fontSize: 12,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      Row(
                        children: [
                          Row(
                            children: [
                              const Icon(Icons.arrow_downward, size: 13, color: ConduitColors.brandLight),
                              const SizedBox(width: 2),
                              Text(
                                Formatters.formatSpeed(stats.totalDownloadRate),
                                style: const TextStyle(
                                  color: ConduitColors.brandLight,
                                  fontSize: 12,
                                  fontWeight: FontWeight.bold,
                                  fontFamily: 'monospace',
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(width: 12),
                          Row(
                            children: [
                              const Icon(Icons.arrow_upward, size: 13, color: ConduitColors.successLight),
                              const SizedBox(width: 2),
                              Text(
                                Formatters.formatSpeed(stats.totalUploadRate),
                                style: const TextStyle(
                                  color: ConduitColors.successLight,
                                  fontSize: 12,
                                  fontWeight: FontWeight.bold,
                                  fontFamily: 'monospace',
                                ),
                              ),
                            ],
                          ),
                        ],
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  // Rolling 5m Chart
                  BandwidthChart(
                    points: telemetry.rollingHistory,
                    height: 110,
                  ),
                ],
              ),
            ),

            // Node Selector & Search Bar
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Row(
                children: [
                  // Node Dropdown
                  Expanded(
                    flex: 4,
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 10),
                      decoration: BoxDecoration(
                        color: ConduitColors.surface,
                        borderRadius: BorderRadius.circular(12),
                        border: Border.all(color: ConduitColors.border),
                      ),
                      child: DropdownButtonHideUnderline(
                        child: DropdownButton<String>(
                          value: torrentsProv.selectedNode,
                          isExpanded: true,
                          dropdownColor: ConduitColors.surface,
                          icon: const Icon(Icons.keyboard_arrow_down, size: 18, color: ConduitColors.textMuted),
                          items: [
                            const DropdownMenuItem(value: 'all', child: Text('All Nodes', style: TextStyle(fontSize: 12))),
                            ...nodes.map((n) => DropdownMenuItem(
                                  value: n.name,
                                  child: Text(n.name, style: const TextStyle(fontSize: 12), overflow: TextOverflow.ellipsis),
                                )),
                          ],
                          onChanged: (val) {
                            if (val != null) torrentsProv.setSelectedNode(val);
                          },
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 10),
                  // Search Input
                  Expanded(
                    flex: 6,
                    child: TextField(
                      controller: _searchController,
                      style: const TextStyle(fontSize: 12),
                      decoration: InputDecoration(
                        hintText: 'Search swarms...',
                        prefixIcon: const Icon(Icons.search, size: 16, color: ConduitColors.textMuted),
                        suffixIcon: _searchController.text.isNotEmpty
                            ? IconButton(
                                icon: const Icon(Icons.clear, size: 14, color: ConduitColors.textMuted),
                                onPressed: () {
                                  _searchController.clear();
                                  torrentsProv.setSearchQuery('');
                                },
                              )
                            : null,
                        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                      ),
                      onChanged: (val) => torrentsProv.setSearchQuery(val),
                    ),
                  ),
                ],
              ),
            ),

            const SizedBox(height: 10),

            // Status Filter Chips
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Row(
                children: [
                  _buildFilterChip('All', 'all', torrentsProv),
                  _buildFilterChip('Downloading (${stats.downloadingCount})', 'downloading', torrentsProv),
                  _buildFilterChip('Seeding (${stats.seedingCount})', 'seeding', torrentsProv),
                  _buildFilterChip('Paused (${stats.pausedCount})', 'paused', torrentsProv),
                  _buildFilterChip('Circuit Broken', 'circuit_broken', torrentsProv),
                  _buildFilterChip('Errors (${stats.errorCount})', 'error', torrentsProv),
                ],
              ),
            ),

            const SizedBox(height: 8),

            // Torrents List
            if (torrentsProv.isLoading && torrentsProv.rawTorrents.isEmpty)
              const Padding(
                padding: EdgeInsets.all(40),
                child: Center(child: CircularProgressIndicator(color: ConduitColors.brandLight)),
              )
            else if (filtered.isEmpty)
              Padding(
                padding: const EdgeInsets.all(40),
                child: Center(
                  child: Column(
                    children: [
                      const Icon(Icons.cloud_off_outlined, size: 40, color: ConduitColors.textMuted),
                      const SizedBox(height: 12),
                      Text(
                        _searchController.text.isNotEmpty
                            ? 'No swarms match "${_searchController.text}"'
                            : 'No fetchers in this status view',
                        style: const TextStyle(color: ConduitColors.textMuted, fontSize: 13),
                      ),
                    ],
                  ),
                ),
              )
            else
              ...filtered.map((t) => TorrentCard(
                    torrent: t,
                    onTap: () {
                      Navigator.push(
                        context,
                        MaterialPageRoute(
                          builder: (_) => TorrentDetailScreen(compoundId: t.compoundId),
                        ),
                      );
                    },
                  )),
          ],
        ),
      ),
    );
  }

  Widget _buildFilterChip(String label, String value, TorrentsProvider provider) {
    final isSelected = provider.selectedStatus == value;
    return Padding(
      padding: const EdgeInsets.only(right: 6),
      child: ChoiceChip(
        label: Text(label),
        selected: isSelected,
        onSelected: (_) => provider.setSelectedStatus(value),
        selectedColor: ConduitColors.brand.withValues(alpha: 0.2),
        backgroundColor: ConduitColors.surface,
        labelStyle: TextStyle(
          color: isSelected ? ConduitColors.brandLight : ConduitColors.textSecondary,
          fontSize: 11,
          fontWeight: isSelected ? FontWeight.bold : FontWeight.normal,
        ),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(
            color: isSelected ? ConduitColors.brand : ConduitColors.border,
          ),
        ),
      ),
    );
  }
}
