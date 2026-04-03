#!/bin/bash
# pardus-browser benchmark suite — local reproducible benchmarks
# Usage: ./bench/run_bench.sh [iterations] [port]
set -euo pipefail

ITERATIONS="${1:-10}"
PORT="${2:-18899}"
BINARY="target/release/pardus-browser"
SITE_DIR="$(dirname "$0")/../bench/site"
RESULTS_DIR="$(dirname "$0")/../bench/results"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
RESULTS_FILE="${RESULTS_DIR}/${TIMESTAMP}.json"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

mkdir -p "$RESULTS_DIR"

check_binary() {
  if [ ! -f "$BINARY" ]; then
    echo -e "${RED}Error: Release binary not found at ${BINARY}${NC}"
    echo -e "${YELLOW}Build it first: cargo +nightly build --release -p pardus-cli${NC}"
    exit 1
  fi
}

start_server() {
  python3 -m http.server "$PORT" -d "$SITE_DIR" --bind 127.0.0.1 > /dev/null 2>&1 &
  SERVER_PID=$!
  sleep 0.5

  if ! curl -s -o /dev/null "http://127.0.0.1:${PORT}/" 2>/dev/null; then
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

run_benchmark() {
  local label="$1"
  shift
  local args=("$@")

  echo -ne "${DIM}  ${label} ...${NC}"

  local values=()
  for i in $(seq 1 "$ITERATIONS"); do
    START_NS=$(python3 -c "import time; print(time.time_ns())")
    "$BINARY" "${args[@]}" > /dev/null 2>&1
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

  local sorted=($(printf '%s\n' "${values[@]}" | sort -n))
  local mid=$(( ${#sorted[@]} / 2 ))
  local p50=${sorted[$mid]}
  local p99_idx=$(( ${#sorted[@]} * 99 / 100 ))
  [ $p99_idx -ge ${#sorted[@]} ] && p99_idx=$(( ${#sorted[@]} - 1 ))
  local p99=${sorted[$p99_idx]}

  echo -e "${GREEN} avg:${avg}ms min:${min}ms max:${max}ms p50:${p50}ms p99:${p99}ms${NC}"

  echo "{\"avg\":$avg,\"min\":$min,\"max\":$max,\"p50\":$p50,\"p99\":$p99}"
}

measure_rss() {
  local url="$1"
  local peak=0
  "$BINARY" navigate "$url" > /dev/null 2>&1 &
  local pid=$!
  while kill -0 "$pid" 2>/dev/null; do
    local rss=$(ps -o rss= -p "$pid" 2>/dev/null | awk '{print $1}')
    rss=${rss:-0}
    [ "$rss" -gt "$peak" ] && peak=$rss
    sleep 0.002
  done
  wait "$pid" 2>/dev/null || true
  echo $((peak * 1024))
}

get_tree_stats() {
  local url="$1"
  local output
  output=$("$BINARY" navigate "$url" --format json 2>/dev/null | sed '1,/^{/d' | sed 's/^.*\) {//' 2>/dev/null || true)
  echo "$output"
}

BASE_URL="http://127.0.0.1:${PORT}"

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

FORMATS=(
  "md:--format md"
  "tree:--format tree"
  "json:--format json --with-nav"
  "interactive:--interactive-only"
)

echo ""
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  pardus-browser benchmark suite — local reproducible${NC}"
echo -e "${BOLD}  Iterations: ${ITERATIONS}  |  Port: ${PORT}${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""

check_binary
echo -e "${CYAN}[0] Starting local HTTP server on port ${PORT}...${NC}"
start_server
echo -e "${GREEN}  Server ready.${NC}"
echo ""

BINARY_SIZE=$(stat -f%z "$BINARY")
BINARY_MB=$(echo "scale=2; $BINARY_SIZE / 1048576" | bc)

echo -e "${CYAN}[1] Binary size${NC}"
echo "  Release binary: ${BINARY_MB} MB"
echo ""

JSON_PAGES="{"
JSON_FORMATS="{"
JSON_RSS="{}"

echo -e "${CYAN}[2] Page benchmarks (${ITERATIONS} runs each)${NC}"
echo ""

for page in "${PAGES[@]}"; do
  echo -e "${BOLD}  ${page}${NC}"
  page_results="{"

  for fmt_entry in "${FORMATS[@]}"; do
    IFS=':' read -r fmt_name fmt_args <<< "$fmt_entry"
    result_json=$(run_benchmark "$fmt_name" navigate "${BASE_URL}/${page}" $fmt_args)
    page_results="${page_results}\"${fmt_name}\":${result_json},"
  done

  # JS benchmark (for pages that benefit from it)
  if [ "$page" = "scripts.html" ] || [ "$page" = "realistic.html" ]; then
    result_json=$(run_benchmark "js" navigate "${BASE_URL}/${page}" --js --wait-ms 3000)
    page_results="${page_results}\"js\":${result_json},"
  fi

  # Network log benchmark
  result_json=$(run_benchmark "network" navigate "${BASE_URL}/${page}" --network-log)
  page_results="${page_results}\"network\":${result_json}"

  page_results="${page_results}}"

  echo ""

  # Tree stats (single run)
  echo -ne "  tree-stats ..."
  stats_output=$("$BINARY" navigate "${BASE_URL}/${page}" --format json 2>/dev/null | sed -n '/^{/,/^}/p')

  total_nodes=$(echo "$stats_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('semantic_tree',{}).get('stats',{}).get('total_nodes','?'))" 2>/dev/null || echo "?")
  landmarks=$(echo "$stats_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('semantic_tree',{}).get('stats',{}).get('landmarks','?'))" 2>/dev/null || echo "?")
  links=$(echo "$stats_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('semantic_tree',{}).get('stats',{}).get('links','?'))" 2>/dev/null || echo "?")
  headings=$(echo "$stats_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('semantic_tree',{}).get('stats',{}).get('headings','?'))" 2>/dev/null || echo "?")
  actions=$(echo "$stats_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('semantic_tree',{}).get('stats',{}).get('actions','?'))" 2>/dev/null || echo "?")
  echo -e " nodes:${total_nodes} landmarks:${landmarks} links:${links} headings:${headings} actions:${actions}"
  echo ""

  stats_json="{\"total_nodes\":${total_nodes:-0},\"landmarks\":${landmarks:-0},\"links\":${links:-0},\"headings\":${headings:-0},\"actions\":${actions:-0}}"
  JSON_PAGES="${JSON_PAGES}\"${page}\":{\"timings\":${page_results},\"stats\":${stats_json}},"
done

JSON_PAGES="${JSON_PAGES%,}}"

echo -e "${CYAN}[3] Memory usage (peak RSS, 3 runs)${NC}"
echo ""

RSS_MEASUREMENTS=3
for page in simple.html wide-dom.html realistic.html scripts.html; do
  echo -ne "  ${page} ..."
  rss_sum=0
  rss_min=999999999
  rss_max=0
  for i in $(seq 1 $RSS_MEASUREMENTS); do
    rss=$(measure_rss "${BASE_URL}/${page}")
    rss_sum=$((rss_sum + rss))
    [ "$rss" -lt "$rss_min" ] && rss_min=$rss
    [ "$rss" -gt "$rss_max" ] && rss_max=$rss
  done
  rss_avg=$((rss_sum / RSS_MEASUREMENTS))
  rss_avg_mb=$(echo "scale=1; $rss_avg / 1048576" | bc)
  rss_min_mb=$(echo "scale=1; $rss_min / 1048576" | bc)
  rss_max_mb=$(echo "scale=1; $rss_max / 1048576" | bc)
  echo -e " avg:${rss_avg_mb}MB min:${rss_min_mb}MB max:${rss_max_mb}MB"
  JSON_RSS=$(echo "$JSON_RSS" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
d['$page'] = {'avg': $rss_avg, 'min': $rss_min, 'max': $rss_max}
print(json.dumps(d))
" 2>/dev/null || echo "$JSON_RSS")
done
echo ""

echo -e "${CYAN}[4] Interaction benchmarks${NC}"
echo ""

for action in "click '.titleline > a:first-child'" "type 'input[name=\"q\"]' 'benchmark search'" "wait 'h1'"; do
  echo -ne "  interact ${action} ..."
  values=()
  for i in $(seq 1 "$ITERATIONS"); do
    START_NS=$(python3 -c "import time; print(time.time_ns())")
    "$BINARY" interact "${BASE_URL}/simple.html" $action > /dev/null 2>&1
    END_NS=$(python3 -c "import time; print(time.time_ns())")
    ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
    values+=($ELAPSED_MS)
  done
  sum=0; min=${values[0]}; max=${values[0]}
  for v in "${values[@]}"; do sum=$((sum + v)); [ "$v" -lt "$min" ] && min=$v; [ "$v" -gt "$max" ] && max=$v; done
  avg=$((sum / ITERATIONS))
  echo -e " avg:${avg}ms min:${min}ms max:${max}ms"
done
echo ""

stop_server
SERVER_PID=""

PLATFORM="$(uname -s) $(uname -m)"

FINAL_JSON=$(python3 -c "
import json, sys

pages_data = json.loads('''${JSON_PAGES}''')
rss_data = json.loads('''${JSON_RSS}''')
binary_mb = ${BINARY_MB}
timestamp = '${TIMESTAMP}'
platform = '${PLATFORM}'

output = {
    'timestamp': timestamp,
    'platform': platform,
    'iterations': ${ITERATIONS},
    'binary_size_mb': round(binary_mb, 2),
    'pages': pages_data,
    'memory': rss_data
}

print(json.dumps(output, indent=2))
" 2>/dev/null)

echo "$FINAL_JSON" > "$RESULTS_FILE"

echo -e "${CYAN}[5] Results saved${NC}"
echo "  File: ${RESULTS_FILE}"
echo ""

echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  Summary — pardus-browser (${BINARY_MB} MB)${NC}"
echo -e "${BOLD}  Platform: ${PLATFORM}  |  Iterations: ${ITERATIONS}${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""

printf "  ${BOLD}%-25s %-8s %-8s %-8s %-8s${NC}\n" "Page" "Avg" "Min" "Max" "P99"
printf "  ${DIM}%-25s %-8s %-8s %-8s %-8s${NC}\n" "─────────────────────────" "────────" "────────" "────────" "────────"

for page in "${PAGES[@]}"; do
  avg_val=$(echo "$JSON_PAGES" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d['$page']['timings']['md']['avg'])" 2>/dev/null || echo "?")
  min_val=$(echo "$JSON_PAGES" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d['$page']['timings']['md']['min'])" 2>/dev/null || echo "?")
  max_val=$(echo "$JSON_PAGES" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d['$page']['timings']['md']['max'])" 2>/dev/null || echo "?")
  p99_val=$(echo "$JSON_PAGES" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d['$page']['timings']['md']['p99'])" 2>/dev/null || echo "?")
  nodes=$(echo "$JSON_PAGES" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d['$page']['stats']['total_nodes'])" 2>/dev/null || echo "?")
  printf "  %-25s %-8s %-8s %-8s %-8s\n" "${page} (${nodes}n)" "${avg_val}ms" "${min_val}ms" "${max_val}ms" "${p99_val}ms"
done

echo ""
echo -e "  Results: ${RESULTS_FILE}"
echo ""
