# pardus-browser

A headless browser built for AI agents. No pixels, no screenshots — just structured semantic state.

```
$ pardus-browser navigate https://example.com

00:00  pardus-browser navigate https://example.com
00:05  connected — parsing semantic state…
       document  [role: document]
       └── region  [role: region]
           ├── heading (h1)  "Example Domain"
           └── [#1] link  "Learn more"  → https://iana.org/domains/example
00:05  semantic tree ready — 0 landmarks, 1 links, 1 headings, 1 actions
00:05  agent-ready: structured state exposed · no pixel buffer · 0 screenshots
```

**Element IDs** — Interactive elements are tagged with unique IDs (`[#1]`, `[#2]`, etc.) that AI agents can use to reference them. This makes it easy for agents to interact with specific elements without needing to understand CSS selectors.

## Why

AI agents don't need screenshots. They need to know what's on a page, what they can interact with, and where they can go. `pardus-browser` fetches a URL, parses the HTML, and outputs a clean semantic tree — landmarks, headings, links, buttons, forms, and their actions — in milliseconds, not seconds.

No Chromium binary. No Docker. No GPU. Just HTTP + HTML parsing.

## Features

- **Semantic tree output** — ARIA roles, headings, landmarks, interactive elements
- **Element IDs** — Unique IDs for interactive elements (e.g., `[#1]`, `[#2]`) that AI agents can use for easy reference
- **Page interaction** — Click links, submit forms, type into fields, wait for selectors, scroll
- **3 output formats** — Markdown (default), tree, JSON
- **Navigation graph** — Internal routes, external links, form descriptors with fields
- **Interactive-only mode** — Strip static content, show only actionable elements
- **Action annotations** — Every interactive element tagged with `navigate`, `click`, `fill`, `toggle`, or `select`
- **Network debugger** — DevTools-style request table with subresource discovery and parallel fetching
- **Session persistence** — Cookies, headers, localStorage across requests
- **CDP server** — Chrome DevTools Protocol WebSocket endpoint for automation (14 domains)
- **Knowledge Graph** — Site-level state map: BFS crawl produces a graph of view-states (semantic + network fingerprints) and verified transitions (link clicks, hash nav, pagination)
- **PDF extraction** — Navigate to PDF URLs and get a semantic tree back: per-page text extraction with heading detection, no external dependencies
- **JavaScript execution** — Optional V8 via deno_core with DOM ops (enabled by default, see known issues)
- **Persistent REPL** — Interactive session with persistent state across commands
- **Tab management** — Multiple tabs with independent history and state
- **Fast** — HTTP GET + HTML parse, typically under 200ms
- **Zero dependencies on Chrome** — Pure Rust, no browser binary needed

## Install

From source (requires Rust nightly):

```bash
# Install nightly toolchain
rustup install nightly

# Clone and build
git clone https://github.com/user/pardus-browser.git
cd pardus-browser
cargo +nightly install --path crates/pardus-cli --feature js 
```

### Docker

```bash
docker build -t pardus-browser .
docker run --rm pardus-browser navigate https://example.com
```

## Usage

### Navigate to a URL

```bash
# Default: Markdown tree
pardus-browser navigate https://example.com

# Raw tree format
pardus-browser navigate https://example.com --format tree

# JSON with navigation graph
pardus-browser navigate https://example.com --format json --with-nav

# Only interactive elements
pardus-browser navigate https://example.com --interactive-only

# Custom headers
pardus-browser navigate https://api.example.com --header "Authorization: Bearer token"

# Enable JavaScript execution (improved — problematic scripts are now filtered)
pardus-browser navigate https://example.com --js

# JS with custom wait time (ms) for async rendering
pardus-browser navigate https://example.com --js --wait-ms 5000

# Verbose logging
pardus-browser navigate https://example.com -v

# Capture and display network request table
pardus-browser navigate https://example.com --network-log

# Network log with JSON output
pardus-browser navigate https://example.com --format json --network-log
```

### PDF viewing

Navigate to a PDF URL the same way you'd navigate to an HTML page. The browser detects `application/pdf` responses, extracts text per-page, and builds a semantic tree with heading detection.

```bash
pardus-browser navigate https://example.com/report.pdf
```

```
00:00  pardus-browser navigate https://example.com/report.pdf
00:01  connected — parsing semantic state…
       document  "Annual Report 2026"  [role: document]
       ├── heading (h1)  "Annual Report 2026"
       ├── text  "This report summarizes our financial performance..."
       ├── heading (h2)  "Revenue"
       ├── text  "Revenue increased by 15% year-over-year..."
       ├── heading (h2)  "Expenses"
       └── text  "Operating expenses decreased due to efficiency gains..."
00:01  semantic tree ready — 0 landmarks, 0 links, 3 headings, 0 actions
```

