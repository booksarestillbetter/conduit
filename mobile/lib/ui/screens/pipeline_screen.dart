import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/media_grab.dart';
import '../../core/models/zone.dart';
import '../../core/utils/formatters.dart';
import '../../providers/nodes_provider.dart';
import '../widgets/media_detail_bottom_sheet.dart';

class PipelineScreen extends StatefulWidget {
  const PipelineScreen({super.key});

  @override
  State<PipelineScreen> createState() => _PipelineScreenState();
}

class _PipelineScreenState extends State<PipelineScreen> {
  int _selectedView = 0; // 0: Active Pipeline, 1: Ghost Archive
  String _archiveFilter = 'all'; // all, imported, fetched, replaced, deleted
  final TextEditingController _searchController = TextEditingController();

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  void _openDetail(BuildContext context, MediaGrab grab) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      builder: (_) => MediaDetailBottomSheet(
        item: grab,
        onRefresh: () => context.read<NodesProvider>().fetchAll(silent: true),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final nodesProv = context.watch<NodesProvider>();
    final activeGrabs = nodesProv.mediaGrabs;
    final ghostGrabs = nodesProv.ghostGrabs;
    final zones = nodesProv.zones;
    final activeZone = nodesProv.activeZone;

    List<MediaGrab> currentItems = _selectedView == 0 ? activeGrabs : ghostGrabs;

    // Apply archive filter
    if (_selectedView == 1 && _archiveFilter != 'all') {
      currentItems = currentItems.where((g) {
        if (_archiveFilter == 'imported') return g.isImported;
        if (_archiveFilter == 'deleted') return g.isDeleted;
        if (_archiveFilter == 'replaced') return g.isReplaced;
        if (_archiveFilter == 'fetched') return g.status == 'fetched' || g.status == 'grabbed';
        return true;
      }).toList();
    }

    // Apply search filter
    final query = _searchController.text.trim().toLowerCase();
    if (query.isNotEmpty) {
      currentItems = currentItems.where((g) {
        return g.title.toLowerCase().contains(query) ||
            g.releaseTitle.toLowerCase().contains(query) ||
            (g.indexer?.toLowerCase().contains(query) ?? false);
      }).toList();
    }

    return Scaffold(
      appBar: AppBar(
        title: Row(
          children: [
            Text(_selectedView == 0 ? '🐾' : '👻', style: const TextStyle(fontSize: 18)),
            const SizedBox(width: 8),
            Text(_selectedView == 0 ? 'Living Pipeline' : 'Ghost Archive'),
          ],
        ),
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(56),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Row(
              children: [
                Expanded(
                  child: SegmentedButton<int>(
                    segments: const [
                      ButtonSegment(value: 0, label: Text('Pipeline', style: TextStyle(fontSize: 12))),
                      ButtonSegment(value: 1, label: Text('Ghost Archive', style: TextStyle(fontSize: 12))),
                    ],
                    selected: {_selectedView},
                    onSelectionChanged: (val) {
                      setState(() => _selectedView = val.first);
                    },
                    style: ButtonStyle(
                      visualDensity: VisualDensity.compact,
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    ),
                  ),
                ),
                if (zones.isNotEmpty) ...[
                  const SizedBox(width: 8),
                  PopupMenuButton<String?>(
                    initialValue: activeZone,
                    tooltip: 'Filter by Zone',
                    icon: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
                      decoration: BoxDecoration(
                        color: ConduitColors.card,
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
                ],
              ],
            ),
          ),
        ),
      ),
      body: Column(
        children: [
          // Filter / Search bar
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _searchController,
                    onChanged: (_) => setState(() {}),
                    decoration: InputDecoration(
                      hintText: _selectedView == 0 ? 'Search active pipeline...' : 'Search archive by title, indexer...',
                      prefixIcon: const Icon(Icons.search, size: 18),
                      isDense: true,
                      contentPadding: const EdgeInsets.symmetric(vertical: 8, horizontal: 12),
                      suffixIcon: _searchController.text.isNotEmpty
                          ? IconButton(
                              icon: const Icon(Icons.clear, size: 16),
                              onPressed: () {
                                _searchController.clear();
                                setState(() {});
                              },
                            )
                          : null,
                    ),
                  ),
                ),
              ],
            ),
          ),

          // Archive Status Filter Chips
          if (_selectedView == 1)
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
              child: Row(
                children: [
                  _filterChip('all', 'All Grabs'),
                  _filterChip('imported', 'Imported'),
                  _filterChip('fetched', 'Fetched'),
                  _filterChip('replaced', 'Replaced'),
                  _filterChip('deleted', 'Deleted'),
                ],
              ),
            ),

          Expanded(
            child: RefreshIndicator(
              color: ConduitColors.brandLight,
              backgroundColor: ConduitColors.surface,
              onRefresh: () => nodesProv.fetchAll(),
              child: currentItems.isEmpty
                  ? Center(
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          Icon(
                            _selectedView == 0 ? Icons.hub_outlined : Icons.inventory_2_outlined,
                            size: 48,
                            color: ConduitColors.textMuted,
                          ),
                          const SizedBox(height: 12),
                          Text(
                            _selectedView == 0 ? 'No active pipeline grabs found' : 'No archived grabs match your filter',
                            style: const TextStyle(color: ConduitColors.textSecondary, fontSize: 14),
                          ),
                          const SizedBox(height: 4),
                          const Text(
                            'Grabs from Sonarr, Radarr, and Lidarr appear here automatically.',
                            style: TextStyle(color: ConduitColors.textMuted, fontSize: 11),
                          ),
                        ],
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                      itemCount: currentItems.length,
                      itemBuilder: (context, index) {
                        final g = currentItems[index];
                        return _buildGrabCard(context, g);
                      },
                    ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _filterChip(String key, String label) {
    final isSelected = _archiveFilter == key;
    return Padding(
      padding: const EdgeInsets.only(right: 6),
      child: ChoiceChip(
        label: Text(label, style: TextStyle(fontSize: 11, color: isSelected ? Colors.white : ConduitColors.textMuted)),
        selected: isSelected,
        selectedColor: ConduitColors.brandLight.withValues(alpha: 0.3),
        backgroundColor: ConduitColors.card,
        onSelected: (_) => setState(() => _archiveFilter = key),
        visualDensity: VisualDensity.compact,
      ),
    );
  }

  Widget _buildGrabCard(BuildContext context, MediaGrab grab) {
    IconData typeIcon = Icons.movie;
    Color typeColor = ConduitColors.brandLight;
    if (grab.isSeries) {
      typeIcon = Icons.tv;
      typeColor = ConduitColors.brandSecondary;
    } else if (grab.isMusic) {
      typeIcon = Icons.music_note;
      typeColor = ConduitColors.purpleLight;
    }

    Color statusColor = ConduitColors.brandLight;
    if (grab.isImported) statusColor = ConduitColors.success;
    if (grab.isDeleted) statusColor = ConduitColors.error;
    if (grab.isReplaced) statusColor = Colors.amber;

    return Card(
      margin: const EdgeInsets.only(bottom: 10),
      color: ConduitColors.card,
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: () => _openDetail(context, grab),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Poster or Icon
              if (grab.posterUrl != null && grab.posterUrl!.isNotEmpty)
                ClipRRect(
                  borderRadius: BorderRadius.circular(8),
                  child: Image.network(
                    grab.posterUrl!,
                    width: 48,
                    height: 72,
                    fit: BoxFit.cover,
                    errorBuilder: (ctx, err, stack) => _cardFallbackIcon(typeIcon, typeColor),
                  ),
                )
              else
                _cardFallbackIcon(typeIcon, typeColor),
              const SizedBox(width: 12),

              // Title & Info
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      grab.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.bold,
                        color: ConduitColors.textPrimary,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      grab.releaseTitle,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 11,
                        color: ConduitColors.textMuted,
                        fontFamily: 'monospace',
                      ),
                    ),
                    const SizedBox(height: 8),
                    Row(
                      children: [
                        _statusBadge(grab.status.toUpperCase(), statusColor),
                        const SizedBox(width: 6),
                        if (grab.quality != null)
                          _statusBadge(grab.quality!, ConduitColors.purpleLight),
                        const SizedBox(width: 6),
                        if (grab.indexer != null)
                          _statusBadge(grab.indexer!, Colors.amber),
                        const Spacer(),
                        Text(
                          Formatters.formatDate(grab.createdAt),
                          style: const TextStyle(fontSize: 10, color: ConduitColors.textMuted),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _cardFallbackIcon(IconData icon, Color color) {
    return Container(
      width: 48,
      height: 72,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Center(child: Icon(icon, size: 24, color: color)),
    );
  }

  Widget _statusBadge(String label, Color color) {
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
