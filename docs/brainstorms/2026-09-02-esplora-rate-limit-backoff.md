# Brainstorm: Esplora rate-limit backoff for ldk-server (B now, A later)

**Date:** 2026-09-02
**Scope decision:** B now (ldk-server local hardening), A next (upstream ldk-node)
**Signal:** vincent@65 — 8× `Incremental sync of on-chain wallet failed` / `TxSyncFailed` (mempool.space) today, last `Lightning-wallet sync failure at 08:16 UTC`, self-recovered. Points to transient HTTP 429/timeout, not persistent outage.
**Reference:** `folgore-plugin/src/recovery.rs:24` `TimeoutRetry` — blocking 60s*2^n×4, no jitter/429. Inspiration only; ldk-server needs async `tokio::time::sleep` + jitter + `Retry-After`.

## Clarified Problem Statement

**Goal:** Stop noisy `TxSyncFailed`/`Incremental sync ... failed` alerts for all `ldk-server` Esplora users, starting with a local `ldk-server` hardening (B) to validate, then upstream fix in `ldk-node` (A).

**Constraints:**
- `ldk-server` today does `builder.set_chain_source_esplora(server_url, None)` (`ldk-server/src/main.rs:166`, `ldk-node/src/builder.rs:340`) — no `EsploraSyncConfig` exposed, no retry, `per_request_timeout_secs=10` default (`ldk-node/src/config.rs:46`).
- BDK incremental sync loop lives in `ldk-node`; `ldk-server` wrapper cannot fully retry internal sync without upstream — B is config/docs/metrics only, A adds real retry.
- Must be `tokio`-async, not `std::thread::sleep`; respect `429` + `Retry-After`, add jitter, bound `onchain_wallet_sync_timeout_secs=60` / `lightning=30`.
- MSRV 1.85, pinned `ldk-node@16eaa6f`, `hyper`+`tokio-rustls`, no breaking API (`None` → defaults stays valid).
- `mempool.space/api` is public rate-limited; self-host `electrs`/`esplora`/`bitcoind RPC` stays optional, not required.

**Non-goals:**
- Mandating self-host, multi-source active-active, CLN `folgore` crate import, consensus/channel logic changes.

**Success criteria:**
- B validates: vincent@65 can tune `EsploraSyncConfig` via `contrib/ldk-server-config.toml` / `util/config.rs`, docs explain `mempool.space` rate-limit; logs use `warn! retry_scheduled` not `error!` for retriable sync, metric `tx_sync_failed_total` replaces per-event paging.
- A later: `ldk-node` Esplora client retries idempotent GETs on `429/5xx/timeout` with exp backoff + jitter + `Retry-After`; `ldk-server` bumps rev; p95 sync +<30s, <1 alert/day under same load.

## Approaches Considered

### Approach A: Upstream async TimeoutRetry in `ldk-node` (deferred to phase 2)
- Sketch: Add `RetryConfig { max_retries:3, base_delay_ms:1000, max_delay_ms:10000, jitter }` to `EsploraSyncConfig`/`SyncTimeoutsConfig` (`ldk-node/src/config.rs:493`), patch Esplora fetch (`ldk-node/src/chain.rs`) to retry with `tokio::time::sleep`, `delay*=2` + jitter, parse `Retry-After`. `ldk-server` wires `set_chain_source_esplora(url, Some(config))`.
- Affected files: `ldk-node/src/config.rs`, `ldk-node/src/chain.rs`, `ldk-server/src/main.rs`, `ldk-server/src/util/config.rs`, `ldk-server-grpc` if config exposed via gRPC.
- Tradeoffs: + fixes root cause for all users. - cross-repo PR, review latency.
- Effort: M

### Approach B: Local hardening in `ldk-server` (do now)
- Sketch: Expose `EsploraSyncConfig` through `Config`/`ConfigBuilder` (`ldk-server/src/util/config.rs:52, 113, 332`), parse from toml (`esplora.{server_url, timeouts, background_sync}`), pass `Some(config)` to `builder.set_chain_source_esplora` (and `set_chain_source_esplora_with_headers` variant, `builder.rs:358`). Extend `contrib/ldk-server-config.toml` and `docs/configuration.md` with example: raise `per_request_timeout_secs`, add interval jitter. Optionally down-rank `TxSyncFailed` log from `error!` to `warn!` in event handling and add `util/metrics.rs` counter. No `thread::sleep` — keep `tokio` intervals.
- Affected files: `ldk-server/src/util/config.rs`, `ldk-server/src/main.rs:165-166`, `contrib/ldk-server-config.toml`, `docs/configuration.md`, `docs/operations.md`, `ldk-server/src/util/metrics.rs` (optional).
- Tradeoffs: + ships in-repo, validates quickly on vincent@65, no upstream wait. - cannot retry BDK internal incremental sync retries fully until A lands; relies on timeout/interval tuning.
- Effort: S (1 day)

### Approach C: Infra + observability complement (bundle with B)
- Sketch: Recommend self-host `electrs`/`esplora` or `bitcoind RPC` as production fallback (`ChainSource::Electrum` already in `config.rs:1174`), document `mempool.space` as best-effort. Add Grafana alert threshold (>5/hour) not per-event. Future: `fallback_urls` list.
- Affected files: `docs/operations.md`, `docs/getting-started.md`, `ldk-server/src/util/metrics.rs`.
- Tradeoffs: + most resilient long-term. - requires operator infra.
- Effort: S

## Recommendation

**Do B now, queue A next** — per user choice 2026-09-02 12:00. B validates Hypothesis: "exposing `EsploraSyncConfig` + tuning timeouts/intervals + docs/metric is enough to reduce noise and confirm rate-limit root cause on vincent@65". If validated, proceed to A (async retry with jitter + 429) as upstream PR; B's config surface is reused by A.

## Open questions

- Confirm HTTP status: `429` with `Retry-After` vs generic `5xx/timeout`? Paste one `ldk-server.log` line with `RUST_LOG=ldk_node=debug`.
- Retry budget for A: `1s→2s→4s + jitter, max 10s, 3 tries` (proposed) vs folgore `60s*2^n`?
- Log level: keep `warn! retry_scheduled {attempt, delay, status}` + metric, not silent?
- Keep `mempool.space` primary on vincent@65 or also test self-host fallback?

## Next

Run:
```
/ship --from-brainstorm docs/brainstorms/2026-09-02-esplora-rate-limit-backoff.md --plan-only
```
then `/ship` to implement B. When B proves useful, re-run same brainstorm path to plan A.
