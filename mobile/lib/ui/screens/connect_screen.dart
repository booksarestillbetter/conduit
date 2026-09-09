import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../providers/auth_provider.dart';
import 'qr_scanner_screen.dart';

class ConnectScreen extends StatefulWidget {
  const ConnectScreen({super.key});

  @override
  State<ConnectScreen> createState() => _ConnectScreenState();
}

class _ConnectScreenState extends State<ConnectScreen> {
  final _serverController = TextEditingController(text: 'https://home.example.com');
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  final _totpController = TextEditingController();
  bool _showManualForm = false;

  @override
  void dispose() {
    _serverController.dispose();
    _usernameController.dispose();
    _passwordController.dispose();
    _totpController.dispose();
    super.dispose();
  }

  Future<void> _startQrPairing() async {
    final result = await Navigator.push<Map<String, String>>(
      context,
      MaterialPageRoute(builder: (_) => const QrScannerScreen()),
    );

    if (result != null && mounted) {
      var serverUrl = result['server_url']!;
      final pairCode = result['pair_code']!;

      // If scanned URL contains 0.0.0.0 or localhost, resolve with real server host
      if (serverUrl.contains('0.0.0.0') || serverUrl.contains('127.0.0.1') || serverUrl.contains('localhost')) {
        final overrideController = TextEditingController(
          text: _serverController.text.trim().isNotEmpty
              ? _serverController.text.trim()
              : 'https://conduit.example.com',
        );

        final confirmedUrl = await showDialog<String>(
          context: context,
          builder: (ctx) => AlertDialog(
            backgroundColor: ConduitColors.surface,
            title: const Text('Confirm Server Address', style: TextStyle(color: ConduitColors.textPrimary)),
            content: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'The scanned QR code contained a server loopback address (localhost/127.0.0.1). Please verify your reachable Conduit server URL below:',
                  style: TextStyle(color: ConduitColors.textSecondary, fontSize: 13),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: overrideController,
                  decoration: const InputDecoration(
                    labelText: 'Conduit Server URL',
                    hintText: 'https://conduit.example.com',
                  ),
                ),
              ],
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(ctx, null),
                child: const Text('Cancel', style: TextStyle(color: ConduitColors.textMuted)),
              ),
              ElevatedButton(
                onPressed: () => Navigator.pop(ctx, overrideController.text.trim()),
                child: const Text('Connect'),
              ),
            ],
          ),
        );

        if (confirmedUrl == null || confirmedUrl.isEmpty) return;
        serverUrl = confirmedUrl;
      }

      if (!mounted) return;
      final auth = context.read<AuthProvider>();
      final success = await auth.pairWithQr(
        serverUrl: serverUrl,
        pairCode: pairCode,
      );

      if (!success && mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(auth.errorMessage ?? 'Pairing failed. Please check QR code or server URL.'),
            backgroundColor: ConduitColors.error,
          ),
        );
      }
    }
  }

  Future<void> _submitManualLogin() async {
    final server = _serverController.text.trim();
    final user = _usernameController.text.trim();
    final pass = _passwordController.text;
    final totp = _totpController.text.trim();

    if (server.isEmpty || user.isEmpty || pass.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Please enter server URL, username, and password'),
          backgroundColor: ConduitColors.warning,
        ),
      );
      return;
    }

    final auth = context.read<AuthProvider>();
    final ok = await auth.login(
      serverUrl: server,
      username: user,
      password: pass,
      totpCode: totp.isNotEmpty ? totp : null,
    );

    if (!ok && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(auth.errorMessage ?? 'Login failed'),
          backgroundColor: ConduitColors.error,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final auth = context.watch<AuthProvider>();
    final isLoading = auth.status == AuthStatus.authenticating;

    return Scaffold(
      body: SafeArea(
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 32),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // Brand Header
                Center(
                  child: Container(
                    width: 72,
                    height: 72,
                    decoration: BoxDecoration(
                      color: ConduitColors.brand.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(20),
                      border: Border.all(color: ConduitColors.brand.withValues(alpha: 0.3)),
                    ),
                    alignment: Alignment.center,
                    child: const Text('🐕', style: TextStyle(fontSize: 36)),
                  ),
                ),
                const SizedBox(height: 16),
                const Text(
                  'Conduit Mobile',
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 26,
                    fontWeight: FontWeight.w800,
                    letterSpacing: -0.5,
                    color: ConduitColors.textPrimary,
                  ),
                ),
                const SizedBox(height: 6),
                const Text(
                  'Unified Transmission Commander & Media Automation',
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 13,
                    color: ConduitColors.textSecondary,
                  ),
                ),
                const SizedBox(height: 36),

                // Primary QR Scan Button
                ElevatedButton.icon(
                  onPressed: isLoading ? null : _startQrPairing,
                  icon: const Icon(Icons.qr_code_scanner, size: 20),
                  label: Text(isLoading ? 'Authenticating...' : 'Scan QR Code to Connect'),
                  style: ElevatedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(vertical: 16),
                    backgroundColor: ConduitColors.brand,
                  ),
                ),

                const SizedBox(height: 12),

                // Explanation
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: ConduitColors.surface,
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: ConduitColors.border),
                  ),
                  child: const Row(
                    children: [
                      Icon(Icons.info_outline, size: 16, color: ConduitColors.brandLight),
                      SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          'Open Conduit Web on your computer, click your profile in the top-right, and choose "Pair Mobile App" to scan.',
                          style: TextStyle(color: ConduitColors.textSecondary, fontSize: 11.5),
                        ),
                      ),
                    ],
                  ),
                ),

                const SizedBox(height: 24),

                // Manual Credentials Divider / Toggle
                Row(
                  children: [
                    const Expanded(child: Divider()),
                    Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 12),
                      child: TextButton(
                        onPressed: () {
                          setState(() {
                            _showManualForm = !_showManualForm;
                          });
                        },
                        child: Text(
                          _showManualForm ? 'Hide Manual Login' : 'Or Sign In Manually',
                          style: const TextStyle(fontSize: 12, color: ConduitColors.brandLight),
                        ),
                      ),
                    ),
                    const Expanded(child: Divider()),
                  ],
                ),

                if (_showManualForm) ...[
                  const SizedBox(height: 12),
                  TextField(
                    controller: _serverController,
                    decoration: const InputDecoration(
                      labelText: 'Conduit Server URL',
                      hintText: 'https://home.example.com:4242',
                      prefixIcon: Icon(Icons.dns, size: 18, color: ConduitColors.textMuted),
                    ),
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: _usernameController,
                    decoration: const InputDecoration(
                      labelText: 'Username',
                      prefixIcon: Icon(Icons.person, size: 18, color: ConduitColors.textMuted),
                    ),
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: _passwordController,
                    obscureText: true,
                    decoration: const InputDecoration(
                      labelText: 'Password',
                      prefixIcon: Icon(Icons.lock, size: 18, color: ConduitColors.textMuted),
                    ),
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: _totpController,
                    keyboardType: TextInputType.number,
                    decoration: const InputDecoration(
                      labelText: '2FA Passcode (if enabled)',
                      prefixIcon: Icon(Icons.security, size: 18, color: ConduitColors.textMuted),
                    ),
                  ),
                  const SizedBox(height: 16),
                  ElevatedButton(
                    onPressed: isLoading ? null : _submitManualLogin,
                    style: ElevatedButton.styleFrom(
                      backgroundColor: ConduitColors.surface,
                      side: const BorderSide(color: ConduitColors.brand),
                    ),
                    child: Text(
                      isLoading ? 'Signing In...' : 'Sign In',
                      style: const TextStyle(color: ConduitColors.brandLight),
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
