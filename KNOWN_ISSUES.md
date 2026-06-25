# Known Issues

## Impeller font rendering is poor at low DPI and small sizes on dark backgrounds

Impeller uses grayscale-only antialiasing with no subpixel rendering support. On desktop monitors running at 1x scale (no HiDPI), small fonts (roughly 14px and below) have hairline strokes that are difficult to read, particularly on dark backgrounds where the thin grey antialiased edge bleeds into the background.

Mobile is unaffected because device pixel ratios of 2-3x mean the same logical font size is rasterized at 2-3x the physical pixels, producing visibly thicker strokes.

**Current workaround:** `font_weight` defaults to `Medium` in `Text::default()` to ensure readable stroke width on 1x desktop displays. The `fontWeight` TSX prop overrides this per element.

**Proper fix:** make the default font weight DPI-aware - use `Regular` when `display_scale >= 2.0`, heavier weights at lower scales. `display_scale` is now plumbed into `PlatformContext` (`set_display_scale` in `lattice/src/lib.rs`), so the remaining work is just reading it at text build time and selecting the default weight from it.

## Cross-context texture sync (UI thread -> render thread)

GPU work the UI thread produces for the render thread to sample - shader-shader (`alloy/src/shader.rs`), texture uploads (`alloy/src/texture.rs`), and `repaintBoundary="snapshot"` rasterization (`render_display_list_to_texture` in `alloy/src/gl.rs`) - writes into GL textures on the UI thread's context but is sampled on the render thread from a separate (SDL-shared) GL context. Shared-object contents are undefined in the reader until the writer's GL commands actually complete; `glFlush` only submits them. Using `glFlush` alone produced intermittent corruption (nothing / shifted / wrong-glyph / colored-rects-in-black / fully-filled-rects) on newer, deeply-pipelined Android GPUs only - older devices, the emulator, and Linux happened to complete the write before the reader sampled. The interim fix was a blocking `glFinish` on the UI thread, which drained the pipeline and stalled the JS thread (a 24-iteration fragment shader re-rendered per frame via `setShaderParams` parked JS for the whole GPU draw).

**Current fix (fence sync):** `Context::submit` (`alloy/src/context.rs`) creates one `glFenceSync` after the frame's GPU work and `glFlush`es it, carrying the `GLsync` handle alongside the `DisplayList` over the mpsc channel (`Frame { dl, fence }` in `alloy/src/backend.rs`). The render thread does a server-side `glWaitSync` on it (`GlSurface::consume_fence` in `alloy/src/gl.rs`) before compositing - the GPU orders sampling after the writes without stalling either CPU thread. One fence per frame covers all three producers because fences are monotonic on the UI context, so coalesced frames just release their (superseded) fence without waiting.

**Caveat for future code:** any new UI-thread producer of a texture the render thread samples is covered automatically *as long as the work is queued before `Context::submit`*. Work done after submit, or on a context outside the share group, would reintroduce the corruption and needs its own fence.