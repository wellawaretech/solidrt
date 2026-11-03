---
title: noq-udp keeps failing every send after an EIO on Android
description: On a phone whose kernel answers a GSO sendmsg with EIO, noq-udp halts segmentation offload but keeps attaching the ECN cmsg, which that kernel also refuses with EIO, so every later send fails and the phone's uploads over flux:p2p stall while downloads work; fixed in noq-udp 1.4.0, in our tree since the iroh 1.4.1 bump (2026-11-03).
created: 2026-10-17
status: resolved
project: noq-udp (n0's quinn fork, under iroh)
versions: noq-udp 1.3.0 (iroh 1.3.0); fixed in noq-udp 1.4.0 (iroh 1.4.1)
link: https://github.com/quinn-rs/quinn/pull/2719
---

# noq-udp keeps failing every send after an EIO on Android

## Symptom

A Pixel 7 casting its window over `flux:p2p` on a direct IPv4 path: the
first packets arrive at the peer (the container header, 140 bytes), then
nothing, while the phone's writes keep succeeding into QUIC's send buffer.
The same phone receives a cast without trouble; a laptop and a tablet as
senders have no such trouble. Earlier, over relay-mixed paths, the phone's
data came late and in bursts. The phone's log carries
`sendmsg error ... EIO ... ecn: Ect0, segment_size`.

## Cause

`noq-udp`'s `send` (src/unix.rs) handles a failed `sendmsg` in two steps.
An `EIO` or `EINVAL` halts GSO (`max_gso_segments` to 1), so later
transmits go out unsegmented. A second fallback drops the ECN cmsg
(`IP_TOS`) from the message and retries, but it fires on `EINVAL` only.
Some Android kernels answer the ECN cmsg with `EIO`, so after GSO is
halted every send that still carries the cmsg fails the same way, and the
datagrams of each failed call are not resent at this layer (QUIC's loss
recovery would only help if a later send went through). The receive path
is untouched, hence uploads stall and downloads work.

## Upstream

- quinn PR 2719, "quinn-udp: handle EIO from sendmsg in the cmsg fallback
  (not just EINVAL)", merged 2026-07-10 (closes quinn issue 2718).
- n0 synced it into noq's `main` with "chore: sync quinn main" (noq PR
  823, merged 2026-10-16), released in noq-udp 1.4.0 (2026-10-20; its
  `unix.rs` cmsg fallback matches `EINVAL | EIO`), which iroh 1.4.1
  (2026-10-21) requires.
- The separate half, the dropped batch itself (quinn issue 2399, "retry
  failed GSO sendmsg on EIO without GSO"), is still open upstream:
  okf/upstream/noq-udp-android-gso-eio-first-call-stall.md.

## Outcome

Bumped iroh, iroh-base and iroh-relay to 1.4.1 on 2026-11-03 (noq,
noq-proto and noq-udp to 1.4.0), the Pixel 7 client and the laptop host
both rebuilt. `examples/call` with the Pixel as the guest on a direct
path (`relayOnly` off) now holds `live; sending; path mixed` in both
directions with no redial (80 s and longer), where the direct path used to
die and the relay carried the data late and in bursts.

What remains is a 15 s stall on the first call of a fresh phone endpoint,
which the redial on the same endpoint gets past; a relay-only guest has
none. That is the other half, worked around by building Android endpoints
without GSO and tracked in
okf/upstream/noq-udp-android-gso-eio-first-call-stall.md.
