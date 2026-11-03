---
title: The first p2p call from a fresh Android endpoint stalls on the direct path
description: On a Pixel 7 the first call over a direct UDP path gets no data through for 15 s in either direction, while the redial on the same endpoint goes live at once and a relay-only endpoint is live in 3 s: the GSO batch that fails with EIO is dropped rather than resent unsegmented (quinn issue 2399, open); forge builds Android endpoints without GSO until then.
created: 2026-11-03
status: filed
project: noq-udp (n0's quinn fork, under iroh)
versions: noq-udp 1.4.0 (iroh 1.4.1)
link: https://github.com/quinn-rs/quinn/issues/2399
---

# The first p2p call from a fresh Android endpoint stalls on the direct path

## Symptom

`examples/call`, laptop as the host, Pixel 7 as the guest, both on iroh
1.4.1, each trial on a fresh dev server, host and phone client
(2026-11-03):

| Guest endpoint | First 15 s | After |
|---|---|---|
| direct (`relayOnly` off), run 1 | no picture either way, `path mixed` | the redial is live at +25 s and stays live |
| direct, run 2 | the host gets nothing; the phone gets a few bytes at +14 s | the redial is live at +25 s |
| relay-only | live at +3 s, `path relay` | stays live |

Both opens time out ("no answer from the source within 15000 ms"), the
guest dials again on the same endpoint, and that call runs. The phone's
upload is the side that stalls; its download crawls behind it, since its
ACKs leave by the same sends. The laptop, as either end, shows no such
thing.

## Cause

The phone's kernel answers a GSO `sendmsg` with `EIO`. Since noq-udp 1.4.0
(okf/upstream/noq-udp-android-eio-ecn-cmsg-send-stall.md, resolved) that
error halts GSO and drops the ECN cmsg for later sends, both kept on the
socket's state, but the datagrams of the failed call are not resent
unsegmented: QUIC loss recovery has to find them, and on a fresh
connection its timers are long enough to outlast the call's 15 s open.
The redial reuses the endpoint's socket, whose GSO is already off, so it
never meets the failure. noq's own log lines (`sendmsg error`, the GSO
halt) do not reach the phone's logcat through our logger, so the cause was
confirmed by switching GSO off instead: with
`enable_segmentation_offload(false)` on the phone's endpoint, three fresh
direct-path first calls were all live by +4 s (2026-11-03), against none
of three without it.

## Upstream

quinn issue 2399, "udp: retry failed GSO sendmsg on EIO without GSO",
filed by someone else, open (last activity 2026-07-07), with a
partial-sends PR linked. It asks for exactly the retry this needs.

## Workaround

`build_endpoint` in forge/src/p2p/endpoint.rs builds every Android
endpoint with GSO off (iroh's
`QuicTransportConfig::enable_segmentation_offload(false)`, on top of
iroh's transport defaults), so the first segmented send never happens. The
cost is CPU per datagram, negligible at call rates; a bulk transfer from a
phone would feel it first.

## When it lands

The workaround comes out when the fix reaches our tree, not when 2399
closes. On each upstream check, follow the chain the ECN fix took
(okf/upstream/noq-udp-android-eio-ecn-cmsg-send-stall.md):

1. 2399 closes, or its partial-sends PR merges, in quinn.
2. n0 syncs it into noq's `main` (a "chore: sync quinn main" PR on
   n0-computer/noq).
3. A noq-udp release carries it: read its `src/unix.rs` at the release tag
   for a failed GSO `sendmsg` that is resent unsegmented, not dropped.
4. An iroh release requires that noq-udp.

Then, in one change: bump iroh, iroh-base and iroh-relay to that release;
remove the `#[cfg(target_os = "android")]` GSO override in
`build_endpoint` (forge/src/p2p/endpoint.rs); rebuild the Android client;
re-run the table above, laptop host and Pixel guest on a direct path, each
trial on a fresh server, host and phone client. If every first call is
live by a few seconds, set this note to `resolved` with the outcome. If
not, put the override back and record what the release missed.
