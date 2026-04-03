#!/bin/bash
# pardus-browser screenshot capture test
# Tests the screenshot module (feature-gated behind `screenshot`)
# Requires: Google Chrome or Chromium installed on the system
set -euo pipefail

cd "$(dirname "$0")/.."

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

PASS=0
FAIL=0
TOTAL=0

run_test() {
    local name="$1"
    shift
    TOTAL=$((TOTAL + 1))
    echo -ne "  ${CYAN}TEST ${TOTAL}: ${name}${NC} ... "
    if output=$("$@" 2>&1); then
        PASS=$((PASS + 1))
        echo -e "${GREEN}OK${NC}"
    else
        FAIL=$((FAIL + 1))
        echo -e "${RED}FAIL${NC}"
        echo "$output" | head -5 | sed 's/^/      /'
    fi
}

check_output() {
    local name="$1"
    local expected="$2"
    shift 2
    TOTAL=$((TOTAL + 1))
    echo -ne "  ${CYAN}TEST ${TOTAL}: ${name}${NC} ... "
    if output=$("$@" 2>&1); then
        if echo "$output" | grep -q "$expected"; then
            PASS=$((PASS + 1))
            echo -e "${GREEN}OK${NC}"
        else
            FAIL=$((FAIL + 1))
            echo -e "${RED}FAIL${NC} (expected '${expected}')"
            echo "$output" | head -3 | sed 's/^/      /'
        fi
    else
        FAIL=$((FAIL + 1))
        echo -e "${RED}FAIL${NC} (exit code $?)"
        echo "$output" | head -3 | sed 's/^/      /'
    fi
}

