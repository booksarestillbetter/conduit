import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../core/models/node.dart';
import '../../core/network/api_client.dart';
import '../../core/utils/formatters.dart';
import '../../providers/auth_provider.dart';
import '../../providers/nodes_provider.dart';
import '../../providers/torrents_provider.dart';
import 'main_navigation_screen.dart';
import 'qr_scanner_screen.dart';

class SetupWizardScreen extends StatefulWidget {
  final bool isModalRerun;

  const SetupWizardScreen({super.key, this.isModalRerun = false});

  @override
  State<SetupWizardScreen> createState() => _SetupWizardScreenState();
}

class _SetupWizardScreenState extends State<SetupWizardScreen> {
  final PageController _pageController = PageController();
  int _currentStep = 0;
  final int _totalSteps = 5;

  // Step 1: Connect form controllers
  final _serverController = TextEditingController(text: 'https://conduit.example.com');
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  final _totpController = TextEditingController();
  bool _showManualForm = false;
  bool _isConnecting = false;
  String? _connectError;

  // Step 2: Discovered nodes
  List<TransmissionNode> _discoveredNodes = [];
  bool _isLoadingNodes = false;

  // Step 3: Discovered settings / Arr services
  Map<String, dynamic> _discoveredSettings = {};
  bool _isLoadingSettings = false;

