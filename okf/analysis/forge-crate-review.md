---
type: analysis
title: Forge crate review
description: Engine-free layering upheld, docs excellent, clippy clean; gaps are untested subprocess/p2p/ffi, stale docs, an implicit single-thread contract and IPv4-only skew.
timestamp: 2026-07-15T00:00:00Z
---

# Forge crate review

Full-crate review of `forge` (~3.9k lines, 17 modules) as of 2026-07-15: every
module read, unit tests run (20, all pass), clippy run (4 trivial lints), and
the flux integration suite mapped against the module list.

## Summary

Forge is the most production-ready crate in the repo. The engine-free contract
(no scripting-engine types, marshalling stays in flux) is upheld everywhere
without exception, the error model is uniform (`Result<_, String>` with
contextual messages), `unsafe` is confined to the one module whose whole job
is unsafe (`ffi`), and the module docs are consistently excellent - most
explain not just what but why the module exists. Clippy is essentially clean.

The gaps are not correctness but scope and hardening: five module docs are
stale ("Destined for the forge crate" - it is the forge crate), the crate
silently assumes a single-threaded host (`Rc` in public APIs) without saying
so at crate level, IPv4-only skew runs through net/mdns/p2p, and three
modules (`subprocess`, `p2p`, `ffi`) have no automated tests at all.

## Completeness

Measured against the crate doc ("engine-free capability cores" for flux) and
the flux modules built on it, every module does what its consumer needs today.
Boundaries worth knowing:

- **IPv4-only skew.** `net::udp_bind` binds `0.0.0.0` only, `mdns::resolve`
  skips IPv6 inputs, `p2p` `bind_port` pins an IPv4 socket. All documented as
  deliberate (the LAN-dev path), but `net::interfaces` happily reports v6
  addresses no other API can use. IPv6 is the crate's largest scope boundary.
- **mdns is query-only.** `resolve`/`browse`/`services` work; there is no
  responder/advertise side, so a flux app can find printers but cannot
  announce itself over DNS-SD (the udp multicast beacon is the current
  substitute).
- **No TLS.** The HTTP server core is plain HTTP/1 (wss/TLS already noted as
  pending in the serve plan); `fetch` does have rustls. No HTTP/2, and no
  request-body size limit (also a known pending item).
- **Routing edge:** a `*` mid-pattern (`/a/*/b`) silently behaves as a
  trailing wildcard - `match_segments` returns on `Wildcard` without looking
  at the rest of the pattern. Reject or document it.
- **subprocess kill is SIGKILL-only** (`start_kill`); there is no graceful
  SIGTERM-then-KILL option and no process-group handling.
- **sqlite sets no `busy_timeout`**, so two connections writing the same file
  get an immediate SQLITE_BUSY instead of a bounded wait.
- **`path::resolve_within` is lexical.** A symlink inside `base` pointing
  outside escapes the containment check. Fine for its current use (trusted
  bases), but the doc sells it as the escape-proof primitive; the caveat
  belongs in the doc, or the check needs a canonicalize step.

## Code quality

Production level for the current stage. Specifics:

- **Layering discipline is real, not aspirational.** Spawning is always left
  to the caller ("spawning is host-specific"), callbacks are generic (`Service`,
  `FnMut`, `WsDispatch`, `HostHandler`), and no module names an engine type.
  The wasm resumable-call bridge and the ffi dispatcher-threading are the two
  hard cases, and both solve it cleanly.
