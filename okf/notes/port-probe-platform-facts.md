---
title: Port probing and bind coexistence per platform
description: Measured facts behind the dev server's free-port search - Windows reports a refused TCP connect only after ~2 s of SYN retries, so short probe budgets read refused as filtered there; loopback and wildcard listeners coexist under SO_REUSEADDR on macOS only, so a bind failure is the whole truth on Linux and Windows but not on a Mac.
created: 2026-08-28
---

# Port probing and bind coexistence per platform

Measured 2026-08-28 on the three desktop platforms, after `srt run` failed
with "No free port between 34884 and 34983" on Windows although nothing was
listening (the search skipped every candidate that did not probe as
`closed`, and none ever did).

## A refused connect is slow on Windows

A TCP connect to a loopback port with no listener, native Windows 11, every
path measured in one process:

| path | 2 s budget | 5 s budget |
|---|---|---|
| `std::net::TcpStream::connect` (blocking) | ConnectionRefused in 2.06 s | 2.02 s |
| std `connect_timeout` | TimedOut | ConnectionRefused in 2.06 s |
| tokio `TcpStream::connect` (1.52, mio 1.2) | Elapsed | ConnectionRefused in 2.03 s |
| `forge::net::probe` | Filtered | Closed in 2.03 s |
| `forge::net::probe`, port with a listener | Open in 0.3 ms | Open in 0.3 ms |

Windows retransmits the SYN before it surfaces the RST, so the refusal
arrives after ~2 s on every path; nothing in tokio or mio drops it. On Linux
and macOS the refusal is immediate. Consequences:

- Any `flux:net` `probe()` budget under ~2 s classifies a refused port as
  `filtered` on Windows. The default is 1000 ms. A sweep that needs the
  open/closed distinction there (host liveness by refusal) must budget 3 s.
- `Open` is instant everywhere, so "something answers" is a safe signal at
  any budget; "nothing answers" is not, on Windows.
- `forge/src/tests/net.rs::probe_open_then_closed` uses a 3 s budget for
  this reason; the CI `test-forge` matrix runs it on Windows.

## Bind coexistence under SO_REUSEADDR

`std::net::TcpListener::bind` sets `SO_REUSEADDR` on Unix and nothing on
Windows; the dev server binds through it (`forge/src/http.rs`), never
`SO_REUSEPORT`. Two listeners on one port, one `0.0.0.0:P` and one
`127.0.0.1:P`, in both orders:

- Linux: the second bind fails with EADDRINUSE, either order. Coexistence
  would need `SO_REUSEPORT` on both.
- Windows: the second bind fails (no `SO_REUSEADDR` at all).
- macOS 26.3 (arm64): both binds succeed, either order, and a connection to
  `127.0.0.1:P` is delivered to the `127.0.0.1` socket whichever bound
  first.

So on a Mac a `--lan` dev server on P and a loopback dev server on P
silently split traffic: everything reaching the first via loopback (control
API, MCP bridge, local client) lands on the second. That is the case the
pre-bind probe in `packages/cli/src/server/main.ts` `bindFirstFree` guards:
a candidate that answers `open` on loopback is skipped; anything else is
left to the bind, which is the whole truth on Linux and Windows.
