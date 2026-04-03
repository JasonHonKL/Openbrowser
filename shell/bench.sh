#!/bin/bash
# pardus-browser benchmark — RAM + Cold Start + E2E + JSON
set -euo pipefail

BINARY="target/release/pardus-browser"
URL="https://example.com"
ITERATIONS=10

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

run_test() {
    local label="$1"
    shift
    local args=("$@")

    echo -e "${CYAN}$label (${ITERATIONS} runs)${NC}"
    echo "─────────────────────────────────"

    local values=()
    for i in $(seq 1 $ITERATIONS); do
        START_NS=$(python3 -c "import time; print(time.time_ns())")
        "$BINARY" "${args[@]}" > /dev/null 2>&1
        END_NS=$(python3 -c "import time; print(time.time_ns())")
        ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
        values+=($ELAPSED_MS)
        printf "  Run %2d: %4d ms\n" "$i" "$ELAPSED_MS"
    done

    local sum=0 min=${values[0]} max=${values[0]}
    for v in "${values[@]}"; do
        sum=$((sum + v))
        [ "$v" -lt "$min" ] && min=$v
        [ "$v" -gt "$max" ] && max=$v
    done
    local avg=$((sum / ITERATIONS))

    echo ""
    printf "  ${GREEN}Avg: %4d ms  |  Min: %4d ms  |  Max: %4d ms${NC}\n\n" "$avg" "$min" "$max"
    echo "$avg:$min:$max"
}

echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  pardus-browser benchmark suite${NC}"
echo -e "${BOLD}  Target: $URL  |  Iterations: $ITERATIONS${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo ""

# 0 ── Binary size
echo -e "${CYAN}[0] Binary Size${NC}"
echo "─────────────────────────────────"
REL_SIZE=$(stat -f%z "$BINARY")
REL_MB=$(echo "scale=2; $REL_SIZE / 1048576" | bc)
DBG_SIZE=$(stat -f%z target/debug/pardus-browser)
DBG_MB=$(echo "scale=2; $DBG_SIZE / 1048576" | bc)
echo "  Release: ${REL_MB} MB"
echo "  Debug:   ${DBG_MB} MB"
echo ""

# 1 ── Cold Start
echo -e "${CYAN}[1] Cold Start (first run includes binary load)${NC}"
echo "─────────────────────────────────"
CS_RESULTS=$(run_test "Cold Start" "navigate" "$URL")
# Extract last line (avg:min:max)
CS_LINE=$(echo "$CS_RESULTS" | tail -1)
CS_AVG=$(echo "$CS_LINE" | cut -d: -f1)
CS_MIN=$(echo "$CS_LINE" | cut -d: -f2)
CS_MAX=$(echo "$CS_LINE" | cut -d: -f3)

# 2 ── RAM (peak RSS via /usr/bin/time)
echo -e "${CYAN}[2] RAM Usage — Peak RSS (${ITERATIONS} runs)${NC}"
echo "─────────────────────────────────"

RSS_VALUES=()
for i in $(seq 1 $ITERATIONS); do
    # Use /usr/bin/time -l on macOS, extract max RSS
    TIME_OUT=$(/usr/bin/time -l "$BINARY" navigate "$URL" > /dev/null 2>&1 || true)
    # Parse from stderr — need to capture it
    MAX_RSS=$(/usr/bin/time -l "$BINARY" navigate "$URL" > /dev/null 2>&1 | grep "maximum resident set size" | awk '{print $1}' || true)

    if [ -z "$MAX_RSS" ]; then
        # Fallback: poll ps
        MAX_RSS=$(python3 -c "
import subprocess, time
proc = subprocess.Popen(['$BINARY','navigate','$URL'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
peak = 0
while proc.poll() is None:
    try:
        r = subprocess.run(['ps','-o','rss=','-p',str(proc.pid)], capture_output=True, text=True)
        kb = int(r.stdout.strip().split()[0]) if r.stdout.strip() else 0
        if kb > peak: peak = kb
    except: pass
    time.sleep(0.001)
print(peak * 1024)
")
    fi
    RSS_VALUES+=("$MAX_RSS")
    RSS_MB=$(echo "scale=2; $MAX_RSS / 1048576" | bc)
    printf "  Run %2d: %6s MB\n" "$i" "$RSS_MB"
done

RSS_SUM=0 RSS_MIN=${RSS_VALUES[0]} RSS_MAX=${RSS_VALUES[0]}
for v in "${RSS_VALUES[@]}"; do
    RSS_SUM=$((RSS_SUM + v))
    [ "$v" -lt "$RSS_MIN" ] && RSS_MIN=$v
    [ "$v" -gt "$RSS_MAX" ] && RSS_MAX=$v
done
RSS_AVG=$((RSS_SUM / ITERATIONS))
RSS_AVG_MB=$(echo "scale=2; $RSS_AVG / 1048576" | bc)
RSS_MIN_MB=$(echo "scale=2; $RSS_MIN / 1048576" | bc)
RSS_MAX_MB=$(echo "scale=2; $RSS_MAX / 1048576" | bc)

echo ""
echo -e "  ${GREEN}Avg: ${RSS_AVG_MB} MB  |  Min: ${RSS_MIN_MB} MB  |  Max: ${RSS_MAX_MB} MB${NC}"
echo ""

# 3 ── JSON + nav graph
echo -e "${CYAN}[3] JSON output --format json --with-nav${NC}"
echo "─────────────────────────────────"
JSON_RESULTS=$(run_test "JSON+Nav" "navigate" "--format" "json" "--with-nav" "$URL")
JSON_LINE=$(echo "$JSON_RESULTS" | tail -1)
JSON_AVG=$(echo "$JSON_LINE" | cut -d: -f1)
JSON_MIN=$(echo "$JSON_LINE" | cut -d: -f2)
JSON_MAX=$(echo "$JSON_LINE" | cut -d: -f3)

# ─── Summary ───
echo ""
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  Benchmark Summary${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo ""
echo "  ┌────────────────────────────────┬─────────────────┐"
echo "  │ Metric                         │ Value           │"
echo "  ├────────────────────────────────┼─────────────────┤"
printf "  │ Binary size (release)          │ %10s MB    │\n" "$REL_MB"
printf "  │ Cold start (avg of %2d)         │ %7d ms      │\n" "$ITERATIONS" "$CS_AVG"
printf "  │ Cold start (best)              │ %7d ms      │\n" "$CS_MIN"
printf "  │ Approx. RAM (peak RSS avg)     │ %7s MB      │\n" "$RSS_AVG_MB"
printf "  │ Approx. RAM (peak RSS min)     │ %7s MB      │\n" "$RSS_MIN_MB"
printf "  │ Approx. RAM (peak RSS max)     │ %7s MB      │\n" "$RSS_MAX_MB"
printf "  │ JSON + nav-graph (avg)         │ %7d ms      │\n" "$JSON_AVG"
echo "  └────────────────────────────────┴─────────────────┘"
echo ""
echo -e "  ${YELLOW}Platform: $(uname -s) $(uname -m)${NC}"
echo -e "  ${YELLOW}Date: $(date -u +"%Y-%m-%d %H:%M UTC")${NC}"
echo ""
