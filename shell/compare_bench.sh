#!/bin/bash
# pardus-browser comparison benchmarks
# Compares pardus-browser against curl, w3m, Puppeteer, and Playwright
# Usage: ./bench/compare_bench.sh [iterations] [port]
set -euo pipefail

ITERATIONS="${1:-10}"
PORT="${2:-18899}"
BINARY="target/release/pardus-browser"
SITE_DIR="$(dirname "$0")/../bench/site"
COMPARE_DIR="$(dirname "$0")/../bench/compare"
RESULTS_DIR="$(dirname "$0")/../bench/results"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
RESULTS_FILE="${RESULTS_DIR}/compare-${TIMESTAMP}.json"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

mkdir -p "$RESULTS_DIR"

PAGES=(
  "simple.html"
  "deep-nested.html"
  "wide-dom.html"
  "interactive.html"
  "semantic.html"
  "content-heavy.html"
  "forms.html"
  "nav-graph.html"
  "realistic.html"
)

BENCH_URL="http://127.0.0.1:${PORT}"

start_server() {
  python3 -m http.server "$PORT" -d "$SITE_DIR" --bind 127.0.0.1 > /dev/null 2>&1 &
  SERVER_PID=$!
  sleep 0.5
  if ! curl -s -o /dev/null "$BENCH_URL/" 2>/dev/null; then
    echo -e "${RED}Error: Failed to start HTTP server on port ${PORT}${NC}"
    exit 1
  fi
}

stop_server() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}

cleanup() {
  stop_server
}
trap cleanup EXIT

run_tool_bench() {
  local tool="$1"
  shift
  local url="$1"
  shift

  local values=()
  for i in $(seq 1 "$ITERATIONS"); do
    START_NS=$(python3 -c "import time; print(time.time_ns())")
    "$@" "$url" > /dev/null 2>&1 || true
    END_NS=$(python3 -c "import time; print(time.time_ns())")
    ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
    values+=($ELAPSED_MS)
  done

  local sum=0 min=${values[0]} max=${values[0]}
  for v in "${values[@]}"; do
    sum=$((sum + v))
    [ "$v" -lt "$min" ] && min=$v
    [ "$v" -gt "$max" ] && max=$v
  done
  local avg=$((sum / ITERATIONS))
  echo "{\"avg\":$avg,\"min\":$min,\"max\":$max}"
}

tool_available() {
  command -v "$1" >/dev/null 2>&1
}

echo ""
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  pardus-browser comparison benchmarks${NC}"
echo -e "${BOLD}  Iterations: ${ITERATIONS}  |  Port: ${PORT}${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""

echo -e "${CYAN}[0] Starting local HTTP server...${NC}"
start_server
echo -e "${GREEN}  Server ready on port ${PORT}${NC}"
echo ""

# Collect results
COMPARISON_JSON="{}"

# ── curl ──
if tool_available curl; then
  echo -e "${CYAN}[1] curl (raw HTTP fetch baseline)${NC}"
  CURL_JSON="{"
  for page in "${PAGES[@]}"; do
    url="${BENCH_URL}/${page}"
    echo -ne "  ${page} ..."
    result=$(run_tool_bench "curl" "$url" curl -s -o /dev/null)
    avg=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['avg'])")
    echo -e " avg:${avg}ms"
    CURL_JSON="${CURL_JSON}\"${page}\":${result},"
  done
  CURL_JSON="${CURL_JSON%,}}"
  COMPARISON_JSON=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['curl'] = json.loads('''${CURL_JSON}''')
print(json.dumps(d))
" 2>/dev/null)
  echo ""
else
  echo -e "${YELLOW}[1] curl not found — skipping${NC}"
  echo ""
fi

# ── w3m ──
if tool_available w3m; then
  echo -e "${CYAN}[2] w3m (text browser)${NC}"
  W3M_JSON="{"
  for page in "${PAGES[@]}"; do
    url="${BENCH_URL}/${page}"
    echo -ne "  ${page} ..."
    result=$(run_tool_bench "w3m" "$url" w3m -dump -no-cookie)
    avg=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['avg'])")
    echo -e " avg:${avg}ms"
    W3M_JSON="${W3M_JSON}\"${page}\":${result},"
  done
  W3M_JSON="${W3M_JSON%,}}"
  COMPARISON_JSON=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['w3m'] = json.loads('''${W3M_JSON}''')
print(json.dumps(d))
" 2>/dev/null)
  echo ""
else
  echo -e "${YELLOW}[2] w3m not found — skipping${NC}"
  echo ""
fi

# ── lynx ──
if tool_available lynx; then
  echo -e "${CYAN}[3] lynx (text browser)${NC}"
  LYNX_JSON="{"
  for page in "${PAGES[@]}"; do
    url="${BENCH_URL}/${page}"
    echo -ne "  ${page} ..."
    result=$(run_tool_bench "lynx" "$url" lynx -dump -nocolor -nolist)
    avg=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['avg'])")
    echo -e " avg:${avg}ms"
    LYNX_JSON="${LYNX_JSON}\"${page}\":${result},"
  done
  LYNX_JSON="${LYNX_JSON%,}}"
  COMPARISON_JSON=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['lynx'] = json.loads('''${LYNX_JSON}''')
