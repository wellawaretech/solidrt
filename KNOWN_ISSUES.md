# Known Issues

## Impeller font rendering is poor at low DPI and small sizes on dark backgrounds

Impeller uses grayscale-only antialiasing with no subpixel rendering support. On desktop monitors running at 1x scale (no HiDPI), small fonts (roughly 14px and below) have hairline strokes that are difficult to read, particularly on dark backgrounds where the thin grey antialiased edge bleeds into the background.

Mobile is unaffected because device pixel ratios of 2-3x mean the same logical font size is rasterized at 2-3x the physical pixels, producing visibly thicker strokes.

**Current workaround:** `font_weight` defaults to `Medium` in `Text::default()` to ensure readable stroke width on 1x desktop displays. The `fontWeight` TSX prop overrides this per element.

**Proper fix:** make the default font weight DPI-aware - use `Regular` when `display_scale >= 2.0`, heavier weights at lower scales. `display_scale` is now plumbed into `PlatformContext` (`set_display_scale` in `lattice/src/lib.rs`), so the remaining work is just reading it at text build time and selecting the default weight from it.

## Snapshot rasterization stalls the UI thread (fence sync not implemented)

A `repaintBoundary="snapshot"` subtree is rasterized into a GL texture on the UI thread and sampled on the render thread from a separate (SDL-shared) GL context. Shared-object contents are undefined in the reader until the writer's GL commands actually complete; `glFlush` only submits them. The original code used `glFlush`, which produced intermittent corruption (nothing / shifted / wrong-glyph / colored-rects-in-black / fully-filled-rects) on newer, deeply-pipelined Android GPUs only - older devices, the emulator, and Linux happened to complete the write before the reader sampled.

**Current fix:** `render_display_list_to_texture` in `alloy/src/gl.rs` calls `glFinish` (not `glFlush`) after rasterizing, blocking the UI thread until the GPU is done so the texture is complete before it crosses to the render thread.

**Remaining limitation:** `glFinish` is a full pipeline drain on the UI thread. It only runs on a *fresh* rasterization (snapshot cache miss), so the stall is rare in practice. The cheaper fix is a GL fence (`glFenceSync` on the UI thread + server-side `glWaitSync` on the render thread), which removes the stall, but it requires carrying a `GLsync` handle per-texture across the mpsc `DisplayList` channel. Deferred until a snapshot-heavy scene shows the stall in profiling.