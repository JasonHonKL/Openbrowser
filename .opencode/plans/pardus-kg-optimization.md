# pardus-kg Optimization Plan

## Pre-requisite Fix (unrelated pre-existing bug)

**File:** `crates/pardus-core/src/page.rs:783-791`

The `snapshot()` method is missing the `redirect_chain` field, causing compilation failure.

```rust
pub fn snapshot(&self) -> PageSnapshot {
    PageSnapshot {
        url: self.url.clone(),
        status: self.status,
        content_type: self.content_type.clone(),
        title: self.title(),
        html: self.html.html(),
        redirect_chain: self.redirect_chain.clone(),
    }
}
```

---

## Phase 1: Bug Fixes (B1-B4)

### B1. Fix query param sorting in `normalize_url`

**File:** `crates/pardus-kg/src/crawler.rs:224-234`

Docstring says "sort query params" but code doesn't sort. Sort query pairs before rebuilding URL string.

### B2. Fix `Box::leak` memory leak in pagination

**File:** `crates/pardus-kg/src/discovery.rs:139,146`

Change `segments` from `Vec<&str>` to `Vec<String>` and use local `String` instead of `Box::leak`.

### B3. Fix duplicate `navigation_graph()` call

**File:** `crates/pardus-kg/src/crawler.rs:176-221`

Pass pre-built `nav_graph` into `discover_transitions_for_page` instead of rebuilding inside.

### B4. Fix same-origin filtering at frontier insertion

**File:** `crates/pardus-kg/src/crawler.rs:237-239`

Replace unused `_root_origin` param with actual same-origin check. Skip cross-origin URLs before enqueueing.

---

## Phase 2: Quick Wins (H4, M1)

### H4. Incremental blake3 hashing

**File:** `crates/pardus-kg/src/fingerprint.rs`

Replace 3 functions (`hash_tree_structure`, `hash_resource_set`, `compute_view_state_id`) to use `blake3::Hasher::new()` + incremental `update()` calls instead of building large intermediate strings.

### M1. Remove duplicate `role_str` function

**File:** `crates/pardus-kg/src/fingerprint.rs:88-122`

Delete the local `role_str()` that allocates `String` per call. Use `node.role.role_str()` which already exists on `SemanticRole` in pardus-core and returns `&str`.

---

## Phase 3: Parallel Fetch (H1)

**File:** `crates/pardus-kg/src/crawler.rs`

- Add `concurrency: usize` field to `CrawlConfig` (default: 4)
- Add `tokio/sync` and `tokio/rt` features to `Cargo.toml`
- Replace serial BFS loop with batched parallel fetch using `tokio::task::JoinSet` + `tokio::sync::Semaphore`
- Result processing stays serial to maintain BFS ordering and safe `HashMap` mutation
- Parallelism is I/O-bound fetch only; semantic tree building stays in collection loop

---

## Phase 4: Single-Pass HTML (H3)

### New unified analysis API

**New file:** `crates/pardus-core/src/page_analysis.rs`

Create `PageAnalysis` struct with `build(html, page_url)` that produces both `SemanticTree` and `NavigationGraph` through a single API call. Initially delegates to individual builders; evolved later into true single-pass.

---

## Phase 5: Memory Optimization (M2-M4, L1, L3)

### M2. Optional tree storage

- Add `store_full_trees: bool` to `CrawlConfig`
- Make `semantic_tree` and `navigation_graph` `Option<T>` on `ViewState`
- Skip serializing when `None`

### M3. Type-safe HashMap keys

**File:** `crates/pardus-kg/src/graph.rs`

Change `states: HashMap<String, ViewState>` to `HashMap<ViewStateId, ViewState>`. Update `add_state` and `has_state` accordingly. Update all callers.

### M4. HashSet for resources

Change `resource_urls: BTreeSet<String>` to `HashSet<String>` across state.rs and fingerprint.rs. Sort only when hashing.

### L1. Remove dead `verify_transitions` config

Remove the unused field from `CrawlConfig`.

### L3. Crawler-level retry

Add `retries: u8` to `FrontierEntry`. On fetch failure, re-enqueue up to 2 retries.

---

## File Change Summary

| Order | File | Changes |
|-------|------|---------|
| 0 | `pardus-core/src/page.rs` | Fix missing `redirect_chain` in `snapshot()` |
| 1 | `pardus-kg/src/discovery.rs` | B2: Fix `Box::leak` |
| 1 | `pardus-kg/src/crawler.rs` | B1: query param sort; B3: pass nav_graph; B4: same-origin filter |
| 2 | `pardus-kg/src/fingerprint.rs` | H4: incremental blake3; M1: remove role_str; M4: HashSet |
| 3 | `pardus-kg/src/crawler.rs` | H1: parallel fetch |
| 3 | `pardus-kg/src/config.rs` | Add `concurrency` |
| 3 | `pardus-kg/Cargo.toml` | Add tokio features |
| 4 | `pardus-core/src/page_analysis.rs` | New file: unified PageAnalysis |
| 4 | `pardus-kg/src/crawler.rs` | Use PageAnalysis |
| 5 | `pardus-kg/src/graph.rs` | M3: HashMap key type |
| 5 | `pardus-kg/src/state.rs` | M2: optional trees; M4: HashSet |
| 5 | `pardus-kg/src/config.rs` | L1: remove dead field; add store_full_trees |

## Verification

```bash
cargo test -p pardus-kg
cargo test -p pardus-core
cargo clippy -p pardus-kg -- -D warnings
cargo build -p pardus-kg
```
