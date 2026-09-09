#!/usr/bin/env bash
set -e

# ==============================================================================
#  Conduit Entrypoint with Nginx Reverse Proxy & Dynamic SSL Management
# ==============================================================================

# 1. Configuration & Default Ports
CONDUIT_INTERNAL_PORT="${CONDUIT_INTERNAL_PORT:-42420}"
CONDUIT_PORT="${CONDUIT_PORT:-4242}"
CONDUIT_HTTP_PORT="${CONDUIT_HTTP_PORT:-$CONDUIT_PORT}"
CONDUIT_HTTPS_PORT="${CONDUIT_HTTPS_PORT:-443}"
CONDUIT_SSL_REDIRECT="${CONDUIT_SSL_REDIRECT:-false}"
CONDUIT_CLIENT_MAX_BODY_SIZE="${CONDUIT_CLIENT_MAX_BODY_SIZE:-256M}"
CONDUIT_SSL_ENABLED="${CONDUIT_SSL_ENABLED:-auto}"
CONDUIT_SSL_CERT="${CONDUIT_SSL_CERT}"
CONDUIT_SSL_KEY="${CONDUIT_SSL_KEY}"
CONDUIT_SSL_AUTO_GENERATE="${CONDUIT_SSL_AUTO_GENERATE:-false}"
CONDUIT_HOSTNAME="${CONDUIT_HOSTNAME:-conduit.local}"
CONDUIT_NGINX_ENABLED="${CONDUIT_NGINX_ENABLED:-true}"
CONDUIT_BIND="${CONDUIT_BIND:-0.0.0.0}"

echo "========================================================"
echo "Starting Conduit"
echo "========================================================"

# 2. Check for Direct Binary Mode (Bypass Nginx)
if [ "$CONDUIT_NGINX_ENABLED" = "false" ] || [ "$CONDUIT_NGINX_ENABLED" = "0" ]; then
    echo "[CONDUIT-START] Nginx bypassed via CONDUIT_NGINX_ENABLED=false. Running backend directly on ${CONDUIT_BIND}:${CONDUIT_PORT}."
    exec /app/conduit
fi

# 3. Locate SSL Certificates
CERT_FILE=""
KEY_FILE=""

# Check explicit variables first
if [ -n "$CONDUIT_SSL_CERT" ] && [ -n "$CONDUIT_SSL_KEY" ]; then
    if [ -f "$CONDUIT_SSL_CERT" ] && [ -f "$CONDUIT_SSL_KEY" ]; then
        CERT_FILE="$CONDUIT_SSL_CERT"
        KEY_FILE="$CONDUIT_SSL_KEY"
    else
        echo "[CONDUIT-SSL] Warning: Specified SSL cert ($CONDUIT_SSL_CERT) or key ($CONDUIT_SSL_KEY) not found!"
    fi
fi

# Search well-known directory locations if not explicitly found
if [ -z "$CERT_FILE" ] || [ -z "$KEY_FILE" ]; then
    CANDIDATE_DIRS=("/data/ssl" "/data/certs" "/data" "/etc/ssl/certs")
    CERT_NAMES=(
        "conduit.crt" "conduit.pem" "conduit-fullchain.pem"
        "fullchain.pem" "cert.pem" "tls.crt"
    )
    KEY_NAMES=(
        "conduit.key" "conduit-key.pem"
        "privkey.pem" "key.pem" "tls.key"
    )

    for dir in "${CANDIDATE_DIRS[@]}"; do
        if [ -d "$dir" ]; then
            for c in "${CERT_NAMES[@]}"; do
                if [ -z "$CERT_FILE" ] && [ -f "$dir/$c" ] && [ -s "$dir/$c" ]; then
                    CERT_FILE="$dir/$c"
                    break
                fi
            done
            for k in "${KEY_NAMES[@]}"; do
                if [ -z "$KEY_FILE" ] && [ -f "$dir/$k" ] && [ -s "$dir/$k" ]; then
                    KEY_FILE="$dir/$k"
                    break
                fi
            done
        fi
        if [ -n "$CERT_FILE" ] && [ -n "$KEY_FILE" ]; then
            break
        fi
    done
fi

