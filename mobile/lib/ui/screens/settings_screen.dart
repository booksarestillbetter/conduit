import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../providers/auth_provider.dart';
import '../../providers/telemetry_provider.dart';
import 'qr_scanner_screen.dart';
import 'setup_wizard_screen.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final auth = context.watch<AuthProvider>();
    final telemetry = context.watch<TelemetryProvider>();

    return Scaffold(
      appBar: AppBar(
        title: const Text('Conduit Mobile Settings'),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          // Connection Info Card
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Row(
                    children: [
                      Icon(Icons.dns, size: 18, color: ConduitColors.brandLight),
                      SizedBox(width: 8),
                      Text(
                        'Connected Server',
                        style: TextStyle(
                          color: ConduitColors.textPrimary,
                          fontSize: 14,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  Text(
                    auth.serverUrl ?? 'Not connected',
                    style: const TextStyle(
                      color: ConduitColors.textSecondary,
                      fontSize: 13,
                      fontFamily: 'monospace',
                    ),
                  ),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      Container(
                        width: 8,
                        height: 8,
                        decoration: BoxDecoration(
                          color: telemetry.isConnected ? ConduitColors.success : ConduitColors.error,
                          shape: BoxShape.circle,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Text(
                        telemetry.isConnected ? 'WebSocket Stream Online' : 'WebSocket Disconnected',
                        style: TextStyle(
                          color: telemetry.isConnected ? ConduitColors.successLight : ConduitColors.errorLight,
                          fontSize: 11,
                        ),
                      ),
                    ],
                  ),
                  if (auth.username != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      'Authenticated as: ${auth.username}',
                      style: const TextStyle(color: ConduitColors.textMuted, fontSize: 12),
                    ),
                  ],
                ],
              ),
            ),
          ),

          const SizedBox(height: 16),

          // Re-run Setup Wizard Button
          Card(
            child: ListTile(
              leading: const Icon(Icons.auto_awesome, color: ConduitColors.brandLight),
              title: const Text('Setup Wizard', style: TextStyle(fontSize: 13.5, fontWeight: FontWeight.w600)),
              subtitle: const Text('Re-run onboarding and discovery walkthrough', style: TextStyle(fontSize: 11, color: ConduitColors.textMuted)),
              trailing: const Icon(Icons.chevron_right, color: ConduitColors.textMuted),
              onTap: () {
                Navigator.push(
                  context,
                  MaterialPageRoute(builder: (_) => const SetupWizardScreen(isModalRerun: true)),
                );
              },
            ),
          ),

          const SizedBox(height: 12),

          // Re-Pair QR Code Button
          Card(
            child: ListTile(
              leading: const Icon(Icons.qr_code_scanner, color: ConduitColors.brandLight),
              title: const Text('Re-Pair Device', style: TextStyle(fontSize: 13.5, fontWeight: FontWeight.w600)),
              subtitle: const Text('Scan a new QR code from Conduit Web', style: TextStyle(fontSize: 11, color: ConduitColors.textMuted)),
              trailing: const Icon(Icons.chevron_right, color: ConduitColors.textMuted),
              onTap: () async {
                final result = await Navigator.push<Map<String, String>>(
                  context,
                  MaterialPageRoute(builder: (_) => const QrScannerScreen()),
                );
                if (result != null && context.mounted) {
                  final ok = await context.read<AuthProvider>().pairWithQr(
                        serverUrl: result['server_url']!,
                        pairCode: result['pair_code']!,
                      );
                  if (ok && context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('Device paired successfully!')),
                    );
                  }
                }
              },
            ),
          ),

          const SizedBox(height: 12),

          // Disconnect Session Button
          Card(
            child: ListTile(
              leading: const Icon(Icons.logout, color: ConduitColors.errorLight),
              title: const Text('Disconnect from Conduit', style: TextStyle(fontSize: 13.5, color: ConduitColors.errorLight, fontWeight: FontWeight.w600)),
              subtitle: const Text('Clears local session token from device', style: TextStyle(fontSize: 11, color: ConduitColors.textMuted)),
              onTap: () {
                showDialog(
                  context: context,
                  builder: (ctx) => AlertDialog(
                    backgroundColor: ConduitColors.surface,
                    title: const Text('Disconnect Conduit?'),
                    content: const Text('You will need to scan a new QR code or enter credentials to reconnect.'),
                    actions: [
                      TextButton(
                        onPressed: () => Navigator.pop(ctx),
                        child: const Text('Cancel', style: TextStyle(color: ConduitColors.textMuted)),
                      ),
                      ElevatedButton(
                        style: ElevatedButton.styleFrom(backgroundColor: ConduitColors.error),
                        onPressed: () {
                          Navigator.pop(ctx);
                          context.read<AuthProvider>().logout();
                        },
                        child: const Text('Disconnect'),
                      ),
                    ],
                  ),
                );
              },
            ),
          ),

          const SizedBox(height: 32),

          // App Info
          const Center(
            child: Column(
              children: [
                Text('🐕', style: TextStyle(fontSize: 24)),
                SizedBox(height: 6),
                Text(
                  'Conduit Mobile v0.5.0',
                  style: TextStyle(
                    color: ConduitColors.textSecondary,
                    fontSize: 12,
                    fontWeight: FontWeight.bold,
                  ),
                ),
                SizedBox(height: 2),
                Text(
                  'Flutter Client for Conduit Media Automation Stack',
                  style: TextStyle(color: ConduitColors.textMuted, fontSize: 10.5),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