Works with all output formats and subcommands:

```bash
# JSON output
pardus-browser navigate https://example.com/report.pdf --format json

# Tree format
pardus-browser navigate https://example.com/report.pdf --format tree
```

**How it works:**

1. **Content-type detection** — Responses with `application/pdf` are routed to PDF extraction instead of HTML parsing
2. **Text extraction** — Uses `pdf-extract` (lopdf) to extract text per page
3. **Heading detection** — Heuristics classify blocks as headings: first block on page (h1/title), ALL CAPS text, short text without sentence-ending punctuation (h2)
4. **Semantic tree** — Each page becomes a `region` node (named "Page N" for multi-page PDFs), text blocks become `text` nodes, headings become `heading` nodes
5. **Zero config** — No flags needed, works automatically on PDF URLs

### Output formats

**Markdown (default)** — clean semantic tree with role annotations and element IDs:

```
document  [role: document]
├── banner  [role: banner]
│   ├── [#1] link "Home"  → /
│   ├── [#2] link "Products"  → /products
│   └── [#3] button "Sign In"
├── main  [role: main]
│   ├── heading (h1) "Welcome to Example"
│   ├── region "Hero"
│   │   ├── text "The fastest way to build"
│   │   └── [#4] link "Get Started"  → /signup
│   └── form "Search"  [role: form]
│       ├── [#5] textbox "Search..."  [action: fill]
│       └── [#6] button "Go"  [action: click]
└── contentinfo  [role: contentinfo]
    ├── [#7] link "Privacy"  → /privacy
    └── [#8] link "Terms"  → /terms
```

Each interactive element has a unique ID in brackets (`[#1]`, `[#2]`, etc.) that can be used with `click-id` and `type-id` commands.

**JSON** — structured data with full navigation graph:

```bash
pardus-browser navigate https://example.com --format json --with-nav
```

Returns:

```json
{
  "url": "https://example.com/",
  "title": "Example Domain",
  "semantic_tree": {
    "root": { "role": "document", "children": [...] },
    "stats": { "landmarks": 4, "links": 12, "headings": 3, "actions": 2 }
  },
  "navigation_graph": {
    "internal_links": [
      { "url": "/products", "label": "Products" },
      { "url": "/signup", "label": "Get Started" }
    ],
    "external_links": ["https://github.com/..."],
    "forms": [
      {
        "action": "/search",
        "method": "GET",
        "fields": [
          { "name": "q", "field_type": "text", "action": "fill" },
          { "name": "go", "field_type": "submit", "action": "click" }
        ]
      }
    ]
  },
  "network_log": {
    "total_requests": 4,
    "total_bytes": 6432,
    "total_time_ms": 312,
    "failed": 0,
    "requests": [
      {
        "id": 1, "method": "GET", "type": "document",
        "initiator": "navigation", "description": "document · navigation",
        "url": "https://example.com/", "status": 200,
        "content_type": "text/html", "body_size": 4304, "timing_ms": 142
      }
    ]
  }
}
```

### Network debugger

Capture and display all network requests in a DevTools-style table:

```bash
pardus-browser navigate https://example.com --network-log
```

```
00:00  pardus-browser navigate https://example.com
00:00  connected — parsing semantic state…
       # Network — 4 requests — 4.6 KB — 312ms total

         Method  Type        Resource                URL                                         Status  Size     Time
         —       ——————       —————————                 —————————————————                               ——————   ————————   ——————
         1       GET         document                 document · navigation                        200     4.2 KB   142ms
         2       GET         stylesheet               stylesheet · css2                            200     128 B    45ms
         3       GET         stylesheet               stylesheet · styles.css                      200     2.1 KB   89ms
         4       GET         script                   script · script.js                           200     0 B      23ms
00:00  semantic tree ready — 0 landmarks, 1 links, 1 headings, 1 actions
00:00  agent-ready: structured state exposed · no pixel buffer · 0 screenshots
```

The network debugger:
- Records the main page request (status, timing, size, headers)
- Discovers all subresources from HTML (`<link>`, `<script>`, `<img>`, `<video>`, `<audio>`, `<iframe>`, `<embed>`, `<object>`, inline CSS `url()`)
- Fetches all discovered subresources in parallel (concurrency limit of 6)
- Includes `network_log` in JSON output when using `--format json --network-log`

### CDP server

Start a Chrome DevTools Protocol WebSocket server for automation:

