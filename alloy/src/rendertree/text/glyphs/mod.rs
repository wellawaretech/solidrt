// The glyph engine: the owned half of text behind the shape/draw seam
// (okf/plans/text-own-rasterizer.md). This module shapes a word to glyph
// ids and kerned positions (harfrust, the HarfBuzz team's Rust port) and
// turns glyphs into atlas cells (swash outlines and coverage masks, msdfgen
// distance fields) that live in a texture a consumer samples itself: the
// text layer every `<text>` draws through (stage 2), the 2d sprite
// layer's world-space text, a terminal grid later. The files:
//
// - fonts.rs: the registered `FontPayload`s as faces (one harfrust font and
//   the bytes swash reads, per payload, with the underline metrics), the
//   one reader of the font bytes, resolved by alias and family name, with
//   a variable font's weight axis instanced per requested weight.
// - shape.rs: a word in a run style to `ShapedGlyphs` - glyph ids, pen
//   positions, cluster map, run metrics - with the fallback policy (a
//   cluster the face lacks re-shaped on the next registered face that
//   covers it) and caret stops read off the cluster map in O(n).
// - cells.rs: a glyph to a `Cell` of pixels of a `CellKind`: a coverage
//   mask at an exact pixel size and subpixel phase (what the text layer and
//   a terminal want), or a multi-channel distance field at a fixed size per
//   em (what a zoomed consumer wants). Generated on the worker thread
//   (worker.rs) or, for a `<text>` within its frame budget, on the UI
//   thread.
// - msdf.rs: the distance-field kind, a glyph outline through the vendored
//   msdfgen (ffi.rs binds the C shim build.rs compiles with its core).
// - atlas.rs: cells packed into one growing texture (etagere's shelf
//   allocator, the one behind WebRender's texture cache), keyed by the
//   owner, with frame stamps for eviction, the CPU mirror growth repacks
//   from and the dirty rects a flush uploads.
// - text_atlas.rs: the one mask atlas every `<text>` draws from: cells
//   keyed on face, size, weight, style, phase and glyph, filled by a
//   warm-up on the worker, a budgeted synchronous path at build, and the
//   worker past the budget.
//
// Engine-independent by the rendertree rule: nothing here names a
// scripting engine, and the only renderer it touches is `crate::Context`'s
// texture registry and glyph pass. `RunStyle` carries Impeller's weight and
// style enums, which are plain data here (`fonts::weight_value`).

mod atlas;
mod cells;
mod ffi;
mod fonts;
mod msdf;
mod shape;
mod text_atlas;
mod worker;

pub use atlas::{AtlasPacker, CellKey as AtlasKey, CellPlacement, Dirty, DirtyRect, GlyphAtlas, InsertOutcome};
pub use cells::{Cell, CellKind, CellRequest, Rasterizer, BYTES_PER_TEXEL};
pub use fonts::{family_names, weight_value, Face, FaceId, FontSet};
pub use shape::{Fallback, PlacedGlyph, ShapeStyle, ShapedGlyphs};
pub use text_atlas::{split_phase, CellKey, HoldSource, StyleKey, TextAtlas, WarmRequest, WorkHold, PHASES};
pub use worker::{CellJob, CellWorker, CellsDone, JobPriority};