  @override
  void initState() {
    super.initState();
    // If already authenticated (e.g. re-running wizard), prefetch nodes & settings
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final auth = context.read<AuthProvider>();
      if (auth.isAuthenticated) {
        _fetchDiscoveryData();
      }
    });
  }

  @override
  void dispose() {
    _pageController.dispose();
    _serverController.dispose();
    _usernameController.dispose();
    _passwordController.dispose();
    _totpController.dispose();
    super.dispose();
  }

  void _nextPage() {
    if (_currentStep < _totalSteps - 1) {
      _pageController.nextPage(
        duration: const Duration(milliseconds: 300),
        curve: Curves.easeInOut,
      );
    }
  }

  void _previousPage() {
    if (_currentStep > 0) {
      _pageController.previousPage(
        duration: const Duration(milliseconds: 300),
        curve: Curves.easeInOut,
      );
    }
  }

  Future<void> _fetchDiscoveryData() async {
    setState(() {
      _isLoadingNodes = true;
      _isLoadingSettings = true;
    });

    try {
      final nodes = await ApiClient.getNodes();
      if (mounted) setState(() => _discoveredNodes = nodes);
    } catch (_) {}

    try {
      final settings = await ApiClient.getSettings();
      if (mounted) setState(() => _discoveredSettings = settings);
    } catch (_) {}

    if (mounted) {
      setState(() {
        _isLoadingNodes = false;
        _isLoadingSettings = false;
      });
    }
  }

  Future<void> _startQrPairing() async {
    final result = await Navigator.push<Map<String, String>>(
      context,
      MaterialPageRoute(builder: (_) => const QrScannerScreen()),
    );

    if (result != null && mounted) {
      var serverUrl = result['server_url']!;
      final pairCode = result['pair_code']!;

      if (serverUrl.contains('0.0.0.0') || serverUrl.contains('127.0.0.1') || serverUrl.contains('localhost')) {
        serverUrl = _serverController.text.trim().isNotEmpty
            ? _serverController.text.trim()
            : 'https://conduit.example.com';
      }

      setState(() {
        _isConnecting = true;
        _connectError = null;
      });

      final auth = context.read<AuthProvider>();
      final success = await auth.pairWithQr(
        serverUrl: serverUrl,
        pairCode: pairCode,
      );

      if (mounted) {
        setState(() => _isConnecting = false);
        if (success) {
          await _fetchDiscoveryData();
          _nextPage();
        } else {
          setState(() => _connectError = auth.errorMessage ?? 'Pairing failed. Please check QR code.');
        }
      }
    }
  }

  Future<void> _submitManualLogin() async {
    final server = _serverController.text.trim();
    final user = _usernameController.text.trim();
    final pass = _passwordController.text;
    final totp = _totpController.text.trim();

    if (server.isEmpty || user.isEmpty || pass.isEmpty) {
      setState(() => _connectError = 'Please provide Server URL, Username, and Password');
      return;
    }

    setState(() {
      _isConnecting = true;
      _connectError = null;
    });

    final auth = context.read<AuthProvider>();
    final success = await auth.login(
      serverUrl: server,
      username: user,
      password: pass,
      totpCode: totp.isNotEmpty ? totp : null,
    );

    if (mounted) {
      setState(() => _isConnecting = false);
      if (success) {
        await _fetchDiscoveryData();
        _nextPage();
      } else {
        setState(() => _connectError = auth.errorMessage ?? 'Login failed. Check server address and credentials.');
      }
    }
  }

  Future<void> _finishSetup() async {
    final auth = context.read<AuthProvider>();
    await auth.completeSetup();

    // Hydrate providers
    if (mounted) {
      context.read<TorrentsProvider>().fetchTorrents();
      context.read<NodesProvider>().fetchAll();

      if (widget.isModalRerun) {
        Navigator.pop(context);
      } else {
        Navigator.pushReplacement(
          context,
          MaterialPageRoute(builder: (_) => const MainNavigationScreen()),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final auth = context.watch<AuthProvider>();

    return Scaffold(
      backgroundColor: ConduitColors.surface,
      appBar: AppBar(
        backgroundColor: Colors.transparent,
        elevation: 0,
        leading: _currentStep > 0
            ? IconButton(
                icon: const Icon(Icons.arrow_back),
                onPressed: _previousPage,
              )
            : (widget.isModalRerun
                ? IconButton(
                    icon: const Icon(Icons.close),
                    onPressed: () => Navigator.pop(context),
                  )
                : null),
        title: Text(
          'Setup Wizard (${_currentStep + 1}/$_totalSteps)',
          style: const TextStyle(fontSize: 14, color: ConduitColors.textMuted),
        ),
        actions: [
          if (_currentStep > 0 && _currentStep < _totalSteps - 1 && auth.isAuthenticated)
            TextButton(
              onPressed: _nextPage,
              child: const Text('Skip', style: TextStyle(color: ConduitColors.brandLight)),
            ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            // Step Progress Bar
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
              child: Row(
                children: List.generate(_totalSteps, (index) {
                  final isDone = index < _currentStep;
                  final isCurrent = index == _currentStep;
                  return Expanded(
                    child: Container(
                      margin: const EdgeInsets.symmetric(horizontal: 2),
                      height: 4,
                      decoration: BoxDecoration(
                        color: isDone || isCurrent ? ConduitColors.brandLight : ConduitColors.border,
                        borderRadius: BorderRadius.circular(2),
                      ),
                    ),
                  );
                }),
              ),
            ),

            Expanded(
              child: PageView(
                controller: _pageController,
                physics: const NeverScrollableScrollPhysics(),
                onPageChanged: (idx) => setState(() => _currentStep = idx),
                children: [
                  _buildStep0Welcome(),
                  _buildStep1Connect(auth),
                  _buildStep2Nodes(),
                  _buildStep3Pipeline(),
                  _buildStep4Finished(auth),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  // --- Step 0: Welcome ---
  Widget _buildStep0Welcome() {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 28, vertical: 16),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Spacer(),
          Container(
            padding: const EdgeInsets.all(24),
            decoration: BoxDecoration(
              color: ConduitColors.card,
              shape: BoxShape.circle,
              border: Border.all(color: ConduitColors.brandLight.withValues(alpha: 0.3), width: 2),
            ),
            child: const Text('🐕', style: TextStyle(fontSize: 56)),
          ),
          const SizedBox(height: 24),
          const Text(
            'Welcome to Conduit',
            style: TextStyle(
              fontSize: 26,
              fontWeight: FontWeight.w900,
              color: ConduitColors.textPrimary,
              letterSpacing: -0.5,
            ),
          ),
          const SizedBox(height: 10),
          const Text(
            'Your intelligent torrent swarm controller & media lifecycle automation stack.',
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 14,
              color: ConduitColors.textSecondary,
              height: 1.4,
            ),
          ),
          const SizedBox(height: 32),

          // Highlights
          _buildHighlightTile(
            icon: Icons.hub_outlined,
            title: 'Unified Swarm Control',
            subtitle: 'Monitor and orchestrate multiple Transmission, qBittorrent & Deluge nodes.',
          ),
          const SizedBox(height: 12),
          _buildHighlightTile(
            icon: Icons.auto_awesome_outlined,
            title: 'Living Media Pipeline',
            subtitle: 'Automated Sonarr, Radarr, and Lidarr webhook & grab tracking.',
          ),
          const SizedBox(height: 12),
          _buildHighlightTile(
            icon: Icons.security_outlined,
            title: 'Tracker Circuit Breakers',
            subtitle: 'Proactively trips failing tracker swarms to protect your ratio.',
          ),

          const Spacer(),
          SizedBox(
            width: double.infinity,
            height: 50,
            child: ElevatedButton(
              onPressed: _nextPage,
              style: ElevatedButton.styleFrom(
                backgroundColor: ConduitColors.brandLight,
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
              ),
              child: const Text('Get Started', style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }

  Widget _buildHighlightTile({required IconData icon, required String title, required String subtitle}) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: ConduitColors.card,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: ConduitColors.border),
      ),
      child: Row(
        children: [
          Container(
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              color: ConduitColors.brandLight.withValues(alpha: 0.15),
              borderRadius: BorderRadius.circular(8),
            ),
            child: Icon(icon, color: ConduitColors.brandLight, size: 20),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary)),
                Text(subtitle, style: const TextStyle(fontSize: 11, color: ConduitColors.textMuted)),
              ],
            ),
          ),
        ],
      ),
    );
  }

  // --- Step 1: Connect Server ---
  Widget _buildStep1Connect(AuthProvider auth) {
    if (auth.isAuthenticated) {
      return Padding(
        padding: const EdgeInsets.all(28),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Spacer(),
            const Icon(Icons.check_circle, size: 64, color: ConduitColors.success),
            const SizedBox(height: 16),
            const Text(
              'Server Connected',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
            ),
            const SizedBox(height: 8),
            Text(
              'Connected to ${auth.serverUrl}\nas ${auth.username ?? "user"}',
              textAlign: TextAlign.center,
              style: const TextStyle(color: ConduitColors.textSecondary, fontSize: 13),
            ),
            const Spacer(),
            SizedBox(
              width: double.infinity,
              height: 48,
              child: ElevatedButton(
                onPressed: () {
                  _fetchDiscoveryData();
                  _nextPage();
                },
                child: const Text('Continue to Nodes Setup'),
              ),
            ),
            const SizedBox(height: 12),
            TextButton(
              onPressed: () => auth.logout(),
              child: const Text('Change Server / Re-authenticate', style: TextStyle(color: ConduitColors.textMuted)),
            ),
          ],
        ),
      );
    }

    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        const Text(
          'Connect to Conduit',
          style: TextStyle(fontSize: 22, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
        ),
        const SizedBox(height: 6),
        const Text(
          'Pair with your existing Conduit server instance using a fast QR scan from the Web UI, or enter credentials manually.',
          style: TextStyle(color: ConduitColors.textSecondary, fontSize: 13, height: 1.4),
        ),
        const SizedBox(height: 24),

        // QR Scan Button (Primary)
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: ConduitColors.card,
            borderRadius: BorderRadius.circular(16),
            border: Border.all(color: ConduitColors.brandLight.withValues(alpha: 0.3)),
          ),
          child: Column(
            children: [
              const Icon(Icons.qr_code_scanner, size: 48, color: ConduitColors.brandLight),
              const SizedBox(height: 12),
              const Text(
                'Instant QR Pairing',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
              ),
              const SizedBox(height: 6),
              const Text(
                'Open your Conduit Web UI > Click Profile > Scan the on-screen QR code.',
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 12, color: ConduitColors.textMuted),
              ),
              const SizedBox(height: 16),
              SizedBox(
                width: double.infinity,
                child: ElevatedButton.icon(
                  onPressed: _isConnecting ? null : _startQrPairing,
                  icon: const Icon(Icons.camera_alt_outlined),
                  label: const Text('Scan QR Code Now'),
                  style: ElevatedButton.styleFrom(
                    backgroundColor: ConduitColors.brandLight,
                    padding: const EdgeInsets.symmetric(vertical: 12),
                  ),
                ),
              ),
            ],
          ),
        ),

        const SizedBox(height: 20),

        // Manual Login Accordion / Toggle
        TextButton.icon(
          onPressed: () => setState(() => _showManualForm = !_showManualForm),
          icon: Icon(_showManualForm ? Icons.keyboard_arrow_up : Icons.keyboard_arrow_down),
          label: Text(
            _showManualForm ? 'Hide Manual Server Connection' : 'Or connect manually with Server URL & Password',
            style: const TextStyle(color: ConduitColors.textMuted, fontSize: 12),
          ),
        ),

        if (_showManualForm) ...[
          const SizedBox(height: 12),
          TextField(
            controller: _serverController,
            decoration: const InputDecoration(
              labelText: 'Server URL',
              hintText: 'https://conduit.example.com',
              prefixIcon: Icon(Icons.dns_outlined, size: 18),
            ),
          ),
          const SizedBox(height: 10),
          TextField(
            controller: _usernameController,
            decoration: const InputDecoration(
              labelText: 'Username',
              prefixIcon: Icon(Icons.person_outline, size: 18),
            ),
          ),
          const SizedBox(height: 10),
          TextField(
            controller: _passwordController,
            obscureText: true,
            decoration: const InputDecoration(
              labelText: 'Password',
              prefixIcon: Icon(Icons.lock_outline, size: 18),
            ),
          ),
          const SizedBox(height: 10),
          TextField(
            controller: _totpController,
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(
              labelText: '2FA TOTP Code (optional)',
              prefixIcon: Icon(Icons.pin_outlined, size: 18),
            ),
          ),
          const SizedBox(height: 16),
          SizedBox(
            width: double.infinity,
            child: ElevatedButton(
              onPressed: _isConnecting ? null : _submitManualLogin,
              child: _isConnecting
                  ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                  : const Text('Connect & Authenticate'),
            ),
          ),
        ],

        if (_connectError != null) ...[
          const SizedBox(height: 16),
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: ConduitColors.error.withValues(alpha: 0.15),
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: ConduitColors.error.withValues(alpha: 0.4)),
            ),
            child: Row(
              children: [
                const Icon(Icons.error_outline, size: 18, color: ConduitColors.error),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(_connectError!, style: const TextStyle(color: ConduitColors.error, fontSize: 12)),
                ),
              ],
            ),
          ),
        ],
      ],
    );
  }

  // --- Step 2: Retriever Daemons (Skippable) ---
  Widget _buildStep2Nodes() {
    return Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Retriever Daemons',
            style: TextStyle(fontSize: 22, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
          ),
          const SizedBox(height: 6),
          const Text(
            'Conduit pools multiple retriever daemons (Transmission, qBittorrent, Deluge) into a unified cluster. Review your discovered nodes below.',
            style: TextStyle(color: ConduitColors.textSecondary, fontSize: 13, height: 1.4),
          ),
          const SizedBox(height: 16),

          Expanded(
            child: _isLoadingNodes
                ? const Center(child: CircularProgressIndicator(color: ConduitColors.brandLight))
                : _discoveredNodes.isEmpty
                    ? Center(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            const Icon(Icons.dns_outlined, size: 48, color: ConduitColors.textMuted),
                            const SizedBox(height: 12),
                            const Text('No Retriever nodes discovered yet', style: TextStyle(color: ConduitColors.textSecondary)),
                            const SizedBox(height: 6),
                            const Padding(
                              padding: EdgeInsets.symmetric(horizontal: 24),
                              child: Text(
                                'You can add Transmission, qBittorrent, or Deluge nodes anytime via the Web UI Settings panel.',
                                textAlign: TextAlign.center,
                                style: TextStyle(color: ConduitColors.textMuted, fontSize: 12),
                              ),
                            ),
                            const SizedBox(height: 16),
                            ElevatedButton.icon(
                              onPressed: _fetchDiscoveryData,
                              icon: const Icon(Icons.refresh, size: 16),
                              label: const Text('Re-check Nodes'),
                            ),
                          ],
                        ),
                      )
                    : ListView.builder(
                        itemCount: _discoveredNodes.length,
                        itemBuilder: (context, index) {
                          final n = _discoveredNodes[index];
                          return Card(
                            margin: const EdgeInsets.only(bottom: 10),
                            color: ConduitColors.card,
                            child: ListTile(
                              leading: Container(
                                width: 10,
                                height: 10,
                                decoration: BoxDecoration(
                                  color: n.online ? ConduitColors.success : ConduitColors.error,
                                  shape: BoxShape.circle,
                                ),
                              ),
                              title: Row(
                                children: [
                                  Text(n.name, style: const TextStyle(fontWeight: FontWeight.bold, color: ConduitColors.textPrimary)),
                                  const SizedBox(width: 8),
                                  Container(
                                    padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1.5),
                                    decoration: BoxDecoration(
                                      color: ConduitColors.brandLight.withValues(alpha: 0.15),
                                      borderRadius: BorderRadius.circular(4),
                                    ),
                                    child: Text(
                                      n.clientDisplayName,
                                      style: const TextStyle(color: ConduitColors.brandLight, fontSize: 9.5, fontWeight: FontWeight.bold),
                                    ),
                                  ),
                                ],
                              ),
                              subtitle: Text(
                                'Free: ${Formatters.formatBytes(n.freeSpaceBytes)} • Swarms: ${n.torrentCount}',
                                style: const TextStyle(fontSize: 11, color: ConduitColors.textMuted),
                              ),
                              trailing: Text(
                                '${n.rpcLatencyMs}ms',
                                style: const TextStyle(fontFamily: 'monospace', fontSize: 11, color: ConduitColors.textMuted),
                              ),
                            ),
                          );
                        },
                      ),
          ),

          const SizedBox(height: 16),
          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: _nextPage,
                  child: const Text('Skip for Now'),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: ElevatedButton(
                  onPressed: _nextPage,
                  child: const Text('Continue'),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  // --- Step 3: Media Pipeline & Arr Stack (Skippable) ---
  Widget _buildStep3Pipeline() {
    final hasSonarr = _discoveredSettings['sonarr']?['enabled'] == true;
    final hasRadarr = _discoveredSettings['radarr']?['enabled'] == true;
    final hasLidarr = _discoveredSettings['lidarr']?['enabled'] == true;

    return Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Media Pipeline & Arr Stack',
            style: TextStyle(fontSize: 22, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
          ),
          const SizedBox(height: 6),
          const Text(
            'Conduit listens for Sonarr, Radarr, and Lidarr media grabs, tracks download progression, and archives deletion events.',
            style: TextStyle(color: ConduitColors.textSecondary, fontSize: 13, height: 1.4),
          ),
          const SizedBox(height: 20),

          Expanded(
            child: _isLoadingSettings
                ? const Center(child: CircularProgressIndicator(color: ConduitColors.brandLight))
                : ListView(
                    children: [
                      _buildServiceCard('Sonarr (TV)', hasSonarr, Icons.tv, ConduitColors.brandSecondary),
                      const SizedBox(height: 10),
                      _buildServiceCard('Radarr (Movies)', hasRadarr, Icons.movie, ConduitColors.brandLight),
                      const SizedBox(height: 10),
                      _buildServiceCard('Lidarr (Music)', hasLidarr, Icons.music_note, ConduitColors.purpleLight),
                      const SizedBox(height: 20),
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: ConduitColors.card,
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: ConduitColors.border),
                  ),
                  child: const Row(
                    children: [
                      Icon(Icons.info_outline, color: ConduitColors.brandLight, size: 20),
                      SizedBox(width: 12),
                      Expanded(
                        child: Text(
                          'Webhooks configured in your Arr apps will feed into Conduit\'s Living Pipeline & Ghost Archive automatically.',
                          style: TextStyle(fontSize: 12, color: ConduitColors.textSecondary),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),

          const SizedBox(height: 16),
          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: _nextPage,
                  child: const Text('Skip for Now'),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: ElevatedButton(
                  onPressed: _nextPage,
                  child: const Text('Continue'),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildServiceCard(String title, bool enabled, IconData icon, Color color) {
    return Card(
      color: ConduitColors.card,
      child: ListTile(
        leading: Container(
          padding: const EdgeInsets.all(8),
          decoration: BoxDecoration(
            color: color.withValues(alpha: 0.15),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Icon(icon, color: color, size: 20),
        ),
        title: Text(title, style: const TextStyle(fontWeight: FontWeight.bold, color: ConduitColors.textPrimary, fontSize: 14)),
        trailing: Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
          decoration: BoxDecoration(
            color: enabled ? ConduitColors.successBg : ConduitColors.surface,
            borderRadius: BorderRadius.circular(6),
            border: Border.all(color: enabled ? ConduitColors.success : ConduitColors.border),
          ),
          child: Text(
            enabled ? 'CONNECTED' : 'STANDBY',
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.bold,
              color: enabled ? ConduitColors.successLight : ConduitColors.textMuted,
            ),
          ),
        ),
      ),
    );
  }

  // --- Step 4: Finished ---
  Widget _buildStep4Finished(AuthProvider auth) {
    return Padding(
      padding: const EdgeInsets.all(28),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Spacer(),
          Container(
            padding: const EdgeInsets.all(20),
            decoration: BoxDecoration(
              color: ConduitColors.success.withValues(alpha: 0.15),
              shape: BoxShape.circle,
            ),
            child: const Icon(Icons.rocket_launch, size: 56, color: ConduitColors.brandLight),
          ),
          const SizedBox(height: 20),
          const Text(
            'You\'re All Set!',
            style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold, color: ConduitColors.textPrimary),
          ),
          const SizedBox(height: 8),
          const Text(
            'Conduit Mobile is configured and ready to monitor your cluster swarms.',
            textAlign: TextAlign.center,
            style: TextStyle(color: ConduitColors.textSecondary, fontSize: 13),
          ),
          const SizedBox(height: 24),

          // Summary Card
          Container(
            padding: const EdgeInsets.all(16),
            decoration: BoxDecoration(
              color: ConduitColors.card,
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: ConduitColors.border),
            ),
            child: Column(
              children: [
                _buildSummaryRow('Server', auth.serverUrl ?? 'Connected'),
                _buildSummaryRow('User', auth.username ?? 'Default'),
                _buildSummaryRow('Nodes', '${_discoveredNodes.length} Detected'),
              ],
            ),
          ),

          const Spacer(),
          SizedBox(
            width: double.infinity,
            height: 50,
            child: ElevatedButton(
              onPressed: _finishSetup,
              style: ElevatedButton.styleFrom(
                backgroundColor: ConduitColors.brandLight,
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
              ),
              child: const Text(
                'Launch Conduit',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }

  Widget _buildSummaryRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(color: ConduitColors.textMuted, fontSize: 12)),
          Text(value, style: const TextStyle(color: ConduitColors.textPrimary, fontSize: 12, fontWeight: FontWeight.bold)),
        ],
      ),
    );
  }
}