echo ""
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  pardus-browser screenshot capture tests${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo ""

# ── Step 0: Check Chrome ──
echo -e "${CYAN}[0] Checking for Chrome/Chromium${NC}"
echo "─────────────────────────────────"

CHROME_PATH=""
if [ -f "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ]; then
    CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
elif command -v chromium &>/dev/null; then
    CHROME_PATH="$(which chromium)"
elif command -v google-chrome &>/dev/null; then
    CHROME_PATH="$(which google-chrome)"
elif command -v chrome &>/dev/null; then
    CHROME_PATH="$(which chrome)"
fi

if [ -z "$CHROME_PATH" ]; then
    echo -e "  ${RED}No Chrome/Chromium found. Install Chrome to run screenshot tests.${NC}"
    exit 1
fi
echo -e "  ${GREEN}Found: ${CHROME_PATH}${NC}"
echo ""

# ── Step 1: Compile with screenshot feature ──
echo -e "${CYAN}[1] Compiling pardus-core with screenshot feature${NC}"
echo "─────────────────────────────────"

COMPILE_OUTPUT=$(cargo +nightly build -p pardus-core --features screenshot 2>&1)
if [ $? -eq 0 ]; then
    echo -e "  ${GREEN}Compiled successfully${NC}"
else
    echo -e "  ${RED}Compilation failed:${NC}"
    echo "$COMPILE_OUTPUT" | tail -10 | sed 's/^/      /'
    exit 1
fi
echo ""

# ── Step 2: Compile and run the screenshot integration test ──
echo -e "${CYAN}[2] Running screenshot integration tests${NC}"
echo "─────────────────────────────────"

TEST_OUTPUT_DIR="/tmp/pardus-screenshot-test"
rm -rf "$TEST_OUTPUT_DIR"
mkdir -p "$TEST_OUTPUT_DIR"

# Write a one-off integration test binary
TEST_SRC="/tmp/pardus_screenshot_test.rs"
cat > "$TEST_SRC" << 'RUSTEOF'
//! Screenshot integration test — exercises capture_full_page and capture_element.
//! Run via: shell/test-screenshot.sh

use std::path::PathBuf;
use std::time::Duration;
use pardus_core::screenshot::{ScreenshotHandle, ScreenshotOptions, ScreenshotFormat};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let chrome_path = args.get(1).cloned().map(PathBuf::from);
    let output_dir = args.get(2).cloned().unwrap_or_else(|| "/tmp/pardus-screenshot-test".into());

    println!("  [setup] Creating ScreenshotHandle...");
    let handle = ScreenshotHandle::new(chrome_path, 1280, 720);

    let mut pass = 0u32;
    let mut fail = 0u32;

    // ── Test 1: Full-page PNG screenshot ──
    print!("  [1/4] Full-page PNG screenshot of example.com ... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let opts = ScreenshotOptions {
        format: ScreenshotFormat::Png,
        full_page: true,
        timeout_ms: 15_000,
        ..Default::default()
    };
    match handle.capture_page("https://example.com", &opts).await {
        Ok(bytes) => {
            let path = format!("{}/full_page.png", output_dir);
            std::fs::write(&path, &bytes).expect("write png");
            let size = bytes.len();
            // PNG magic bytes: 89 50 4E 47
            if size > 1000 && bytes[0..4] == [0x89, 0x50, 0x4E, 0x47] {
                println!("OK ({} bytes, valid PNG)", size);
                pass += 1;
            } else {
                println!("FAIL ({} bytes, invalid PNG header)", size);
                fail += 1;
            }
        }
        Err(e) => {
            println!("FAIL ({})", e);
            fail += 1;
        }
    }

    // ── Test 2: Viewport-only PNG screenshot ──
    print!("  [2/4] Viewport-only PNG screenshot of example.com ... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let opts_viewport = ScreenshotOptions {
        format: ScreenshotFormat::Png,
        full_page: false,
        timeout_ms: 15_000,
        ..Default::default()
    };
    match handle.capture_page("https://example.com", &opts_viewport).await {
        Ok(bytes) => {
            let path = format!("{}/viewport.png", output_dir);
            std::fs::write(&path, &bytes).expect("write png");
            let size = bytes.len();
            if size > 500 && bytes[0..4] == [0x89, 0x50, 0x4E, 0x47] {
                println!("OK ({} bytes, valid PNG)", size);
                pass += 1;
            } else {
                println!("FAIL ({} bytes, invalid)", size);
                fail += 1;
            }
        }
        Err(e) => {
            println!("FAIL ({})", e);
            fail += 1;
        }
    }

    // ── Test 3: JPEG screenshot with quality ──
    print!("  [3/4] JPEG screenshot (quality=80) of example.com ... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let opts_jpeg = ScreenshotOptions {
        format: ScreenshotFormat::Jpeg { quality: 80 },
        full_page: true,
        timeout_ms: 15_000,
        ..Default::default()
    };
    match handle.capture_page("https://example.com", &opts_jpeg).await {
        Ok(bytes) => {
            let path = format!("{}/full_page.jpg", output_dir);
            std::fs::write(&path, &bytes).expect("write jpeg");
            let size = bytes.len();
            // JPEG magic: FF D8 FF
            if size > 500 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
                println!("OK ({} bytes, valid JPEG)", size);
                pass += 1;
            } else {
                println!("FAIL ({} bytes, invalid JPEG header)", size);
                fail += 1;
            }
        }
        Err(e) => {
            println!("FAIL ({})", e);
            fail += 1;
        }
    }

    // ── Test 4: Element screenshot ──
    print!("  [4/4] Element screenshot of h1 on example.com ... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let opts_elem = ScreenshotOptions {
        format: ScreenshotFormat::Png,
        full_page: false,
        timeout_ms: 15_000,
        ..Default::default()
    };
    match handle.capture_element("https://example.com", "h1", &opts_elem).await {
        Ok(bytes) => {
            let path = format!("{}/element_h1.png", output_dir);
            std::fs::write(&path, &bytes).expect("write png");
            let size = bytes.len();
            if size > 100 && bytes[0..4] == [0x89, 0x50, 0x4E, 0x47] {
                println!("OK ({} bytes, valid PNG)", size);
                pass += 1;
            } else {
                println!("FAIL ({} bytes, invalid)", size);
                fail += 1;
            }
        }
        Err(e) => {
            println!("FAIL ({})", e);
            fail += 1;
        }
    }

    println!("");
    println!("  Results: {} passed, {} failed", pass, fail);

    if fail > 0 {
        std::process::exit(1);
    }
}
RUSTEOF

