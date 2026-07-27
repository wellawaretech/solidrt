---
type: backlog-item
title: flux:net socket gaps
description: "Three flux:net gaps surfaced by the linux VM's NAT gateway: Udp.close, TCP half-close, and raw ICMP; closed by one cancellation token per socket."
status: done
timestamp: 2026-07-23T00:00:00Z
---

# flux:net socket gaps

Motivation (linux VM, 2026-07-23): projects/linux now runs a full NAT
gateway for its riscv64 guest - TinyEMU's virtio-net feeds an in-wasm
slirp (cartridge/net.c), and a thin JS shuttle services its socket
imports with flux:net. Building that surfaced three API gaps, in
descending order of pain:

1. DONE 2026-07-23, widened to all three socket types: the missing piece was
   a cancellation story for forge::net as a whole, not just Udp. One
   CancellationToken per socket, select!ed against the pending await. Udp
   gained close() (pending recv resolves null), Listener gained close()
   (pending accept resolves done, ending for-await), and Conn.close() was
   fixed to cancel a pending read (which previously survived close and even
   resurrected the read half) and to drop the write half immediately, so the
   peer sees FIN at close time instead of at GC.

   Original item: `Udp` has no `close()`. `Conn` has close and `Listener` docs say
   "drop it to stop", but a Udp socket with a pending `recv()` cannot be
   released at all: the shuttle can only stop delivering and drop its
   reference, leaving the bound socket plus the pending recv promise
   alive until GC (if ever - the pending recv references the socket).
   The NAT churns through per-flow UDP sockets (DNS queries, one socket
   per guest flow with LRU eviction at 32), so every evicted flow leaks
   a bound port. Fix shape: `close()` that releases the fd and rejects
   or resolves-with-end any pending `recv()`.

2. DONE 2026-07-23 as `conn.closeWrite()` (Deno's name, chosen over
   shutdown/finish; pairs with close()). forge Conn::close_write flushes and
   shuts down the write half, then drops it; reads continue until the peer
   closes; write() errors afterwards; idempotent and a no-op once closed.
   Survey note: TCP was the only duplex missing this - p2p Stream.finish()
   and subprocess endStdin() already had it, WebSocket/http don't need it.

   Original item: `Conn` has no half-close (`shutdown(WR)`). A TCP proxy cannot
   propagate the guest's FIN while still reading the response, so
   protocols where the client signals end-of-request with EOF before
   the server answers (netcat-style pipes, some git/rsync transports)
   stall through the NAT. HTTP(S) is unaffected - clients keep the
   socket open until the response lands. Fix shape: `conn.shutdown()`
   (write side only), keeping the read iterator alive until the peer
   closes.

3. DONE 2026-07-23 as `icmpEcho(host, payload?, { timeoutMs? })` -> `{ status,
   rttMs?, payload? }` (status = reply / timeout / unsupported), a purpose-built
   probe rather than a raw socket, matching probe()'s infallible-outcome shape.
   forge icmp_echo over an unprivileged ICMP socket (socket2 SOCK_DGRAM +
   IPPROTO_ICMPV4, hand-rolled echo packet + checksum, blocking send/recv on
   spawn_blocking). Correlation is seq + payload (Linux ping sockets rewrite the
   id); parser handles the BSD/macOS IP-header-prepend quirk. No new dependency,
   no packet crafting exposed. `unsupported` = no unprivileged ICMP socket
   (restricted ping_group_range, Windows), keeping "host silent" distinct from
   "cannot ask". IPv4 only. Verified: reply on this box's loopback, timeout for
   TEST-NET-1. Naming aside also landed: Conn.peer -> Conn.remoteAddr (pairs
   with localAddr), and the half-close verb unified to closeWrite across net
   Conn, p2p P2pStream (was finish), and subprocess Child (was endStdin).

   Original item: No raw ICMP socket, so guest `ping <real host>` cannot be proxied;
   the NAT answers ping only for its own gateway address. Lowest
   priority: TCP/DNS reachability covers the practical need, and an
   unprivileged ICMP (SOCK_DGRAM, IPPROTO_ICMP) socket has platform
   quirks (Linux needs net.ipv4.ping_group_range; macOS allows it;
   Android varies). Fix shape if wanted: `icmpEcho(host, payload,
   timeoutMs)` as a purpose-built probe rather than a raw socket -
   matches probe()'s "infallible outcome" philosophy and avoids
   exposing packet crafting.

Non-gap noted while diagnosing: `connect()`'s `timeoutMs` is dial-only
(good); a mid-session EOF that looked like a runtime read timeout turned
out to be the TLS server's own ~15s handshake deadline.
