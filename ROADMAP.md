# Pardus Browser Roadmap

**Version:** 0.4.0-dev | **Branch:** dev/roadmap | **Updated:** April 3, 2026

---

## Completed

Core engine, CLI, and all major subsystems are stable. Summary of shipped features:

| Area | Delivered |
|------|-----------|
| **Semantic Engine** | ARIA role tree, navigation graph, element IDs (`[#N]`), action annotations (navigate/click/fill/toggle/select), interactive-only mode, 4 output formats (md, tree, json, llm) |
| **Page Interaction** | Click, type, submit, wait-for-selector, scroll pagination, JS-level interaction (deno_core DOM), inline event handler registration, DOM mutation serialization |
| **JavaScript** | V8 via deno_core, 35+ Rust DOM ops, thread-based timeouts, inline script execution, analytics/problematic script filtering |
| **Security** | SSRF protection (private IPs, metadata endpoints, scheme blocking), Basic/Bearer auth, CSP parsing & enforcement, certificate pinning (SPKI hash + CA), sandbox mode (off/strict/moderate/minimal) |
| **Session & Cache** | Cookie/localStorage/auth persistence, HTTP cache (RFC 7234: ETag, Last-Modified, 304), disk cache, shared HTTP client factory |
| **Proxy** | HTTP/HTTPS/SOCKS5, per-command flags, env var support (HTTP_PROXY etc.), no-proxy exclusions |
| **Tabs** | Multi-tab with independent state, history navigation, activation, list/info |
| **Network** | DevTools-style request table, parallel subresource fetch, full request/response logging, HAR 1.2 export, CSS/JS coverage reporting |
| **SSE** | Streaming parser (HTML Living Standard), async client with auto-reconnect, thread-safe manager, JS EventSource API, 4 deno_core ops, SSRF validation |
| **WebSocket** | WS/WSS via tokio-tungstenite, connection pooling, per-origin limits, CDP events |
| **CDP Server** | WebSocket endpoint on ws://127.0.0.1:9222, 14 domain handlers (Browser, Target, Page, DOM, Network, Runtime, Input, CSS, Console, Log, Security, Emulation, Performance, Pardus), event bus, node mapping |
| **Knowledge Graph** | BFS crawler, blake3 fingerprinting, transition discovery (links/hash/pagination), JSON graph output |
| **Frames** | Recursive iframe/frame parsing with depth limits, sandbox token awareness, iframe-aware semantic tree |
| **Shadow DOM** | Shadow boundary piercing (query_selector_deep, query_selector_all_deep) |
| **Adapters** | Playwright (Python + Node.js), Puppeteer (Node.js), Docker image with health check |
| **CLI** | 8 subcommands (navigate, interact, serve, repl, tab, map, clean), rustyline REPL, verbose logging |
| **Perf** | Connection pooling, HTTP/2 push simulation, configurable memory limits, ~200ms page parse |

---

## In Progress

_(Currently empty)_

---

## Planned (Near-term)

### Screenshots (Optional)
- [ ] HTML→PNG rendering — For when pixels actually matter
- [ ] Element screenshots — Capture specific element bounds
- [ ] Viewport clipping — Configurable resolution
- [ ] CDP screenshot API — Page.captureScreenshot compliance

---

## Future Roadmap (2026+)

### AI Agent Intelligence

- [ ] **Action planning** — Suggested next actions based on page state
- [ ] **Auto-form filling** — AI-guided form completion with validation
- [ ] **Smart wait conditions** — Wait for network idle, DOM stability, or content mutations instead of fixed timers
- [ ] **Session recording & replay** — Serialize action sequences to JSON, replay deterministically
- [ ] **Page diff** — Compare semantic trees between navigations; detect what changed (new elements, removed content, state transitions)
- [ ] **Anti-bot detection hints** — Report Cloudflare/PerimeterX/DataDome challenges in semantic output so agents know they're blocked
- [ ] **Login flow templates** — Declarative YAML/JSON descriptors for common auth patterns (email+password, SSO click-through, MFA TOTP)
- [ ] **Content extraction** — Article/main-content extraction (Readability-style) stripping nav, ads, footers; output clean text for LLM ingestion
- [ ] **Structured data extraction** — Detect and expose JSON-LD, Open Graph, microdata, RDFa from pages as typed Rust structs

### Agent Orchestration & Multi-Step

- [ ] **Workflow engine** — Chain page visits, interactions, and conditional branching into reusable pipelines
- [ ] **Parallel crawling** — Concurrent page visits with configurable concurrency limits and per-domain rate limiting
- [ ] **State checkpointing** — Save/restore full browser state (cookies, localStorage, tabs, history) to disk for resumable sessions
- [ ] **Agent context window management** — Automatically truncate or summarize older page states to fit LLM context limits during long sessions

### JS & DOM Completeness

- [ ] **Shadow DOM in JS runtime** — Wire shadow boundary piercing into deno_core ops; currently only works in Rust-side parsing
- [ ] **External script execution** — Fetch and execute `<script src="...">` with same timeout/filtering as inline scripts
- [ ] **fetch/XHR in JS** — Full `window.fetch` / `XMLHttpRequest` implementation that routes through pardus-core's HTTP client with cache and SSRF enforcement
- [ ] **Cookie API in JS** — `document.cookie` getter/setter wired to the session cookie store
- [ ] **localStorage/sessionStorage in JS** — Persistent and per-session storage backed by pardus-core session store
- [ ] **MutationObserver shim** — Allow JS to observe DOM changes for SPA reactivity detection
- [ ] **Event dispatch** — Allow agents to fire arbitrary DOM events (change, input, submit, custom) for frameworks that listen on native events

