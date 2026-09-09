import 'dart:math';
import 'package:flutter/material.dart';
import 'package:fl_chart/fl_chart.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/cluster_stats.dart';
import '../../core/utils/formatters.dart';

class BandwidthChart extends StatelessWidget {
  final List<BandwidthPoint> points;
  final double height;

  const BandwidthChart({
    super.key,
    required this.points,
    this.height = 140,
  });

  @override
  Widget build(BuildContext context) {
    if (points.isEmpty) {
      return Container(
        height: height,
        alignment: Alignment.center,
        child: const Text(
          'Waiting for live bandwidth telemetry...',
          style: TextStyle(color: ConduitColors.textMuted, fontSize: 12),
        ),
      );
    }

    final dlSpots = <FlSpot>[];
    final ulSpots = <FlSpot>[];

    var maxSpeed = 1024.0 * 1024.0; // Min 1 MB/s ceiling
    for (int i = 0; i < points.length; i++) {
      final p = points[i];
      dlSpots.add(FlSpot(i.toDouble(), p.downloadSpeed.toDouble()));
      ulSpots.add(FlSpot(i.toDouble(), p.uploadSpeed.toDouble()));
      maxSpeed = max(maxSpeed, max(p.downloadSpeed.toDouble(), p.uploadSpeed.toDouble()));
    }

    // Add 15% headroom
    final maxY = maxSpeed * 1.15;

    return SizedBox(
      height: height,
      child: LineChart(
        LineChartData(
          minX: 0,
          maxX: max((points.length - 1).toDouble(), 1.0),
          minY: 0,
          maxY: maxY,
          gridData: FlGridData(
            show: true,
            drawVerticalLine: false,
            horizontalInterval: maxY / 3,
            getDrawingHorizontalLine: (value) => const FlLine(
              color: ConduitColors.borderSubtle,
              strokeWidth: 1,
              dashArray: [4, 4],
            ),
          ),
          titlesData: FlTitlesData(
            leftTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
            topTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
            rightTitles: AxisTitles(
              sideTitles: SideTitles(
                showTitles: true,
                reservedSize: 45,
                interval: maxY / 3,
                getTitlesWidget: (value, meta) {
                  return Padding(
                    padding: const EdgeInsets.only(left: 4),
                    child: Text(
                      Formatters.formatBytes(value.toInt()),
                      style: const TextStyle(
                        color: ConduitColors.textMuted,
                        fontSize: 9,
                        fontFamily: 'monospace',
                      ),
                    ),
                  );
                },
              ),
            ),
            bottomTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
          ),
          borderData: FlBorderData(show: false),
          lineTouchData: LineTouchData(
            touchTooltipData: LineTouchTooltipData(
              getTooltipItems: (touchedSpots) {
                if (touchedSpots.isEmpty) return [];
                final spotIdx = touchedSpots.first.spotIndex;
                final point = spotIdx >= 0 && spotIdx < points.length ? points[spotIdx] : null;

                return touchedSpots.map((spot) {
                  final isDl = spot.barIndex == 0;
                  var text = '${isDl ? "DL: " : "UL: "}${Formatters.formatSpeed(spot.y.toInt())}';

                  // If this is the last spot line and we have top contributors, append them
                  if (spot.barIndex == 1 && point != null && point.topTorrents.isNotEmpty) {
                    final top = point.topTorrents.take(3).map((t) {
                      final speedStr = t.downloadSpeed > 0 ? "↓${Formatters.formatSpeed(t.downloadSpeed)}" : "↑${Formatters.formatSpeed(t.uploadSpeed)}";
                      return '\n• ${t.name.length > 18 ? "${t.name.substring(0, 16)}..." : t.name} ($speedStr)';
                    }).join();
                    text += '\nActive Swarms:$top';
                  }

                  return LineTooltipItem(
                    text,
                    TextStyle(
                      color: isDl ? ConduitColors.brandLight : ConduitColors.successLight,
                      fontWeight: FontWeight.bold,
                      fontSize: 11,
                    ),
                  );
                }).toList();
              },
            ),
          ),
          lineBarsData: [
            // Download Speed Series (Sky Blue)
            LineChartBarData(
              spots: dlSpots,
              isCurved: true,
              curveSmoothness: 0.2,
              color: ConduitColors.brandLight,
              barWidth: 2,
              isStrokeCapRound: true,
              dotData: const FlDotData(show: false),
              belowBarData: BarAreaData(
                show: true,
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [
                    ConduitColors.brand.withValues(alpha: 0.3),
                    ConduitColors.brand.withValues(alpha: 0.0),
                  ],
                ),
              ),
            ),
            // Upload Speed Series (Emerald Green)
            LineChartBarData(
              spots: ulSpots,
              isCurved: true,
              curveSmoothness: 0.2,
              color: ConduitColors.successLight,
              barWidth: 2,
              isStrokeCapRound: true,
              dotData: const FlDotData(show: false),
              belowBarData: BarAreaData(
                show: true,
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [
                    ConduitColors.success.withValues(alpha: 0.25),
                    ConduitColors.success.withValues(alpha: 0.0),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
