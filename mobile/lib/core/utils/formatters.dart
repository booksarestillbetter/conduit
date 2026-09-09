import 'dart:math';
import 'package:intl/intl.dart';

class Formatters {
  static String formatBytes(int bytes, {int decimals = 1}) {
    if (bytes <= 0) return '0 B';
    const suffixes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
    final i = (log(bytes) / log(1024)).floor();
    final clampedI = i.clamp(0, suffixes.length - 1);
    final size = bytes / pow(1024, clampedI);
    return '${size.toStringAsFixed(decimals)} ${suffixes[clampedI]}';
  }

  static String formatSpeed(int bytesPerSecond) {
    if (bytesPerSecond <= 0) return '0 B/s';
    return '${formatBytes(bytesPerSecond)}/s';
  }

  static String formatRatio(double ratio) {
    if (ratio.isInfinite || ratio.isNaN || ratio < 0) return '0.00';
    return ratio.toStringAsFixed(2);
  }

  static String formatEta(int seconds) {
    if (seconds < 0 || seconds >= 8640000) return '∞';
    if (seconds < 60) return '${seconds}s';
    final minutes = (seconds / 60).floor();
    if (minutes < 60) {
      final remSec = seconds % 60;
      return '${minutes}m ${remSec}s';
    }
    final hours = (minutes / 60).floor();
    final remMin = minutes % 60;
    if (hours < 24) {
      return '${hours}h ${remMin}m';
    }
    final days = (hours / 24).floor();
    final remHours = hours % 24;
    return '${days}d ${remHours}h';
  }

  static String formatDateTime(DateTime dt) {
    final now = DateTime.now();
    final diff = now.difference(dt);
    if (diff.inSeconds < 60) return 'just now';
    if (diff.inMinutes < 60) return '${diff.inMinutes}m ago';
    if (diff.inHours < 24) return '${diff.inHours}h ago';
    if (diff.inDays < 7) return '${diff.inDays}d ago';
    return DateFormat('MMM d, yyyy').format(dt);
  }

  static String formatDate(DateTime dt) => formatDateTime(dt);
}