# Run the test via cargo with --bin trick: we create a temp binary crate
TEST_CRATE_DIR="/tmp/pardus-screenshot-test-crate"
rm -rf "$TEST_CRATE_DIR"
mkdir -p "$TEST_CRATE_DIR/src"
cp "$TEST_SRC" "$TEST_CRATE_DIR/src/main.rs"

# Get pardus-core path (relative from project root)
CORE_PATH="$(pwd)/crates/pardus-core"

cat > "$TEST_CRATE_DIR/Cargo.toml" << TOMLEOF
[package]
name = "pardus-screenshot-test"
version = "0.1.0"
edition = "2021"

[dependencies]
pardus-core = { path = "${CORE_PATH}", features = ["screenshot"] }
tokio = { version = "1", features = ["full"] }
TOMLEOF

echo -e "  ${YELLOW}Building test binary (this may take a moment)...${NC}"
BUILD_OUTPUT=$(cargo build --release --manifest-path "$TEST_CRATE_DIR/Cargo.toml" 2>&1)
if [ $? -ne 0 ]; then
    echo -e "  ${RED}Build failed:${NC}"
    echo "$BUILD_OUTPUT" | tail -15 | sed 's/^/      /'
    FAIL=$((FAIL + 1))
else
    echo -e "  ${GREEN}Test binary built${NC}"
    echo ""
    TEST_BIN="$TEST_CRATE_DIR/target/release/pardus-screenshot-test"
    if [ -f "$TEST_BIN" ]; then
        echo -e "${CYAN}[3] Executing screenshot tests${NC}"
        echo "─────────────────────────────────"
        echo ""
        if "$TEST_BIN" "$CHROME_PATH" "$TEST_OUTPUT_DIR" 2>&1; then
            PASS=$((PASS + 4))
            TOTAL=$((TOTAL + 4))
        else
            FAIL=$((FAIL + 4))
            TOTAL=$((TOTAL + 4))
        fi
    else
        echo -e "  ${RED}Test binary not found at ${TEST_BIN}${NC}"
        FAIL=$((FAIL + 1))
        TOTAL=$((TOTAL + 1))
    fi
fi
echo ""

# ── Step 3: Verify output files ──
echo -e "${CYAN}[4] Verifying output files${NC}"
echo "─────────────────────────────────"

EXPECTED_FILES=(
    "full_page.png"
    "viewport.png"
    "full_page.jpg"
    "element_h1.png"
)

for f in "${EXPECTED_FILES[@]}"; do
    filepath="${TEST_OUTPUT_DIR}/${f}"
    TOTAL=$((TOTAL + 1))
    if [ -f "$filepath" ]; then
        size=$(stat -f%z "$filepath" 2>/dev/null || stat -c%s "$filepath" 2>/dev/null)
        if [ "$size" -gt 100 ]; then
            echo -e "  ${GREEN}✓ ${f} (${size} bytes)${NC}"
            PASS=$((PASS + 1))
        else
            echo -e "  ${RED}✗ ${f} (${size} bytes — too small)${NC}"
            FAIL=$((FAIL + 1))
        fi
    else
        echo -e "  ${RED}✗ ${f} (missing)${NC}"
        FAIL=$((FAIL + 1))
    fi
done
echo ""

# ── Summary ──
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  Results: ${PASS} passed, ${FAIL} failed, ${TOTAL} total${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
echo ""
echo "  Output files saved to: ${TEST_OUTPUT_DIR}/"
echo ""

if [ $FAIL -gt 0 ]; then
    exit 1
fi