### Network & Protocol

- [ ] **Request interception** — Intercept, modify, or block requests before they're sent (URL rewrite, header injection, body substitution)
- [ ] **Response mocking** — Return canned responses for specific URL patterns; useful for testing agents against controlled data
- [ ] **Request deduplication** — Avoid parallel fetches of the same resource within a time window
- [ ] **Retry with backoff** — Configurable retry policy for transient failures (5xx, timeout, connection reset)
- [ ] **Cookie jar API** — Full programmatic cookie management (list, set, delete, domain filtering) via CLI, CDP, and library
- [ ] **Auth token rotation** — Auto-refresh expiring Bearer tokens when 401 is received; configurable refresh endpoint/callback

### Web Standards & Content

- [ ] **PDF text extraction** — Parse PDF bytes to semantic tree (already partially implemented in `pdf.rs`); extend with table, form-field, and image extraction
- [ ] **RSS/Atom feed parsing** — Detect and parse feed content into structured items (title, link, date, summary)
- [ ] **Robots.txt parser** — Respect crawl directives; expose `is_allowed(url)` for the knowledge graph crawler
- [ ] **Meta refresh & redirects** — Parse `<meta http-equiv="refresh">` and JS `location.href` assignments as navigations
- [ ] **Content encoding** — Handle gzip/brotli/zstd transfer encodings beyond what reqwest provides automatically

### CDP Completeness

- [ ] **DOM manipulation** — Implement stubbed methods: setNodeValue, setNodeName, removeAttribute, copyTo, moveTo, undo/redo
- [ ] **Input event dispatch** — Wire mouse/keyboard events through pardus-core interaction system (currently stubbed)
- [ ] **File upload** — Implement DOM.setFileInputFiles for `<input type="file">` handling
- [ ] **Network interception in CDP** — Fetch.enable / Fetch.requestPaused for request/response modification over CDP
- [ ] **Runtime console API** — Full console.log/warn/error capture with argument serialization
- [ ] **Coverage in CDP** — CSS.stopRuleUsageTracking, Pardus.getCoverage return real data (currently only CLI)

### Performance & Reliability

- [ ] **Streaming HTML parser** — Parse HTML as bytes arrive instead of waiting for full response; reduce time-to-first-semantic-node
- [ ] **Incremental semantic tree updates** — When JS modifies the DOM, recompute only the changed subtree instead of full rebuild
- [ ] **Request prioritization** — Resource scheduler with priority classes (document > CSS > JS > images) and concurrency limits
- [ ] **Configurable timeouts per phase** — Separate timeouts for DNS, TLS handshake, TTFB, body download
- [ ] **Graceful degradation** — Return partial results on timeout instead of failing; e.g., "page partially loaded, 3 of 10 resources fetched"

### Security & Authentication

- [ ] **OAuth 2.0 / OIDC flow** — Authorization code flow with PKCE; token exchange and refresh automation
- [ ] **mTLS support** — Client certificate authentication for enterprise APIs
- [ ] **Redirect chain audit** — Log full redirect hops with status codes; detect open redirects and suspicious chains

### API & Integration

- [ ] **Python bindings** — PyO3 wrapper for Python agents
- [ ] **Node.js bindings** — N-API for JavaScript agents
- [ ] **Library crate API** — Stable public Rust API (`pardus-core` as a library) with `no_std`-friendly semantic types for embedding
- [ ] **Webhook notifications** — POST page state to a configurable URL on navigation, interaction, or error events

### Developer Experience

- [ ] **Accessibility audit** — Automated a11y checks (missing alt text, contrast issues, ARIA violations, heading order)
- [ ] **Visual regression** — Diff screenshots for testing
- [ ] **REPL improvements** — Auto-completion, syntax highlighting, multi-line input
- [ ] **Structured error types** — Typed errors with codes, recovery hints, and machine-readable JSON output
- [ ] **Configuration file** — `pardus.toml` for persistent settings (proxy, headers, sandbox, CSP) instead of CLI flags only
- [ ] **Plugin system** — Loadable WASM or shared-library plugins for custom extractors, interceptors, or output formatters
- [ ] **Benchmarking harness** — Automated perf regression tracking across releases (page parse time, JS execution, memory)

---

## Metrics & Targets

| Metric | Current | Target |
|--------|---------|--------|
| Cold start | ~50ms | <20ms |
| Page parse (typical) | ~150ms | <80ms |
| Streaming first node | N/A | <50ms after TTFB |
| JS execution timeout | 3s fixed | Per-script configurable |
| CDP method coverage | ~60% | >90% |
| CDP domains | 14 | 16+ (add Fetch, DOMSnapshot) |
| Test count | ~741 | 1000+ |
| Binary size | ~3.7MB | <8MB |
| Memory per tab | Unbounded | Configurable hard cap |

---

## Known Issues

| Issue | Status | Workaround |
|-------|--------|------------|
| External scripts not executed | By design | Only inline scripts supported |
| setTimeout/setInterval no-ops | By design | Prevents infinite loops |
| Complex SPA interactions | Partial | Use `--wait-ms` for async content |

---

*For contributing to the roadmap, open an issue with the `roadmap` label.*
