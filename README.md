# pardus-browser

A headless browser built for AI agents. No pixels, no screenshots — just structured semantic state.

```
$ pardus-browser navigate https://example.com

00:00  pardus-browser navigate https://example.com
00:05  connected — parsing semantic state…
       document  [role: document]
       └── region  [role: region]
           ├── heading (h1)  "Example Domain"
           └── link  "Learn more"  → https://iana.org/domains/example
00:05  semantic tree ready — 0 landmarks, 1 links, 1 headings, 1 actions
00:05  agent-ready: structured state exposed · no pixel buffer · 0 screenshots
```

## Why

AI agents don't need screenshots. They need to know what's on a page, what they can interact with, and where they can go. `pardus-browser` fetches a URL, parses the HTML, and outputs a clean semantic tree — landmarks, headings, links, buttons, forms, and their actions — in milliseconds, not seconds.

No Chromium binary. No Docker. No GPU. Just HTTP + HTML parsing.

## Demo

[demo.mp4](demo/demo.mp4)

## Features

- **Semantic tree output** — ARIA roles, headings, landmarks, interactive elements
- **3 output formats** — Markdown (default), tree, JSON
- **Navigation graph** — Internal routes, external links, form descriptors with fields
- **Interactive-only mode** — Strip static content, show only actionable elements
- **Action annotations** — Every interactive element tagged with `navigate`, `click`, `fill`, `toggle`, or `select`
- **Fast** — HTTP GET + HTML parse, typically under 200ms
- **Zero dependencies on Chrome** — Pure Rust, no browser binary needed

## Install

From source (requires Rust 1.85+):

```bash
git clone https://github.com/user/pardus-browser.git
cd pardus-browser
cargo install --path crates/pardus-cli
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

# Enable JavaScript execution (for SPAs like React/Vue/Angular)
pardus-browser navigate https://example.com --js

# JS with custom wait time (ms) for async rendering
pardus-browser navigate https://example.com --js --wait-ms 5000

# Verbose logging
pardus-browser navigate https://example.com -v
```

### Output formats

**Markdown (default)** — clean semantic tree with role annotations:

```
document  [role: document]
├── banner  [role: banner]
│   ├── link "Home"  → /
│   ├── link "Products"  → /products
│   └── button "Sign In"
├── main  [role: main]
│   ├── heading (h1) "Welcome to Example"
│   ├── region "Hero"
│   │   ├── text "The fastest way to build"
│   │   └── link "Get Started"  → /signup
│   └── form "Search"  [role: form]
│       ├── textbox "Search..."  [action: fill]
│       └── button "Go"  [action: click]
└── contentinfo  [role: contentinfo]
    ├── link "Privacy"  → /privacy
    └── link "Terms"  → /terms
```

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
  }
}
```

### Clean cache

```bash
# Wipe everything
pardus-browser clean

# Only cookies
pardus-browser clean --cookies-only

# Only cache
pardus-browser clean --cache-only
```

## Architecture

```
pardus-browser
├── crates/pardus-core    Core library — HTML parsing, semantic tree, navigation graph
├── crates/pardus-cdp     CDP WebSocket server (planned)
└── crates/pardus-cli     CLI binary
```

**pardus-core** — The engine. Fetches pages via `reqwest`, parses HTML with `scraper`, builds a semantic tree mapping ARIA roles and interactive states. Outputs Markdown, tree, or JSON.

**pardus-cdp** — Chrome DevTools Protocol server (planned). Will expose a WebSocket endpoint for Playwright/Puppeteer integration, enabling JS-rendered pages and real-time interaction.

**pardus-cli** — The `pardus-browser` command-line tool.

## Semantic roles detected

| Element | Role | Action |
|---------|------|--------|
| `<a href>` | `link` | `navigate` |
| `<button>` | `button` | `click` |
| `<input type=text>` | `textbox` | `fill` |
| `<input type=submit>` | `button` | `click` |
| `<input type=checkbox>` | `checkbox` | `toggle` |
| `<select>` | `combobox` | `select` |
| `<textarea>` | `textbox` | `fill` |
| `<img>` | `img` | — |
| `<h1>`–`<h6>` | `heading (hN)` | — |
| `<nav>` | `navigation` | — |
| `<main>` | `main` | — |
| `<header>` | `banner` | — |
| `<footer>` | `contentinfo` | — |
| `<form>` | `form` | — |
| `<article>` | `article` | — |
| `<ul>/<ol>` | `list` | — |
| `<table>` | `table` | — |
| `[role=...]` | custom | varies |
| `[tabindex]` | varies | varies |

## Roadmap

- [x] **JavaScript execution** — V8 via deno_core with custom DOM ops
- [ ] **Full DOM API** — querySelector, event dispatching, complete Element API
- [ ] **CDP WebSocket server** — Playwright/Puppeteer compatible API
- [ ] **Page interaction** — Click, type, scroll, wait for selectors
- [ ] **Session persistence** — Cookies, localStorage, auth flows
- [ ] **Proxy support** — HTTP/SOCKS proxies
- [ ] **Screenshots** — Optional, for when pixels actually matter

## License

MIT License
