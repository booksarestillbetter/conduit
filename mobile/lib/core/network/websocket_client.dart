import 'dart:async';
import 'dart:convert';
import 'package:web_socket_channel/web_socket_channel.dart';
import '../storage/session_manager.dart';
import 'api_client.dart';

class ConduitWebSocketClient {
  WebSocketChannel? _channel;
  final StreamController<Map<String, dynamic>> _streamController =
      StreamController<Map<String, dynamic>>.broadcast();
  bool _isConnecting = false;
  bool _isConnected = false;
  Timer? _reconnectTimer;

  Stream<Map<String, dynamic>> get stream => _streamController.stream;
  bool get isConnected => _isConnected;

  Future<void> connect() async {
    if (_isConnecting || _isConnected) return;
    _isConnecting = true;

    try {
      final serverUrl = await SessionManager.getServerUrl();
      if (serverUrl == null) {
        _isConnecting = false;
        return;
      }

      // 1. Issue a secure 60s WS ticket via REST
      final ticket = await ApiClient.issueWsTicket();

      // 2. Convert http:// / https:// to ws:// / wss://
      var wsBase = serverUrl;
      if (wsBase.startsWith('https://')) {
        wsBase = 'wss://${wsBase.substring(8)}';
      } else if (wsBase.startsWith('http://')) {
        wsBase = 'ws://${wsBase.substring(7)}';
      }

      final wsUri = Uri.parse('$wsBase/api/ws?ticket=$ticket');

      _channel = WebSocketChannel.connect(wsUri);
      _isConnected = true;
      _isConnecting = false;

      _streamController.add({'type': 'connection_status', 'connected': true});

      _channel!.stream.listen(
        (message) {
          try {
            final decoded = jsonDecode(message.toString()) as Map<String, dynamic>;
            _streamController.add(decoded);
          } catch (e) {
            // Ignore malformed ping frames
          }
        },
        onError: (error) {
          _handleDisconnect();
        },
        onDone: () {
          _handleDisconnect();
        },
      );
    } catch (e) {
      _handleDisconnect();
    }
  }

  void _handleDisconnect() {
    _isConnected = false;
    _isConnecting = false;
    _channel = null;
    _streamController.add({'type': 'connection_status', 'connected': false});

    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(const Duration(seconds: 3), () {
      connect();
    });
  }

  void disconnect() {
    _reconnectTimer?.cancel();
    _channel?.sink.close();
    _channel = null;
    _isConnected = false;
    _isConnecting = false;
    _streamController.add({'type': 'connection_status', 'connected': false});
  }

  void dispose() {
    disconnect();
    _streamController.close();
  }
}