```bash
# Start on default host/port
pardus-browser serve

# Custom host and port
pardus-browser serve --host 0.0.0.0 --port 9222

# With inactivity timeout
pardus-browser serve --timeout 60
```

Implemented CDP domains: Browser, Target, Page, Runtime, DOM, Network, Emulation, Input, CSS, Log, Console, Security, Performance, Pardus (custom extensions)

### Knowledge Graph (site mapping)

Map a site's functional structure into a deterministic state graph. Nodes are view-states (semantic tree hash + resource fingerprint), edges are verified transitions.

```bash
# Map a site (default: depth 3, max 50 pages)
pardus-browser map https://example.com --output kg.json

# Shallow crawl
pardus-browser map https://example.com --depth 1 --output kg.json

# Deep crawl with higher page limit
pardus-browser map https://example.com --depth 5 --max-pages 200 --output kg.json

# Skip pagination discovery (only follow links)
pardus-browser map https://example.com --output kg.json --no-pagination

# Verbose logging
pardus-browser map https://example.com -v --output kg.json
```

**Output** — JSON with all view-states, transitions, and stats:

```json
{
  "root_url": "https://example.com",
  "built_at": "2026-04-02T14:30:00Z",
  "stats": {
    "total_states": 12,
    "total_transitions": 23,
    "verified_transitions": 21,
    "max_depth_reached": 3,
    "pages_crawled": 12,
    "crawl_duration_ms": 5420
  },
  "states": {
    "a1b2c3...": {
      "url": "https://example.com/",
      "title": "Example Corp",
      "fingerprint": {
        "url_path": "/",
        "tree_hash": "def456...",
        "resource_set_hash": "789abc..."
      },
      "semantic_tree": { ... },
      "resource_urls": ["https://example.com/styles.css", ...]
    }
  },
  "transitions": [
    {
      "from": "a1b2c3...",
      "to": "d4e5f6...",
      "trigger": { "type": "link_click", "url": "/about", "label": "About Us" },
      "verified": true,
      "outcome": { "status": 200, "final_url": "https://example.com/about", "matched_prediction": true }
    },
    {
      "from": "a1b2c3...",
      "to": "a1b2c3...",
      "trigger": { "type": "hash_navigation", "fragment": "features", "label": "Features" },
      "verified": true
    }
  ]
}
```

**How it works:**

1. **BFS crawl** — Starting from the root URL, visits pages breadth-first up to `--depth` and `--max-pages`
2. **State fingerprinting** — Each page gets a composite ID: blake3 hash of semantic tree structure (roles + interactivity, not text) + resource URLs + URL path
3. **Deduplication** — Pages with identical fingerprints are merged (same layout, different copy = same state)
4. **Transition discovery** — For each page, discovers: link clicks, hash navigation (`#section`), pagination (`?page=N`, `/page/N`), and optional form submissions
5. **Verification** — Each transition is followed and the target state is confirmed

**Transition types:**

| Type | Trigger | Example |
|------|---------|---------|
| `link_click` | Click internal link | `<a href="/about">About</a>` |
| `hash_navigation` | Hash/anchor link | `<a href="#features">Features</a>` |
| `pagination` | URL-based pagination | `?page=2`, `/page/2`, `?offset=20` |
| `form_submit` | Form submission | `<form action="/search">` |

### Clean cache

```bash
# Wipe everything
pardus-browser clean

# Only cookies
pardus-browser clean --cookies-only

# Only cache
pardus-browser clean --cache-only

# Custom cache directory
pardus-browser clean --cache-dir /path/to/cache
```

### Tab management

```bash
# Open a new tab (fetches page and shows summary)
pardus-browser tab open https://example.com

# Open with JS execution
pardus-browser tab open https://example.com --js

# List all open tabs
pardus-browser tab list

# Show active tab info
pardus-browser tab info

# Navigate the active tab
pardus-browser tab navigate https://example.com/page2
```

**Note:** Tab state does not persist across CLI invocations. For persistent tab sessions, use the REPL or the CDP server.

### Interactive REPL

Start a persistent interactive session where browser state (tabs, pages, cookies, history) is preserved across commands:

```bash
# Start REPL with defaults
pardus-browser repl

# Enable JS execution by default
pardus-browser repl --js

# Set default output format and JS wait time
pardus-browser repl --format json --wait-ms 5000
```

Once inside the REPL, the prompt shows the current URL context:

