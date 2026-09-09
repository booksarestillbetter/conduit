class Peer {
  final String address;
  final String clientName;
  final double progress;
  final int rateToClient;
  final int rateToPeer;
  final String flagStr;
  final bool isEncrypted;
  final bool isUtp;
  final String? countryCode;

  Peer({
    required this.address,
    required this.clientName,
    required this.progress,
    required this.rateToClient,
    required this.rateToPeer,
    required this.flagStr,
    required this.isEncrypted,
    required this.isUtp,
    this.countryCode,
  });

  factory Peer.fromJson(Map<String, dynamic> json) {
    return Peer(
      address: json['address']?.toString() ?? '0.0.0.0',
      clientName: json['client_name']?.toString() ?? json['clientName']?.toString() ?? 'Unknown',
      progress: (json['progress'] as num?)?.toDouble() ?? 0.0,
      rateToClient: (json['rate_to_client'] as num?)?.toInt() ?? (json['rateToClient'] as num?)?.toInt() ?? 0,
      rateToPeer: (json['rate_to_peer'] as num?)?.toInt() ?? (json['rateToPeer'] as num?)?.toInt() ?? 0,
      flagStr: json['flag_str']?.toString() ?? json['flagStr']?.toString() ?? '',
      isEncrypted: json['is_encrypted'] == true || json['isEncrypted'] == true,
      isUtp: json['is_utp'] == true || json['isUTP'] == true,
      countryCode: json['country_code']?.toString(),
    );
  }
}
