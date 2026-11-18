---
title: A shader element whose input texture changes every frame rebuilds its recordings
description: A <shader> that samples a live texture (a video frame, a camera, a target rendered per frame) takes Damage::Paint on every content change, which re-records the node and every recording above it, where a <texture> showing the same live id takes the present-only path; acceptable while shader panels sample static inputs, to revisit when a video-backed panel shows in paintMs.
created: 2026-11-19
---

# A shader element whose input texture changes every frame rebuilds its recordings

## What happens

`RenderTree::texture_content_changed` (alloy/src/rendertree/tree/mod.rs)
damages a `<shader>` node whose input id is in the changed set with
`Damage::Paint` on the whole node: the node re-records, and so does every
recording above it, and the revision moves. A `<texture>` showing a live
id takes the cheaper present-only route, because its draw is one texture
quad the raster thread can re-present from the retained recording.

A `<shader>` cannot take that route as it stands: its inputs ride in the
recorded op (`Op::DrawShader { inputs }`, the texture handles resolved at
record time), so the recording is the thing that names the texture.

## When it matters

Only with an input that changes every frame: a video or camera texture,
or a shader target rendered per frame bound as an input. The stage 3
measurement sampled static inputs. The cost is the node's own record
walk plus the recordings above it per frame, visible in `paintMs`.

## Ways out

- Let the content-change path mark the node present-only when the
  recording's op already references the id (the handle is the same; only
  the texels moved), as `<texture>` does. Needs the walk to tell "this
  recording's draw_shader binds id N" without re-recording.
- Or bind inputs by id at execution instead of by handle at record time,
  which is the key-versus-placement question the text stage decided
  against for glyphs; not worth reopening for this alone.
