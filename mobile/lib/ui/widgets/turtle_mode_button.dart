import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../providers/telemetry_provider.dart';

class TurtleModeButton extends StatelessWidget {
  const TurtleModeButton({super.key});

  @override
  Widget build(BuildContext context) {
    final telemetry = context.watch<TelemetryProvider>();
    final isTurtle = telemetry.stats.altSpeedEnabled;

    return IconButton(
      tooltip: isTurtle ? 'Turtle Mode Active (Throttled)' : 'Enable Turtle Mode',
      onPressed: () {
        context.read<TelemetryProvider>().toggleTurtleMode();
      },
      icon: Container(
        padding: const EdgeInsets.all(6),
        decoration: BoxDecoration(
          color: isTurtle ? ConduitColors.warningBg : ConduitColors.surface,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(
            color: isTurtle ? ConduitColors.warning : ConduitColors.border,
            width: 1,
          ),
        ),
        child: Text(
          '🐢',
          style: TextStyle(
            fontSize: 16,
            shadows: isTurtle
                ? [
                    Shadow(
                      color: ConduitColors.warning.withValues(alpha: 0.8),
                      blurRadius: 8,
                    )
                  ]
                : null,
          ),
        ),
      ),
    );
  }
}
