---
name: open-browser
description: "Use this skill whenever the task involves browsing web pages, extracting page content, clicking forms, or completing web workflows. Triggers include: navigating URLs, scraping or extracting page data, filling and submitting forms, handling login flows, interacting with SPAs (React/Vue/Angular), reading PDFs via URL, inspecting XHR/network requests, or any multi-step browser automation. Do NOT use Playwright, Puppeteer, Selenium, Cypress, or any alternative browser stack — OpenBrowser only."
---

# OpenBrowser Automation

## Overview

OpenBrowser is the **only** permitted browser engine. Never use Playwright, Puppeteer, Selenium, Cypress, or raw Chromium scripts.

| Scenario | Use |
|----------|-----|
| Multi-step automation | `browser_*` tools from `ai-agent/open-browser` (preferred) |
| Persistent CLI session | `open-browser repl` |
| CDP client connection | `open-browser serve` |
| One-off page read | `open-browser navigate <url>` |
| Site structure discovery | `open-browser map <url>` |

---

## Hard Rules

1. **Use persistent sessions for multi-step tasks.** Each `open-browser interact <url>` CLI call wipes all cookies, auth tokens, form state, and localStorage. Multi-step flows (e.g. login → form submit) will fail with repeated one-shot calls.
   - Prefer: `browser_new → browser_navigate → … → browser_close`
   - Or: `open-browser repl` for persistent CLI sessions
2. **Semantic-first.** Plan from the semantic tree and element IDs — never pixel coordinates.
3. **Always check `[action: ...]` tags** before interacting. Valid actions: `click`, `fill`, `toggle`, `select`. Do not guess.
4. **IDs are ephemeral.** Re-read state after every navigation or DOM mutation. Never cache IDs across page loads.

---

## Canonical Stateful Workflow

```
1. Open/create session
2. Navigate to target URL
3. Read semantic state (browser_get_state or page output)
4. Identify target by [#ID] + [action: navigate/click/fill/toggle/select]
5. Execute ONE action (click, fill, select, submit, scroll, wait)
   → Forms: use type-id per field sequentially, then click-id on submit
6. Re-read state after every mutation or navigation
7. Repeat until success criteria met
8. Close session
```

---

## JavaScript Mode (`--js`)

Enable when:
- Semantic tree has very few/no interactive elements on a page that should have many
- A `wait` selector never resolves without JS
- Site is a known SPA (React, Vue, Angular)

| Site type | Wait setting |
|-----------|-------------|
| Default | `--wait-ms 2000` |
| Slow / heavy SPA | `--wait-ms 5000` |

> Only inline `<script>` tags execute. External scripts are not fetched. `setTimeout`/`setInterval` are no-ops.

---

## Output Format Selection

| Goal | Flag |
|------|------|
| Structured data extraction | `--format json` |
| Navigation graph | `--format json --with-nav` |
| Tree structure inspection | `--format tree` |
| General reading (default) | *(omit — markdown is default)* |

> `--format llm` does not exist. Always include source URL and relevant section in extracted answers.

---

## Advanced Strategies

**Site Exploration** — use before interacting with unknown or complex sites:
```bash
open-browser map https://example.com --depth 2 --output kg.json
# Read kg.json → states (url, title, semantic_tree) + transitions (verified edges)
```

**PDF Handling** — navigate directly, no external parser needed:
```bash
open-browser navigate https://example.com/report.pdf
# Auto-detects application/pdf, returns parsed semantic tree
```

**XHR / Dynamic Data** — inspect raw API responses:
```bash
open-browser navigate https://example.com --network-log --format json
```

# 若需要 auth，使用 browser_* tool API 的 headers 參數（CLI 目前不支援）

**New Tab Handling** — after a flow opens a new tab:
```bash
open-browser tab list          # find the new tab ID
open-browser tab switch <id>   # switch context explicitly
# In REPL: tab list → tab switch <id>
```

---

## Quick CLI Reference

```bash
# Navigate and read semantic tree (HTML or PDF)
open-browser navigate "https://example.com"
open-browser navigate "https://example.com/report.pdf"

# Output formats
open-browser navigate "https://example.com" --format json
open-browser navigate "https://example.com" --format json --with-nav
open-browser navigate "https://example.com" --format tree

# Interactive elements only
open-browser navigate "https://example.com" --interactive-only

# JavaScript mode
open-browser navigate "https://example.com" --js --wait-ms 2000

# Network debug / XHR extraction
open-browser navigate "https://example.com" --network-log --format json

# Auth bypass
open-browser navigate "https://api.example.com/data" --header "Authorization: Bearer <token>"

# Form filling by element ID
open-browser interact "https://example.com/login" type-id 1 "my_username"
open-browser interact "https://example.com/login" type-id 2 "my_password"
open-browser interact "https://example.com/login" click-id 3

# Map site structure
open-browser map "https://example.com" --depth 2 --output kg.json

# Persistent REPL session
open-browser repl
open> visit https://example.com
open [https://example.com]> click #3
open [https://example.com]> type #5 "search query"
open [https://example.com]> tab open https://example.com/page2
open [https://example.com/page2]> back
open [https://example.com]> exit

# CDP server
open-browser serve --host 0.0.0.0 --port 9222
```

---

## Error Recovery

| Error | Fix |
|-------|-----|
| Element ID stale after navigation | Re-read state before retrying — IDs reset on every page load |
| Semantic tree nearly empty | Add `--js --wait-ms 3000` and re-read |
| `method not found` from CDP | Fall back to `open-browser` CLI or REPL |
| External scripts not executing | By design — use `--wait-ms` to let inline-script async content settle |
| Tab state lost between commands | Tab state doesn't persist across CLI calls — use `open-browser repl` |

---

## Safety & Limitations

- Private/loopback/link-local/metadata endpoints may be blocked — report the constraint, do not bypass with another browser stack.
- `Page.printToPDF` is unsupported in semantic-only mode.
- Screenshot support requires optional build feature — assume unavailable unless confirmed.
- Prefer core primitives: `semanticTree`, `interact`, `wait`. If a higher-level tool returns `method not found`, fall back to these.