print(json.dumps(d))
" 2>/dev/null)
  echo ""
else
  echo -e "${YELLOW}[3] lynx not found — skipping${NC}"
  echo ""
fi

# ── pardus-browser ──
if [ -f "$BINARY" ]; then
  echo -e "${CYAN}[4] pardus-browser (semantic tree)${NC}"
  PARDUS_JSON="{"
  for page in "${PAGES[@]}"; do
    url="${BENCH_URL}/${page}"
    echo -ne "  ${page} ..."
    result=$(run_tool_bench "pardus-browser" "$url" "$BINARY" navigate --format json)
    avg=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['avg'])")
    echo -e " avg:${avg}ms"
    PARDUS_JSON="${PARDUS_JSON}\"${page}\":${result},"
  done
  PARDUS_JSON="${PARDUS_JSON%,}}"
  COMPARISON_JSON=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['pardus-browser'] = json.loads('''${PARDUS_JSON}''')
print(json.dumps(d))
" 2>/dev/null)
  echo ""
else
  echo -e "${YELLOW}[4] pardus-browser binary not found — skipping${NC}"
  echo -e "${DIM}  Build it: cargo +nightly build --release -p pardus-cli${NC}"
  echo ""
fi

stop_server
SERVER_PID=""

# ── Puppeteer ──
if [ -d "${COMPARE_DIR}/node_modules" ] && tool_available node; then
  echo -e "${CYAN}[5] Puppeteer (headless Chrome)${NC}"
  echo -e "${DIM}  Restarting server for Node.js benchmarks...${NC}"
  start_server
  PUPPETEER_JSON=$(BENCH_URL="$BENCH_URL" ITERATIONS="$ITERATIONS" ALL_PAGES=1 \
    node "${COMPARE_DIR}/puppeteer.mjs" 2>&1 | tail -1 || echo "{}")
  PUPPETEER_PAGES=$(echo "$PUPPETEER_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(json.dumps(d.get('pages', {})))
" 2>/dev/null || echo "{}")
  COMPARISON_JSON=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['puppeteer'] = json.loads('''${PUPPETEER_PAGES}''')
print(json.dumps(d))
" 2>/dev/null)
  stop_server
  SERVER_PID=""
  echo ""
else
  echo -e "${YELLOW}[5] Puppeteer not installed — skipping${NC}"
  echo -e "${DIM}  Install: cd bench/compare && npm install && npx playwright install chromium${NC}"
  echo ""
fi

# ── Playwright ──
if [ -d "${COMPARE_DIR}/node_modules" ] && tool_available node; then
  echo -e "${CYAN}[6] Playwright (headless Chromium)${NC}"
  echo -e "${DIM}  Restarting server for Node.js benchmarks...${NC}"
  start_server
  PLAYWRIGHT_JSON=$(BENCH_URL="$BENCH_URL" ITERATIONS="$ITERATIONS" ALL_PAGES=1 \
    node "${COMPARE_DIR}/playwright.mjs" 2>&1 | tail -1 || echo "{}")
  PLAYWRIGHT_PAGES=$(echo "$PLAYWRIGHT_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(json.dumps(d.get('pages', {})))
" 2>/dev/null || echo "{}")
  COMPARISON_JSON=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['playwright'] = json.loads('''${PLAYWRIGHT_PAGES}''')
print(json.dumps(d))
" 2>/dev/null)
  stop_server
  SERVER_PID=""
  echo ""
else
  echo -e "${YELLOW}[6] Playwright not installed — skipping${NC}"
  echo -e "${DIM}  Install: cd bench/compare && npm install && npx playwright install chromium${NC}"
  echo ""
fi

# Save results
FINAL_JSON=$(python3 -c "
import json, sys

comparison = json.loads('''${COMPARISON_JSON}''')

output = {
    'timestamp': '${TIMESTAMP}',
    'platform': '$(uname -s) $(uname -m)',
    'iterations': ${ITERATIONS},
    'comparison': comparison
}

print(json.dumps(output, indent=2))
" 2>/dev/null)

echo "$FINAL_JSON" > "$RESULTS_FILE"

# ── Summary table ──
echo ""
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  Comparison Summary${NC}"
echo -e "${BOLD}  Platform: $(uname -s) $(uname -m)  |  Iterations: ${ITERATIONS}${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""

printf "  ${BOLD}%-25s" "Page"
tools=$(echo "$COMPARISON_JSON" | python3 -c "import sys,json; print(' '.join(json.load(sys.stdin).keys()))" 2>/dev/null)
for tool in $tools; do
  printf " %-12s" "$tool"
done
echo ""

printf "  ${DIM}%-25s" "─────────────────────────"
for tool in $tools; do
  printf " %-12s" "────────────"
done
echo ""

for page in "${PAGES[@]}"; do
  printf "  %-25s" "$page"
  for tool in $tools; do
    avg=$(echo "$COMPARISON_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
p = d.get('$tool', {}).get('$page', {})
print(p.get('avg', '?'))
" 2>/dev/null || echo "?")
    printf " %-11s" "${avg}ms"
  done
  echo ""
done

echo ""
echo -e "  Results: ${RESULTS_FILE}"
echo ""
