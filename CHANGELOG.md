<!--
How to write an entry. Newest release first.

Head each release "## <version> - <YYYY-MM-DD>", the date of its tag.

Sections, in this order. Omit none: a section with nothing in it says
"None." so readers can trust that it was considered.

  Fixes                 Things that were broken and now are not. A
                        security-relevant fix leads the list, labelled
                        [security].
  Features              New and changed behavior an app or its users can
                        see. Lead the bullet with the area when there is
                        one: "**GPU: ...**", "**Input: ...**". Performance
                        work belongs here too, not in Various: faster is
                        something users see.
  API                   New or changed public surface. A feature that also
                        adds surface appears twice: Features says what it
                        does, API names the surface.
  Developer experience  What an app developer touches while building: srt
                        commands and their output, dev server behavior,
                        launcher UI, error messages. Contributor-facing
                        build and CI work goes in Various instead.
  Agents                Aimed at coding agents: scaffold AGENTS.md,
                        MCP surfaces, examples (examples exist for agents).
  Breaking changes      Removals and signature changes. Behavior changes
                        an app can notice without opting in go here too,
                        as a note under "None." So do platform and
                        requirement changes that can strand someone: a
                        dropped target, a minimum OS, Bun or Rust bump.
                        New platform support, which strands nobody, goes
                        in Features.
  Various               Everything else: dependency updates, build, CI,
                        backlog notes. Lead the dependency bullet with
                        "Dependencies:", and say so even when there are
                        none.

Rules:
- Roll a cluster of related commits into ONE bullet stating the larger
  goal they served, mentioning the pieces in passing. Not one bullet per
  commit.
- Short descriptions. No docs-were-updated bullets, no individual test
  file names, no rebuilt bundles.
- No per-crate labels. Platform-specific entries DO get a label, leading
  the bullet: [windows], [android], [linux], [macos]. Labels are always
  lowercase, whatever the thing is normally called.
- ASCII only, no em-dashes.
-->

# Changelog

## 0.0.45 - 2026-08-04

### Fixes
- **viewBox hit testing** - hits now go through the `viewBox` transform, so hit targets line up with what you see (regression test added).
- [windows] **Present fences** - missing `glFlush` meant post-swap ANGLE/D3D11 fences never signalled; frames could stall.

### Features
- **GPU: the low-level API can now render a 3D scene frame** - N meshes, one material each, one shared depth buffer, one pass. That completes with retained draw lists per target (`createDrawTarget`, add/remove with stable ids), explicit draw order, index buffers, cull mode, and per-instance attributes.
- **GPU: subtree shader effects** - a `<view>` can run a fragment shader over its own rasterization (params, extra textures, outset, and `previous` for source-history effects like cross-dissolve). Hit testing stays on the undistorted geometry.

### API
- `@solidrt/core/gpu`: `createDrawTarget` + `addDraw` / `removeDraw` / `setDrawOrder` / `setDrawParams` / `setDrawTextures` / `setDrawRange`; new `DrawId`, `CullMode`, `IndexBinding`, `IndexFormat`, `IndexRange` types.
- `createRenderPipeline` gains `instanceAttributes`, `instanceBuffer`, `cull`; `setDraw` accepts an index range.
- `<view shader={...}>` with the new `ViewShaderProps`.

### Developer experience
- None.

### Agents
- **Scaffold `AGENTS.md`** - agents must first check for an already-running dev server + client (MCP `list_clients`) and work against it via `reload` / `get_logs` / `get_snapshot`, instead of starting a second `srt run`.
- **New examples** covering the new surfaces: `gpu-draw-list`, `view-shader`, `view-shader-history`, `view-viewbox` (plus README index and a `responsive-grid` touch-up).

### Breaking changes
- None. One behavior change: `instanceCount` now defaults to one instance per instance-buffer record (was required).

### Various
- Dependencies: no updates.
- Tests now run in release CI, with previously-uncommitted lattice and flux tests added.
- [windows] Build: `Makefile.windows` + static-CRT cmake toolchain file.
- Backlog filed: ANGLE present-fence pacing, adaptive fence depth, GPU cube maps, GPU uniform arrays.

---

Releases before 0.0.45 are not covered here; see the git log.
