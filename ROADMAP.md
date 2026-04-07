# Pardus Browser Roadmap

**Version:** 0.4.0-dev | **Branch:** dev/roadmap | **Updated:** April 4, 2026

---

## Completed

Core engine, CLI, and all major subsystems are stable. Summary of shipped features:

| Area | Delivered |
|------|-----------|
| **Semantic Engine** | ARIA role tree, navigation graph, element IDs (`[#N]`), action annotations (navigate/click/fill/toggle/select), interactive-only mode, 4 output formats (md, tree, json, llm) |
| **Page Interaction** | Click, type, submit, wait-for-selector, scroll pagination, JS-level interaction (deno_core DOM), inline event handler registration, DOM mutation serialization |
| **JavaScript** | V8 via deno_core, 42+ Rust DOM ops, thread-based timeouts, inline script execution, analytics/problematic script filtering |
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
| **AI Agent Intelligence** | Action planning (page-type classification, suggested next actions), auto-form filling with validation, smart wait conditions (network idle, DOM stability, content mutations), session recording & replay (JSON serialization, deterministic replay) |
| **Anti-bot Detection** | Challenge detection (reCAPTCHA, hCaptcha, Turnstile, JS challenges), risk scoring, human-in-the-loop resolution |
| **Meta Refresh** | `<meta http-equiv="refresh">` parsing with delay, relative URLs, query params, fragments, base tag support, redirect depth limiting |

---

## In Progress

### Tauri Desktop App — Mission Control for AI Agents

**Priority: Urgent** — The desktop app is the primary interface for users to manage, monitor, and assist AI browsing agents.

**Phase 1 — Semantic Tree Viewer + CAPTCHA Handoff (current)**
- [ ] Semantic tree viewer panel — render ARIA role tree with interactive nodes in Tauri dashboard
- [x] Per-instance controls — URL bar, navigate, agent status (idle/running/waiting-challenge)
- [x] CAPTCHA handoff — when agent hits a challenge, popup OS webview (WKWebView/WebKitGTK/WebView2) for user to solve, then sync cookies back to headless browser via CDP `Network.setCookie`
- [x] Cookie bridge — `tokio-tungstenite` WebSocket client to inject cookies into headless CDP server
- [x] Agent action log — real-time log of agent actions (navigate, click, type, wait) streamed from CDP events
- [x] Cross-platform — dashboard is pure HTML/CSS (no OS webview dependency for primary view); CAPTCHA popup uses OS webview only when needed

**Phase 1.5 — Webview ↔ Headless Browser Sync (current)**
- [x] Click interceptor — JS injected into OS webview captures clicks on links, buttons, inputs and forwards to Pardus headless browser via Tauri events
- [x] CSS selector generator — produces unique selectors for any clicked DOM element (ID, name, type, nth-of-type)
- [x] Form input sync — debounced `input` event tracking syncs typed values to headless browser form state
- [x] Select/checkbox/radio change tracking — `change` events forwarded as `select`/`toggle` CDP actions
- [x] Form submission interception — `submit` events captured and forwarded as `submit` CDP actions
- [x] Navigation sync — when headless browser navigates after a forwarded action, OS webview is updated to the new URL
- [x] Href fallback — when CSS selector doesn't match in headless browser, falls back to direct URL navigation
- [x] Action log events — `webview-action-log` Tauri events emitted for frontend action log integration
- [x] Pardus UI guard — toolbar and challenge banner clicks are not intercepted

**Phase 2 — Multi-Agent Dashboard (current)**
- [x] Multiple concurrent agent instances — spawn/manage N agents in one window
- [x] Agent status grid — grid view with AgentCard components showing status, URL, last action per instance
- [x] Live agent action streaming — GlobalActionStream component aggregates actions across all instances with color-coding
- [x] Take-over button — pause agent (stop_agent), open browser window for manual interaction, resume via resume_agent command
- [x] Agent conversation panel — ChatPanel shows LLM conversation per instance (already existed)
- [x] Per-instance state — AgentContext restructured to Map<string, InstanceState> with per-instance messages, events, tree
- [x] Grid/detail view modes — grid overview (default for multi-instance) + drill-down detail view
- [x] View mode toggle — sidebar toggle between grid and detail views
- [x] Backend resume_agent command — Tauri command that sends message to agent sidecar and updates status
- [x] TakeOverBar component — orange banner during manual control with Resume/Open Browser buttons