```
pardus> visit https://example.com
  document  [role: document]
  └── region  [role: region]
      ├── heading (h1)  "Example Domain"
      └── link  "Learn more"  → https://iana.org/domains/example
  0 landmarks, 1 links, 1 headings, 1 actions

pardus [https://example.com]> tab open https://httpbin.org
Opened tab 2: httpbin.org

pardus [https://httpbin.org]> tab list
Tabs (2 total):
  * [2] Ready — httpbin.org — https://httpbin.org
    [1] Ready — Example Domain — https://example.com

pardus [https://httpbin.org]> tab switch 1
Switched to tab 1: https://example.com

pardus [https://example.com]> click 'a'
Navigated to: https://iana.org/domains/example

pardus [https://iana.org/domains/example]> back
pardus [https://example.com]> exit
Bye.
```

**REPL commands:**

| Command | Description |
|---------|-------------|
| `visit <url>` / `open <url>` | Navigate to URL |
| `click <selector>` | Click an element using CSS selector |
| `click #<id>` | Click an element by its ID (e.g., `click #1`) |
| `type <selector> <value>` | Type into a field using CSS selector |
| `type #<id> <value>` | Type into a field by its ID (e.g., `type #3 hello`) |
| `submit <selector> [name=value...]` | Submit a form |
| `scroll [down\|up\|to-top\|to-bottom]` | Scroll the page |
| `wait <selector> [timeout_ms]` | Wait for element |
| `back` / `forward` | Navigate history |
| `reload` | Reload current page |
| `tab list` / `tab open <url>` / `tab switch <id>` / `tab close [id>` / `tab info` | Tab management |
| `js [on\|off]` | Toggle JS execution |
| `format md\|tree\|json` | Change output format |
| `wait-ms <ms>` | Set JS wait time |
| `help` | Show available commands |
| `exit` / `quit` | Exit REPL |

### Programmatic usage

The `Browser` type unifies navigation, interaction, and tab management into a single API:

```rust
use pardus_core::Browser;

let mut browser = Browser::new(BrowserConfig::default());

// Navigate (creates a tab automatically)
let tab = browser.navigate("https://example.com").await?;

// Interact using CSS selectors — click updates the tab automatically if navigation occurs
let result = browser.click("a").await?;

// Interact using element IDs — easier for AI agents
let result = browser.click_by_id(1).await?;  // Click element with ID [#1]
let result = browser.type_by_id(3, "search query").await?;  // Type into element [#3]

// Chain interactions
browser.type_text("input[name='q']", "search query")?;
browser.submit("form", &state).await?;

// Tab management
let id = browser.create_tab("https://example.com/page2");
browser.switch_to(id).await?;
browser.go_back().await?;

// Access current state
let page = browser.current_page().unwrap();
let tree = page.semantic_tree();

// Find element by ID
if let Some(element) = page.find_by_element_id(1) {
    println!("Element selector: {}", element.selector);
}
```

### Page interaction

Interact with pages using the `interact` subcommand. Works at the HTTP level — clicks follow links and submit forms, no rendering engine required.

```bash
# Click a link — follows href, returns new page
pardus-browser interact https://example.com click 'a'

# Click by element ID — easier for AI agents
pardus-browser interact https://example.com click-id 1

# Click a submit button — finds enclosing form, submits it
pardus-browser interact https://example.com click 'button[type="submit"]'

# Type into a field (returns the field state)
pardus-browser interact https://example.com type 'input[name="q"]' 'search query'

# Type by element ID — easier for AI agents
pardus-browser interact https://example.com type-id 3 'search query'

# Submit a form with field values
pardus-browser interact https://example.com submit 'form' --field 'q=rust+language'

# Wait for a CSS selector to appear (with timeout)
pardus-browser interact https://example.com wait '.result-list' --timeout-ms 5000

# Scroll — detects URL pagination (?page=, ?offset=, /page/N)
pardus-browser interact 'https://example.com/news?page=1' scroll --direction down

# JSON output for the result page
pardus-browser interact https://example.com click 'a' --format json

# Enable JS execution before interaction
pardus-browser interact https://example.com wait '.dynamic-content' --js --wait-ms 3000
```

**How interactions work:**

| Action | Mechanism |
|--------|-----------|
| `click` (link) | Resolves href, HTTP GET, returns new page |
| `click` (button) | Finds enclosing `<form>`, collects all fields (including hidden CSRF tokens), submits via HTTP |
| `type` | Returns field selector + value (accumulate in `FormState` before submit) |
| `submit` | Collects all form fields from HTML, merges with `--field` values, HTTP POST/GET |
| `wait` | Checks current HTML for selector match; polls by re-fetching if not found |
| `scroll` | Detects pagination patterns in URL (`?page=`, `?offset=`, `?start=`, `/page/N`) |

## Architecture

