#!/usr/bin/env bash
set -euo pipefail

# ========= EDIT THIS =========
GEMINI_API_KEY=""
# =============================

APP_DIR="/opt/cognee"
SERVER_IP="$(curl -s4 https://ifconfig.me || hostname -I | awk '{print $1}')"

export DEBIAN_FRONTEND=noninteractive

echo "=== Updating system ==="
apt-get update -y
apt-get upgrade -y

echo "=== Installing dependencies ==="
apt-get install -y \
  git \
  curl \
  ca-certificates \
  ufw \
  docker.io \
  docker-compose-v2

systemctl enable --now docker

echo "=== Opening firewall ports ==="
ufw allow OpenSSH || true
ufw allow 8000/tcp || true
ufw --force enable || true

echo "=== Cloning Cognee ==="
rm -rf "$APP_DIR"
git clone https://github.com/topoteretes/cognee.git "$APP_DIR"
cd "$APP_DIR"

echo "=== Creating .env ==="
if [ -f ".env.template" ]; then
  cp .env.template .env
else
  touch .env
fi

set_env() {
  local key="$1"
  local value="$2"

  if grep -q "^${key}=" .env; then
    sed -i "s|^${key}=.*|${key}=${value}|g" .env
  else
    echo "${key}=${value}" >> .env
  fi
}

echo "=== Configuring Gemini ==="

# LLM config
set_env "LLM_PROVIDER" "\"gemini\""
set_env "LLM_MODEL" "\"gemini/gemini-3.1-flash-lite\""
set_env "LLM_API_KEY" "\"${GEMINI_API_KEY}\""

# Embedding config
set_env "EMBEDDING_PROVIDER" "\"gemini\""
set_env "EMBEDDING_MODEL" "\"gemini/gemini-embedding-001\""
set_env "EMBEDDING_API_KEY" "\"${GEMINI_API_KEY}\""

# Public API config
set_env "REQUIRE_AUTHENTICATION" "true"
set_env "CORS_ALLOWED_ORIGINS" "\"*\""

# Simple single-server storage defaults
set_env "DB_PROVIDER" "sqlite"
set_env "GRAPH_DATABASE_PROVIDER" "kuzu"
set_env "VECTOR_DB_PROVIDER" "lancedb"

echo "=== Starting Cognee ==="
docker compose up -d --build cognee

echo "=== Creating systemd autostart ==="
cat >/etc/systemd/system/cognee.service <<EOF
[Unit]
Description=Cognee Docker Compose Service
Requires=docker.service
After=docker.service

[Service]
Type=oneshot
WorkingDirectory=${APP_DIR}
ExecStart=/usr/bin/docker compose up -d cognee
ExecStop=/usr/bin/docker compose down
RemainAfterExit=yes
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable cognee.service

echo "================================================="
echo "Cognee should be live at:"
echo "API:  http://${SERVER_IP}:8000"
echo "Docs: http://${SERVER_IP}:8000/docs"
echo ""
echo "Check logs with:"
echo "cd ${APP_DIR} && docker compose logs -f cognee"
echo "================================================="