#!/bin/bash
# Integration tests for pardus-browser
# Usage: ./tests/run_tests.sh [--js]

set -e

cd "$(dirname "$0")/../.."
BIN="cargo run -q --"
PASS=0
FAIL=0
TOTAL=0

run_test() {
    local name="$1"
    shift
    TOTAL=$((TOTAL + 1))
    echo -n "  TEST $TOTAL: $name ... "
    if output=$("$@" 2>&1); then
        # Check output contains expected patterns
        PASS=$((PASS + 1))
        echo "OK"
    else
        FAIL=$((FAIL + 1))
        echo "FAIL"
        echo "    $output" | head -3
    fi
}

check_output() {
    local name="$1"
    local expected="$2"
    shift 2
    TOTAL=$((TOTAL + 1))
    echo -n "  TEST $TOTAL: $name ... "
    if output=$("$@" 2>&1); then
        if echo "$output" | grep -q "$expected"; then
            PASS=$((PASS + 1))
            echo "OK"
        else
            FAIL=$((FAIL + 1))
            echo "FAIL (expected '$expected')"
        fi
    else
        FAIL=$((FAIL + 1))
        echo "FAIL (exit code $?)"
        echo "    $output" | head -3
    fi
}

echo ""
echo "=========================================="
echo "  pardus-browser integration tests"
echo "=========================================="
echo ""

# ---- 1. Basic navigate (no JS) ----
echo "--- Static HTML parsing ---"

check_output "example.com — tree output" "Example Domain" \
    $BIN navigate https://example.com --format tree

check_output "example.com — md output" "Example Domain" \
    $BIN navigate https://example.com --format md

check_output "example.com — JSON output" "\"Example Domain\"" \
    $BIN navigate https://example.com --format json

check_output "example.com — JSON has links" "\"Learn more\"" \
    $BIN navigate https://example.com --format json --with-nav

check_output "example.com — nav graph has external link" "iana.org" \
    $BIN navigate https://example.com --format json --with-nav

check_output "example.com — stats line" "1 links" \
    $BIN navigate https://example.com --format md

# ---- 2. Interactive-only ----
echo ""
echo "--- Interactive-only mode ---"

check_output "example.com — interactive-only has link" "Learn more" \
    $BIN navigate https://example.com --interactive-only

check_output "example.com — interactive-only no headings" "heading" \
    $BIN navigate https://example.com --interactive-only --format md
    # Should NOT contain "heading" since interactive-only filters headings

# ---- 3. JS execution ----
echo ""
echo "--- JavaScript execution ---"

check_output "example.com --js — still parses" "Example Domain" \
    $BIN navigate https://example.com --js --format md

check_output "google.com --js — parses page" "document" \
    $BIN navigate https://www.google.com --js --format md

# ---- 4. Clean command ----
echo ""
echo "--- Clean command ---"

check_output "clean — no error" "does not exist" \
    $BIN clean
    # Should say cache dir doesn't exist (fresh)

echo ""
echo "=========================================="
echo "  Results: $PASS passed, $FAIL failed, $TOTAL total"
echo "=========================================="

if [ $FAIL -gt 0 ]; then
    exit 1
fi