**Phase 3 — Rendered View (Optional)**
- [ ] Rendered page tab — OS webview shows actual page pixels (WKWebView on macOS, WebKitGTK on Linux, WebView2 on Windows)
- [ ] Split view — semantic tree on left, rendered pixels on right
- [ ] Screenshot capture — use pardus-core screenshot feature (chromiumoxide) for pixel-perfect captures

**Architecture:**
```
┌─ Mission Control ──────────────────────────────────────┐
│ ┌─ Agents ─────┐  ┌─ Semantic Tree ──────────────────┐ │
│ │ ● Agent 1    │  │ [Document]                        │ │
│ │   Shopping   │  │  ├── [Nav] "Menu"                 │ │
│ │   Running    │  │  ├── [Main]                       │ │
│ │              │  │  │   ├── [H1] "Welcome"           │ │
│ │ ● Agent 2    │  │  │   ├── [TextBox #3] "Email"    │ │
│ │   Research   │  │  │   └── [Button #4] "Submit"    │ │
│ │   ⚠ CAPTCHA  │  │  └── [Footer]                     │ │
│ └──────────────┘  └───────────────────────────────────┘ │
│ ┌─ Action Log ────────────────────────────────────────┐ │
│ │ 12:03:01 Navigate → shop.example.com                │ │
│ │ 12:03:02 Click [#5] "Add to Cart"                   │ │
│ │ 12:03:03 ⚠ CAPTCHA detected — Cloudflare           │ │
│ └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

---

## Planned (Near-term)

### Screenshots (Optional)
- [x] HTML→PNG rendering — For when pixels actually matter
- [x] Element screenshots — Capture specific element bounds
- [x] Viewport clipping — Configurable resolution
- [x] CDP screenshot API — Page.captureScreenshot compliance

---

## Future Roadmap (2026+)

### AI Agent Intelligence

- [x] **Action planning** — Suggested next actions based on page state
- [x] **Auto-form filling** — AI-guided form completion with validation
- [x] **Smart wait conditions** — Wait for network idle, DOM stability, or content mutations instead of fixed timers
- [x] **Session recording & replay** — Serialize action sequences to JSON, replay deterministically
- [ ] **Page diff** — Compare semantic trees between navigations; detect what changed (new elements, removed content, state transitions)
- [x] **Anti-bot detection hints** — Report Cloudflare/PerimeterX/DataDome challenges in semantic output so agents know they're blocked
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
- [x] **Event dispatch** — Allow agents to fire arbitrary DOM events (change, input, submit, custom) for frameworks that listen on native events

### Network & Protocol

- [x] **Request interception** — Intercept, modify, or block requests before they're sent (URL rewrite, header injection, body substitution)
- [x] **Response mocking** — Return canned responses for specific URL patterns; useful for testing agents against controlled data
- [x] **Request deduplication** — Avoid parallel fetches of the same resource within a time window
- [x] **Retry with backoff** — Configurable retry policy for transient failures (5xx, timeout, connection reset)
- [x] **Cookie jar API** — Full programmatic cookie management (list, set, delete, domain filtering) via CLI, CDP, and library
- [ ] **Auth token rotation** — Auto-refresh expiring Bearer tokens when 401 is received; configurable refresh endpoint/callback

### Web Standards & Content

- [x] **PDF text extraction** — Parse PDF bytes to semantic tree with table, form-field (AcroForm), and image metadata extraction
- [x] **RSS/Atom feed parsing** — Detect and parse RSS/Atom feed content into structured items (title, link, date, summary)
- [ ] **Robots.txt parser** — Respect crawl directives; expose `is_allowed(url)` for the knowledge graph crawler
- [x] **Meta refresh & redirects** — Parse `<meta http-equiv="refresh">` and JS `location.href` assignments as navigations
- [ ] **Content encoding** — Handle gzip/brotli/zstd transfer encodings beyond what reqwest provides automatically

### CDP Completeness

- [x] **DOM manipulation** — Implement stubbed methods: setNodeValue, setNodeName, removeAttribute, copyTo, moveTo, undo/redo
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