- **The single-thread assumption is implicit.** `fetch::do_fetch` takes
  `Rc<reqwest::Client>`; `subprocess::Child`, `p2p::Stream`, and
  `websocket::SocketSink`/`Topics` are `Rc`-based. That is coherent (the
  QuickJS thread) but contradicts the lib.rs claim that "a pure-Rust host
  could use it directly" - a multi-threaded host cannot. State the contract
  in the crate doc, or take `&Client` / plain values where `Rc` adds nothing
  (fetch's `Rc` in particular buys nothing at this layer).
- **Panic discipline is good**: three `expect`s and one `unreachable!`, each
  locally justified (piped stdio just configured, response builder with known
  headers, match gated by `KNOWN_SIGNALS`). No `todo!`/`unimplemented!`.
- **`unsafe` is confined to ffi.rs** (12 blocks) and that module's trust
  model, liveness contract, and drop-order invariants are documented to an
  unusual standard (the `DispatchGuard` lifetime transmute, the LE-dependent
  `Slot` union, field-declaration drop order). One hardening nit: `open_bytes`
  writes the library to a predictable name in the shared temp dir before
  dlopen; an exclusive-create temp file (O_EXCL / `tempfile`) would close the
  pre-planted-path window.
- **Unbounded write queues.** `p2p::Stream::write` and the websocket writer
  use unbounded mpsc channels. Websocket compensates with byte accounting and
  Bun-style backpressure signals; p2p has nothing - a fast JS producer against
  a slow peer grows the queue without bound. Mirroring the websocket
  accounting (or a bounded channel) is the fix.
- **Error strings, not error types.** Uniform and adequate for marshalling to
  JS exceptions, but hosts cannot discriminate (e.g. connect-refused vs DNS
  failure) without string matching. Not worth changing today; worth knowing
  it is load-bearing.
- Small efficiency notes: `sqlite::query_with` clones every column name into
  every row (share one header vector instead); `websocket::Topics::publish`
  clones the payload per subscriber (`Bytes` would be zero-copy);
  `mdns::query` always waits the full timeout window even when every wanted
  answer has arrived (resolve could early-exit).
- Clippy: 4 minor lints (needless refs in a wasm example, the `Result<_, ()>`
  on `send_obligated`, a complex type, an index loop in sqlite). Nothing
  behavioral.
- **Stale docs**: path.rs, fs.rs, http.rs, p2p.rs, websocket.rs, sqlite.rs,
  subprocess.rs still carry "Destined for the `forge` crate (see REDESIGN.md)"
  and most module docs reference pre-reorganization marshalling paths
  (`plugins/flux/*.rs`; the real layout is `flux/src/plugins/modules/*.rs`).
  mdns.rs and net.rs show the corrected form to copy.

## Tests

20 unit tests in `src/tests/` (mdns, net, wasm), all passing, plus the flux
integration suite (16 files, ~2.9k lines, gated `unix + compile`) driving most
modules through the real JS marshalling. Coverage by module:

| module | unit | integration | verdict |
|---|---|---|---|
| wasm | 11 tests incl. re-entrancy, unwind, call_indirect | - | strong |
| mdns | correlation + wire round-trip | flux mdns.rs | good |
| net | probe/udp/interfaces | flux net.rs | good |
| http | - | flux http.rs (23 fns) + websocket.rs | good, but routing precedence/edge cases deserve unit tests (cheap: `RouteTable` is pure) |
| fs, path, process, sqlite, events, websocket | - | dedicated flux test files | good |
| fetch | - | indirectly (as the client driving serve tests) | acceptable |
| stream, seek, logger | - | incidental | fine (trivial) |
| **subprocess** | - | **none** (manual example only) | gap |
| **p2p** | - | **none automated** (lattice tunnel e2e is env-gated; manual examples) | gap |
| **ffi** | - | **none** (2 manual examples) | gap - and it is the crate's riskiest module |

The three gaps are also the three modules whose failure modes are the nastiest
(child-process lifecycle, QUIC lifecycle, undefined behavior). `subprocess` is
the easiest win: `run_output`/spawn/kill/status against `/bin/sh -c`-free
fixtures (echo, cat, sleep) is deterministic and CI-safe. For `ffi`, the
existing `ffi_smoke` example is already most of a test - a tiny fixture
`.so` built by the test (or a libc function with a known ABI) would let the
call/callback/memory paths run in CI. p2p can test ticket encode/parse and
hex codec purely today; loopback endpoint-to-endpoint is feasible but heavier.

## Improvement points, ranked

1. **Add subprocess tests** (deterministic, no fixtures needed) and **ffi
   smoke tests**; promote the pure parts of p2p (`encode_ticket`/`parse_dial`,
   `decode_hex32`) into unit tests now, loopback e2e later.
2. **Fix the stale module docs** (7 files): drop "Destined for the forge
   crate", point at the real marshalling paths. Ten-minute change, removes
   real confusion for the next reader.
3. **State the single-thread host contract in lib.rs** (or de-`Rc` fetch).
4. **Backpressure for p2p writes** - mirror the websocket accounting.
5. **Document (or fix) the `resolve_within` symlink caveat** and the
   mid-pattern `*` routing behavior; add unit tests for both while there.
6. **sqlite `busy_timeout`** default; consider the shared-header row shape if
   large query results show up in profiles.
7. **ffi `open_bytes` temp file**: exclusive create.
8. IPv6 and mdns-advertise: track as deliberate scope items in the backlog,
   not code debt.
