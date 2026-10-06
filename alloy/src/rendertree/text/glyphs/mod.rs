// The glyph engine: the owned half of text behind the shape/draw seam
// (okf/plans/text-own-rasterizer.md, stage 1). Impeller still shapes and
// draws every `<text>`; this module shapes a word to glyph ids and kerned
// positions (harfrust, the HarfBuzz team's Rust port) and turns glyphs into
// atlas cells (swash outlines and coverage masks, msdfgen distance fields)
// that live in a texture a consumer samples itself - the 2d sprite layer's
// world-space text first, the draw half of `<text>` and a terminal grid
// later. Four pieces, one per file:
//
// - fonts.rs: the registered `FontPayload`s as faces (one harfrust font and
//   the bytes swash reads, per payload), resolved by alias and family name
//   exactly as the underline metrics table resolves them, with a variable
//   font's weight axis instanced per requested weight.
// - shape.rs: a word in a run style to `ShapedGlyphs` - glyph ids, pen
//   positions, cluster map, run metrics - and caret stops read off the
//   cluster map in O(n) (the Impeller path re-shapes every prefix).
// - cells.rs: a glyph to a `Cell` of pixels of a `CellKind`: a coverage
//   mask at an exact pixel size (what the draw half and a terminal want), or
//   a multi-channel distance field at a fixed size per em (what a zoomed
//   consumer wants). Generated on the worker thread (worker.rs), never on
//   the frame.
// - atlas.rs: cells packed into one growing texture (etagere's shelf
//   allocator, the one behind WebRender's texture cache), with the CPU
//   mirror growth repacks from and the dirty rects a flush uploads.
//
// Engine-independent by the rendertree rule: nothing here names a
// scripting engine, and the only renderer it touches is `crate::Context`'s
// texture registry. `RunStyle` carries Impeller's weight and style enums,
// which are plain data here (`fonts::weight_value`).

mod atlas;
mod cells;
mod fonts;
mod shape;
mod worker;

pub use atlas::{AtlasPacker, CellPlacement, DirtyRect, GlyphAtlas, InsertOutcome};
pub use cells::{Cell, CellKind, CellRequest, Rasterizer, BYTES_PER_TEXEL};
pub use fonts::{family_names, weight_value, Face, FaceId, FontSet};
pub use shape::{PlacedGlyph, ShapedGlyphs};
pub use worker::{CellJob, CellWorker, CellsDone};