# Fallback: Dynamic wildcard scanning for any valid cert and key in candidate directories
if [ -z "$CERT_FILE" ] || [ -z "$KEY_FILE" ]; then
    for dir in "${CANDIDATE_DIRS[@]}"; do
        if [ -d "$dir" ]; then
            if [ -z "$CERT_FILE" ]; then
                for f in "$dir"/*.crt "$dir"/*.pem "$dir"/*.cer; do
                    if [ -f "$f" ] && grep -q "BEGIN CERTIFICATE" "$f" 2>/dev/null; then
                        CERT_FILE="$f"
                        break
                    fi
                done
            fi
            if [ -z "$KEY_FILE" ]; then
                for f in "$dir"/*.key "$dir"/*.pem; do
                    if [ -f "$f" ] && grep -q -- "PRIVATE KEY" "$f" 2>/dev/null; then
                        KEY_FILE="$f"
                        break
                    fi
                done
            fi
        fi
        if [ -n "$CERT_FILE" ] && [ -n "$KEY_FILE" ]; then
            break
        fi
    done
fi

# 4. Determine SSL Mode
USE_SSL=false
case "${CONDUIT_SSL_ENABLED,,}" in
    true|1|yes|on)
        if [ -n "$CERT_FILE" ] && [ -n "$KEY_FILE" ]; then
            USE_SSL=true
        elif [ "$CONDUIT_SSL_AUTO_GENERATE" = "true" ] || [ "$CONDUIT_SSL_AUTO_GENERATE" = "1" ]; then
            echo "[CONDUIT-SSL] CONDUIT_SSL_ENABLED=true but no cert found. Auto-generating self-signed certificate in /data/ssl/..."
            mkdir -p /data/ssl
            CERT_FILE="/data/ssl/conduit.crt"
            KEY_FILE="/data/ssl/conduit.key"
            openssl req -x509 -nodes -days 3650 -newkey rsa:2048 \
                -keyout "$KEY_FILE" -out "$CERT_FILE" \
                -subj "/C=US/ST=State/L=City/O=Conduit/CN=${CONDUIT_HOSTNAME}" 2>/dev/null
            chmod 600 "$KEY_FILE"
            USE_SSL=true
            echo "[CONDUIT-SSL] Self-signed certificate generated successfully at $CERT_FILE."
        else
            echo "--------------------------------------------------------"
            echo "❌ [CONDUIT-SSL] ERROR: CONDUIT_SSL_ENABLED=true is set, but no valid SSL certificate or key was found!"
            echo "   Please place your certificate and private key in:"
            echo "   - Certificate: /data/ssl/conduit.crt (or fullchain.pem)"
            echo "   - Private Key: /data/ssl/conduit.key (or privkey.pem)"
            echo "   Or set CONDUIT_SSL_AUTO_GENERATE=true to create a self-signed cert."
            echo "--------------------------------------------------------"
            echo "[CONDUIT-SSL] Falling back to HTTP mode for safety."
            USE_SSL=false
        fi
        ;;
    false|0|no|off)
        echo "[CONDUIT-SSL] SSL explicitly disabled (CONDUIT_SSL_ENABLED=${CONDUIT_SSL_ENABLED})."
        USE_SSL=false
        ;;
    auto|*)
        if [ -n "$CERT_FILE" ] && [ -n "$KEY_FILE" ]; then
            echo "[CONDUIT-SSL] Valid SSL certificates found at ${CERT_FILE} and ${KEY_FILE}. Auto-enabling SSL mode."
            USE_SSL=true
        else
            echo "[CONDUIT-SSL] No SSL certificates detected in /data/ssl/. Running in standard HTTP mode."
            echo "[CONDUIT-SSL] (To enable SSL later, place conduit.crt and conduit.key in /data/ssl/ and restart)."
            USE_SSL=false
        fi
        ;;
esac

# 5. Generate Dynamic Nginx Configuration
mkdir -p /etc/nginx/conf.d /etc/nginx/sites-enabled /var/log/nginx /var/cache/nginx
rm -f /etc/nginx/conf.d/*.conf /etc/nginx/sites-enabled/* /etc/nginx/sites-available/*

cat << 'EOF' > /etc/nginx/nginx.conf
user www-data;
worker_processes auto;
pid /run/nginx.pid;
error_log /var/log/nginx/error.log warn;

events {
    worker_connections 1024;
    multi_accept on;
}

http {
    include /etc/nginx/mime.types;
    default_type application/octet-stream;

    log_format main '$remote_addr - $remote_user [$time_local] "$request" '
                    '$status $body_bytes_sent "$http_referer" '
                    '"$http_user_agent" "$http_x_forwarded_for" '
                    'rt=$request_time uct="$upstream_connect_time" uht="$upstream_header_time" urt="$upstream_response_time"';

    access_log /var/log/nginx/access.log main;

    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    keepalive_timeout 65;
    types_hash_max_size 2048;
    server_tokens off;

    # Gzip Compression
    gzip on;
    gzip_disable "msie6";
    gzip_vary on;
    gzip_proxied any;
    gzip_comp_level 6;
    gzip_types text/plain text/css text/xml application/json application/javascript application/rss+xml application/atom+xml image/svg+xml;

    # WebSocket Upgrade Mapping
    map $http_upgrade $connection_upgrade {
        default upgrade;
        '' close;
    }

    include /etc/nginx/conf.d/*.conf;
}
EOF

# Build Server Block
if [ "$USE_SSL" = "true" ]; then
    echo "[CONDUIT-NGINX] Configuring Nginx with SSL (Certificate: ${CERT_FILE}, Key: ${KEY_FILE}, Port: ${CONDUIT_HTTPS_PORT})..."

    # If HTTP port differs from HTTPS port
    if [ "$CONDUIT_HTTP_PORT" != "$CONDUIT_HTTPS_PORT" ]; then
        if [ "$CONDUIT_SSL_REDIRECT" = "true" ] || [ "$CONDUIT_SSL_REDIRECT" = "1" ]; then
            cat << EOF > /etc/nginx/conf.d/conduit_http_redirect.conf
server {
    listen ${CONDUIT_HTTP_PORT};
    listen [::]:${CONDUIT_HTTP_PORT};
    server_name _;

    location / {
        return 301 https://\$host:${CONDUIT_HTTPS_PORT}\$request_uri;
    }
}
EOF
        else
            cat << EOF > /etc/nginx/conf.d/conduit_http_dual.conf
server {
    listen ${CONDUIT_HTTP_PORT};
    listen [::]:${CONDUIT_HTTP_PORT};
    server_name _;
    client_max_body_size ${CONDUIT_CLIENT_MAX_BODY_SIZE};

    location / {
        proxy_pass http://127.0.0.1:${CONDUIT_INTERNAL_PORT};
        proxy_http_version 1.1;
        proxy_set_header Host \$http_host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_set_header X-Forwarded-Host \$http_host;
        proxy_set_header X-Forwarded-Port \$server_port;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$connection_upgrade;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
EOF
        fi
    fi

    cat << EOF > /etc/nginx/conf.d/conduit_https.conf
server {
    listen ${CONDUIT_HTTPS_PORT} ssl;
    listen [::]:${CONDUIT_HTTPS_PORT} ssl;
    server_name _;
    client_max_body_size ${CONDUIT_CLIENT_MAX_BODY_SIZE};

    ssl_certificate ${CERT_FILE};
    ssl_certificate_key ${KEY_FILE};

    # Modern TLS Security
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers 'ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:DHE-RSA-AES128-GCM-SHA256:DHE-RSA-AES256-GCM-SHA384';
    ssl_prefer_server_ciphers off;
    ssl_session_cache shared:CONDUIT_SSL:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;

    # Auto-redirect plain HTTP requests hitting the HTTPS port to HTTPS
    error_page 497 =301 https://\$host:\$server_port\$request_uri;

    # Full Reverse Proxy Routing for Conduit (SPA, API, WebSockets, Metrics)
    location / {
        proxy_pass http://127.0.0.1:${CONDUIT_INTERNAL_PORT};
        proxy_http_version 1.1;
        proxy_set_header Host \$http_host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-Host \$http_host;
        proxy_set_header X-Forwarded-Port \$server_port;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$connection_upgrade;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
EOF

else
    echo "[CONDUIT-NGINX] Configuring Nginx in HTTP mode (Port: ${CONDUIT_HTTP_PORT})..."

    cat << EOF > /etc/nginx/conf.d/conduit_http.conf
server {
    listen ${CONDUIT_HTTP_PORT};
    listen [::]:${CONDUIT_HTTP_PORT};
    server_name _;
    client_max_body_size ${CONDUIT_CLIENT_MAX_BODY_SIZE};

    # Full Reverse Proxy Routing for Conduit (SPA, API, WebSockets, Metrics)
    location / {
        proxy_pass http://127.0.0.1:${CONDUIT_INTERNAL_PORT};
        proxy_http_version 1.1;
        proxy_set_header Host \$http_host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_set_header X-Forwarded-Host \$http_host;
        proxy_set_header X-Forwarded-Port \$server_port;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$connection_upgrade;
        proxy_buffering off;
        proxy_read_timeout 3600s;
    }
}
EOF
fi

# 6. Test Nginx Configuration
nginx -t

# 7. Start Conduit Backend (Internal Daemon)
echo "[CONDUIT-START] Launching Conduit backend binary on 127.0.0.1:${CONDUIT_INTERNAL_PORT} (HTTP)..."
export CONDUIT_BIND="127.0.0.1"
export CONDUIT_PORT="${CONDUIT_INTERNAL_PORT}"

# Nginx handles SSL termination externally, so the internal daemon runs in HTTP mode
unset CONDUIT_SSL_ENABLED CONDUIT_SSL_CERT CONDUIT_SSL_KEY

/app/conduit --bind 127.0.0.1 --port "${CONDUIT_INTERNAL_PORT}" --ssl-enabled false &
CONDUIT_PID=$!

# Trap signals for graceful shutdown
cleanup() {
    echo "[CONDUIT-SHUTDOWN] Received termination signal. Shutting down gracefully..."
    kill -TERM "$CONDUIT_PID" 2>/dev/null || true
    nginx -s quit 2>/dev/null || true
    wait "$CONDUIT_PID" 2>/dev/null || true
    exit 0
}

trap cleanup SIGINT SIGTERM SIGHUP

# 8. Start Nginx Reverse Proxy in Foreground
echo "[CONDUIT-START] Nginx reverse proxy online. Ready for connections."
nginx -g 'daemon off;' &
NGINX_PID=$!

# Wait for either process to terminate
wait -n "$CONDUIT_PID" "$NGINX_PID" || true
cleanup
