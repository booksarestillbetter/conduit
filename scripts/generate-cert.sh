#!/usr/bin/env bash
set -e

# ==============================================================================
#  Conduit SSL Certificate Generation Helper
# ==============================================================================

TARGET_DIR="${1:-/store/conduit/config/ssl}"
DOMAIN="${2:-conduit.local}"
DAYS="${3:-3650}"

echo "========================================================"
echo "🐕 Generating SSL Certificate for Conduit"
echo "   Target Directory: $TARGET_DIR"
echo "   Domain/Host:     $DOMAIN"
echo "   Validity:        $DAYS days"
echo "========================================================"

mkdir -p "$TARGET_DIR"

CERT_FILE="$TARGET_DIR/conduit.crt"
KEY_FILE="$TARGET_DIR/conduit.key"

openssl req -x509 -nodes -days "$DAYS" -newkey rsa:2048 \
  -keyout "$KEY_FILE" \
  -out "$CERT_FILE" \
  -subj "/C=US/ST=CA/L=LosAngeles/O=Conduit/CN=$DOMAIN" \
  -addext "subjectAltName = DNS:$DOMAIN,DNS:localhost,IP:127.0.0.1"

chmod 600 "$KEY_FILE"
chmod 644 "$CERT_FILE"

echo ""
echo "✅ SSL Certificate & Key created successfully:"
echo "   - Certificate: $CERT_FILE"
echo "   - Private Key: $KEY_FILE"
echo ""
echo "Conduit container will automatically detect these certificates and enable SSL on restart."
