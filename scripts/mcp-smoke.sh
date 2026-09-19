#!/usr/bin/env bash
# MCP smoke test against a running ScholarGate gateway (Streamable HTTP).
#
# Usage:
#   scripts/mcp-smoke.sh [BASE_URL] [TOKEN]
#   scripts/mcp-smoke.sh http://127.0.0.1:8795 "$MCP_AUTH_TOKEN"
#
# Requires curl. Run `pnpm tauri dev` (or the built app) first so the gateway is up.
set -euo pipefail

BASE="${1:-http://127.0.0.1:8795}"
TOKEN="${2:-}"

call() {
  local body="$1"
  if [ -n "$TOKEN" ]; then
    curl -sS -H "Authorization: Bearer ${TOKEN}" \
      -H 'content-type: application/json' -H 'accept: application/json' \
      -d "${body}" "${BASE}/mcp"
  else
    curl -sS -H 'content-type: application/json' -H 'accept: application/json' \
      -d "${body}" "${BASE}/mcp"
  fi
  printf '\n'
}

echo "== health =="
curl -sS "${BASE}/health"; printf '\n\n'

echo "== initialize =="
call '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mcp-smoke","version":"1"}}}'

echo "== tools/list =="
call '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'

echo "== tools/call get_search_catalog =="
call '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_search_catalog","arguments":{}}}'

echo "== tools/call list_workspaces =="
call '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_workspaces","arguments":{}}}'

echo "Done. A 401 means the gateway token is set but missing/invalid."