```
pardus-browser
├── crates/pardus-core    Core library — Browser type, HTML parsing, semantic tree, navigation graph, interaction, tabs
├── crates/pardus-debug   Network debugger — request recording, subresource discovery, table output
├── crates/pardus-cdp     CDP WebSocket server — Chrome DevTools Protocol for automation (14 domains)
├── crates/pardus-kg      Knowledge Graph — BFS site crawler, state fingerprinting, transition discovery
└── crates/pardus-cli     CLI binary
```

**pardus-core** — The engine. The `Browser` type is the main entry point — it owns the HTTP client, tab state, and provides navigation + interaction as a single cohesive API. Internally, it fetches pages via `reqwest`, parses HTML with `scraper`, and builds semantic trees mapping ARIA roles and interactive states. PDF URLs are detected by content-type and extracted into semantic trees via `pdf-extract`. Provides page interaction (click, type, submit, wait, scroll) with automatic tab updates on navigation. Includes tab management, history navigation, session persistence (cookies, headers, localStorage), and optional JavaScript execution via deno_core (enabled by default). Outputs Markdown, tree, or JSON.

**pardus-debug** — Network debugging. Records all HTTP requests to a shared `NetworkLog`, discovers subresources from parsed HTML (stylesheets, scripts, images, fonts, media), fetches them in parallel, and formats DevTools-style request tables.

**pardus-cdp** — Chrome DevTools Protocol server. Exposes a WebSocket endpoint for browser automation with 14 domain handlers (Browser, Target, Page, Runtime, DOM, Network, Emulation, Input, CSS, Log, Console, Security, Performance, Pardus). Includes event bus, target management, message routing, and session lifecycle.

**pardus-kg** — Knowledge Graph. BFS site crawler that builds a deterministic state map: nodes are view-states identified by composite fingerprints (semantic tree structure hash + resource URL set hash + normalized URL), edges are verified transitions (link clicks, hash navigation, pagination). Produces a JSON graph suitable for AI agent consumption — an agent can query the graph to understand what states exist and how to reach them without trial-and-error navigation.

**pardus-cli** — The `pardus-browser` command-line tool. Provides `navigate`, `interact`, `map`, `tab`, `serve`, `repl`, and `clean` subcommands. All commands use the unified `Browser` type.

## Semantic roles detected

| Element | Role | Action |
|---------|------|--------|
| `<html>` / `<body>` | `document` | — |
| `<header>` | `banner` | — |
| `<nav>` | `navigation` | — |
| `<main>` | `main` | — |
| `<aside>` | `complementary` | — |
| `<footer>` | `contentinfo` | — |
| `<section>` / `[role=region]` | `region` | — |
| `<form>` | `form` | — |
| `<form role=search>` | `search` | — |
| `<article>` | `article` | — |
| `<h1>`–`<h6>` | `heading (hN)` | — |
| `<a href>` | `link` | `navigate` |
| `<button>` | `button` | `click` |
| `<input type=text/email/...>` | `textbox` | `fill` |
| `<input type=submit>` | `button` | `click` |
| `<input type=checkbox>` | `checkbox` | `toggle` |
| `<input type=radio>` | `radio` | `toggle` |
| `<select>` | `combobox` | `select` |
| `<textarea>` | `textbox` | `fill` |
| `<img>` | `img` | — |
| `<ul>` / `<ol>` | `list` | — |
| `<li>` | `listitem` | — |
| `<table>` | `table` | — |
| `<tr>` | `row` | — |
| `<td>` | `cell` | — |
| `<th>` | `columnheader` / `rowheader` | — |
| `<dialog>` | `dialog` | — |
| `[role=...]` | custom | varies |
| `[tabindex]` | varies | varies |

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full project roadmap, including:

- ✅ **Completed features** — Semantic tree, CDP server, JS execution, REPL, tab management, Knowledge Graph, PDF extraction
- 🔧 **In progress** — CDP ↔ Browser API integration, JS-level interactions
- 📋 **Near-term** — Proxy support, screenshots, KG-driven agent loop
- 🚀 **Future** — AI agent features, performance, WebSocket/SSE, bindings for Python/Node.js

## Known Issues

| Issue | Status | Workaround |
|-------|--------|------------|
| ~~JS execution hangs on complex sites~~ | **Fixed** | ~~Don't use `--js` flag~~ |
| External scripts not executed | By design | Only inline scripts supported |
| setTimeout/setInterval no-ops | By design | Prevents infinite loops |

## Requirements

- **Rust nightly** required (deno_core uses `const_type_id` feature)
- Install: `rustup install nightly`

## License

MIT License
