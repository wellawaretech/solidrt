---
type: backlog-item
title: AVIF decoding in decodeImage
description: The one practical web image format decodeImage lacks; pure-Rust decode does not exist in the image crate, so it needs the dav1d C system dependency.
status: open
timestamp: 2026-07-19T00:00:00Z
---

# AVIF decoding in decodeImage

Source: binary size work 2026-07-19. decodeImage covers the practical web
set: png, jpeg, webp, gif, bmp, ico (lattice/Cargo.toml image features).
AVIF is the notable gap: CDNs increasingly serve it alongside webp, so apps
fetching web images will eventually hit it.

Why deferred: the image crate only *encodes* AVIF in pure Rust (ravif ->
rav1e, an encoder we deliberately evicted for size). Decoding requires the
`avif-native` feature, which binds the dav1d C library as a system
dependency on every platform, including Android. That is a real build and
packaging cost for a format apps can usually avoid by requesting webp
(Accept-header negotiation server-side).

Revisit when an app actually needs it, or if a pure-Rust AV1 decoder
becomes viable in the image crate.
