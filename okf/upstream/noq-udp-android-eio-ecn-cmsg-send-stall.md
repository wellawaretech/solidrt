---
title: noq-udp keeps failing every send after an EIO on Android
description: On a phone whose kernel answers a GSO sendmsg with EIO, noq-udp halts segmentation offload but keeps attaching the ECN cmsg, which that kernel also refuses with EIO, so every later send fails and the phone's uploads over flux:p2p stall while downloads work; fixed in quinn and synced into noq's main, not in a noq release yet.
created: 2026-10-17
status: fixed-upstream
project: noq-udp (n0's quinn fork, under iroh)
versions: noq-udp 1.3.0 (iroh 1.3.0)
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
  823, merged 2026-10-16). No noq release carries it yet; our tree pins
  noq-udp 1.3.0 through iroh 1.3.0, whose `send` still has the
  `EINVAL`-only fallback.
- The separate half, the dropped batch itself (quinn issue 2399, "retry
  failed GSO sendmsg on EIO without GSO"), is open upstream with a
  partial-sends PR linked.

## When it resolves

Bump iroh (and with it noq, noq-proto, noq-udp) to the release that carries
the sync, then re-run the Pixel as a sender over a direct path:
`examples/screencast` cast from the phone, and `examples/call` as the
guest, both without a relay. Until then the phone works as a receiver, and
through a relay (`serveRelay` of flux:p2p) its direct path dies and the
relay path carries the data, late and in bursts. okf/plans/video-encode.md
step 7 records the decision not to work around it in our tree.
