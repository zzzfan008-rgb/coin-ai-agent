#!/bin/bash
# Coin-AI 三服务启动脚本
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WEB_DIST="$SCRIPT_DIR/web-client/dist"

# ── qwen 凭据（从 .env 或直接填入）────────────
source "$SCRIPT_DIR/.env" 2>/dev/null || true

QWEN_API_KEY="${DASHSCOPE_API_KEY:-}"
QWEN_BASE_URL="${QWEN_BASE_URL:-https://maas.qianwenaiapi.com/compatible-mode/v1}"

echo "=== 停止旧进程 ==="
lsof -ti:8081 | xargs kill -9 2>/dev/null || true
lsof -ti:8080 | xargs kill -9 2>/dev/null || true
lsof -ti:5173 | xargs kill -9 2>/dev/null || true
sleep 1

echo ""
echo "=== 启动 api-core (port 8081) ==="
cd "$SCRIPT_DIR/api-core"
DASHSCOPE_API_KEY="$QWEN_API_KEY" \
QWEN_BASE_URL="$QWEN_BASE_URL" \
./target/release/api-core > "$SCRIPT_DIR/../api-core.log" 2>&1 &
echo "PID: $!"

echo ""
echo "=== 启动 api-gateway (port 8080) ==="
cd "$SCRIPT_DIR/api-gateway"
source "$SCRIPT_DIR/.env"
WEB_DIST="$WEB_DIST" ./bin/server \
  > /tmp/gateway.log 2>&1 &
echo "PID: $!"

echo ""
echo "=== 启动 web-client (port 5173) ==="
cd "$SCRIPT_DIR/web-client"
npm run dev \
  > /tmp/vite.log 2>&1 &
echo "PID: $!"

sleep 3
echo ""
echo "=== 健康检查 ==="
curl -s http://localhost:8081/health | python3 -c "import sys,json;d=json.load(sys.stdin);print('core: ok | qwen:', d.get('llm_providers',{}).get('qwen'))"
curl -s http://localhost:8080/health | python3 -c "import sys,json;d=json.load(sys.stdin);print('gateway:', d['status'])"
curl -s -o /dev/null -w "vite: %{http_code}" http://localhost:5173/
echo ""
echo ""
echo "✅ 访问 http://localhost:8080/  (SPA，由 gateway 托管)"
echo "   登录: designer1 / admin123"
