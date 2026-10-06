// The glyph atlas: cells packed into one texture that grows as glyphs
// arrive. Two layers: `AtlasPacker` is the pure half (etagere's shelf
// allocator placing cells, a CPU mirror of the texels, the dirty rects a
// flush uploads, growth as a repack from the mirror), checkable without a
// GPU; `GlyphAtlas` puts a packer behind a registered texture in
// `crate::Context` and turns its dirty state into uploads. etagere is the
// allocator behind WebRender's texture cache: fast, and shelves keep
// placements stable until a repack. The texture is sampled by whoever
// declares the id; its contents are the engine's, its life the owner's
// (`destroy`).
//
// Growth doubles the smaller side up to `max_side`, then the atlas is full:
// `insert` says so and the glyph stays missing, which a consumer draws as
// nothing at the right advance. Every placement moves on growth, so the
// outcome names it and the owner re-reads every cell it hands out.
use super::cells::{Cell, CellKind, BYTES_PER_TEXEL};
use crate::gpu::{SamplerState, TextureFormat, TextureRect};
use crate::Context;
use etagere::{size2, AtlasAllocator};
use std::collections::HashMap;

/// The side of a fresh atlas. Small enough to cost nothing for a font that
/// shows a few labels, one doubling away from holding ASCII at 48 texels
/// per em.
const INITIAL_SIDE: u32 = 512;
/// Transparent texels around a mask cell, so a linear tap at its edge never
/// reaches the neighbour. A distance-field cell carries its range as
/// padding of its own.
const MASK_PADDING: u32 = 1;

/// Where a glyph's cell sits in the atlas, in texels, plus the cell's
/// placement relative to the glyph origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPlacement {
  pub glyph: u16,
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
  pub left: i32,
  pub top: i32,
}

/// What an insert did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertOutcome {
  /// Placed; the other cells stayed where they were.
  Placed,
  /// Placed after a growth: EVERY cell moved, re-read all placements.
  Grew,
  /// The atlas is at its maximum side and the cell does not fit.
  Full,
}

/// A dirty rect of the mirror, in texels: x, y, width, height.
pub type DirtyRect = (u32, u32, u32, u32);

/// The pure half: placements, the texel mirror and what changed.
pub struct AtlasPacker {
  kind: CellKind,
  width: u32,
  height: u32,
  max_side: u32,
  allocator: AtlasAllocator,
  cells: HashMap<u16, CellPlacement>,
  /// Every cell's pixels, for growth repacks.
  sources: HashMap<u16, Cell>,
  /// The texels as uploaded, rows top to bottom, rgba8.
  mirror: Vec<u8>,
  /// Rects the mirror changed since the last `take_dirty`.
  dirty: Vec<DirtyRect>,
  /// A growth happened since the last `take_dirty`: the whole mirror is new.
  grew: bool,
}

impl AtlasPacker {
  /// An empty packer at the initial side (capped by `max_side`).
  pub fn new(kind: CellKind, max_side: u32) -> Self {
    let side = INITIAL_SIDE.min(max_side);
    Self {
      kind,
      width: side,
      height: side,
      max_side,
      allocator: AtlasAllocator::new(size2(side as i32, side as i32)),
      cells: HashMap::new(),
      sources: HashMap::new(),
      mirror: vec![0u8; side as usize * side as usize * BYTES_PER_TEXEL],
      dirty: Vec::new(),
      grew: false,
    }
  }

  pub fn kind(&self) -> CellKind {
    self.kind
  }

  pub fn size(&self) -> (u32, u32) {
    (self.width, self.height)
  }

  /// The texels, rows top to bottom, rgba8: what the texture holds after a
  /// flush.
  pub fn mirror(&self) -> &[u8] {
    &self.mirror
  }

  /// The placement of a glyph already packed.
  pub fn placement(&self, glyph: u16) -> Option<CellPlacement> {
    self.cells.get(&glyph).copied()
  }

  /// Every placement, for an owner re-reading after a growth.
  pub fn placements(&self) -> impl Iterator<Item = CellPlacement> + '_ {
    self.cells.values().copied()
  }

  pub fn len(&self) -> usize {
    self.cells.len()
  }

  pub fn is_empty(&self) -> bool {
    self.cells.is_empty()
  }

  /// Add a cell. A glyph already present is a no-op (`Placed`).
  pub fn insert(&mut self, cell: Cell) -> InsertOutcome {
    if self.cells.contains_key(&cell.glyph) {
      return InsertOutcome::Placed;
    }
    if self.place(&cell) {
      self.sources.insert(cell.glyph, cell);
      return InsertOutcome::Placed;
    }
    // Grow until it fits or the cap is reached, repacking what is there.
    while self.grow() {
      if self.place(&cell) {
        self.sources.insert(cell.glyph, cell);
        return InsertOutcome::Grew;
      }
    }
    InsertOutcome::Full
  }

  /// What changed since the last call: `Some(None)` after a growth (the
  /// whole mirror is new), `Some(rects)` for the rects written since, None
  /// when nothing changed.
  pub fn take_dirty(&mut self) -> Option<Option<Vec<DirtyRect>>> {
    if self.grew {
      self.grew = false;
      self.dirty.clear();
      return Some(None);
    }
    if self.dirty.is_empty() {
      return None;
    }
    Some(Some(std::mem::take(&mut self.dirty)))
  }

  /// The mirror's texels of a rect, rows top to bottom.
  pub fn copy_out(&self, x: u32, y: u32, width: u32, height: u32) -> Vec<u8> {
    let row_bytes = width as usize * BYTES_PER_TEXEL;
    let stride = self.width as usize * BYTES_PER_TEXEL;
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
      let from = (y as usize + row) * stride + x as usize * BYTES_PER_TEXEL;
      out.extend_from_slice(&self.mirror[from..from + row_bytes]);
    }
    out
  }

  // Allocate and blit `cell` at the current size.
  fn place(&mut self, cell: &Cell) -> bool {
    let padding = match self.kind {
      CellKind::Mask { .. } => MASK_PADDING,
      CellKind::Msdf { .. } => 0,
    };
    let (w, h) = (cell.width + 2 * padding, cell.height + 2 * padding);
    let Some(alloc) = self.allocator.allocate(size2(w as i32, h as i32)) else {
      return false;
    };
    let x = alloc.rectangle.min.x as u32;
    let y = alloc.rectangle.min.y as u32;
    // The padding ring is cleared too: the mirror is zero where nothing was
    // ever placed, but a repack reuses texels.
    self.fill_rect(x, y, w, h, 0);
    self.blit(cell, x + padding, y + padding);
    self.dirty.push((x, y, w, h));
    let placement = CellPlacement {
      glyph: cell.glyph,
      x: x + padding,
      y: y + padding,
      width: cell.width,
      height: cell.height,
      left: cell.left,
      top: cell.top,
    };
    self.cells.insert(cell.glyph, placement);
    true
  }

  // Double the smaller side (the square stays square) and repack every
  // cell. False at the cap.
  fn grow(&mut self) -> bool {
    let (width, height) =
      if self.width <= self.height { (self.width * 2, self.height) } else { (self.width, self.height * 2) };
    if width > self.max_side || height > self.max_side {
      return false;
    }
    self.width = width;
    self.height = height;
    self.allocator = AtlasAllocator::new(size2(width as i32, height as i32));
    self.mirror = vec![0u8; width as usize * height as usize * BYTES_PER_TEXEL];
    self.cells.clear();
    self.grew = true;
    // Tallest first packs shelves tightest; the sources map has no order.
    let mut cells: Vec<Cell> = self.sources.values().cloned().collect();
    cells.sort_by(|a, b| b.height.cmp(&a.height).then(b.width.cmp(&a.width)).then(a.glyph.cmp(&b.glyph)));
    for cell in &cells {
      // A cell that fit before fits in a larger atlas; this cannot fail.
      self.place(cell);
    }
    self.dirty.clear();
    true
  }

  fn blit(&mut self, cell: &Cell, x: u32, y: u32) {
    let row_bytes = cell.width as usize * BYTES_PER_TEXEL;
    let stride = self.width as usize * BYTES_PER_TEXEL;
    for row in 0..cell.height as usize {
      let from = row * row_bytes;
      let to = (y as usize + row) * stride + x as usize * BYTES_PER_TEXEL;
      self.mirror[to..to + row_bytes].copy_from_slice(&cell.pixels[from..from + row_bytes]);
    }
  }

  fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, value: u8) {
    let stride = self.width as usize * BYTES_PER_TEXEL;
    let row_bytes = w as usize * BYTES_PER_TEXEL;
    for row in 0..h as usize {
      let start = (y as usize + row) * stride + x as usize * BYTES_PER_TEXEL;
      self.mirror[start..start + row_bytes].fill(value);
    }
  }
}

/// A packer behind a registered texture.
pub struct GlyphAtlas {
  packer: AtlasPacker,
  texture: u64,
}

impl GlyphAtlas {
  /// Create the atlas texture (rgba8, `sampler`) in the registry. `max_side`
  /// caps growth: the device's texture size limit, or less.
  pub fn new(ctx: &Context, kind: CellKind, sampler: SamplerState, max_side: u32, label: &str) -> Result<Self, String> {
    let packer = AtlasPacker::new(kind, max_side);
    let (width, height) = packer.size();
    let texture = ctx.create_texture_from_pixels(
      width,
      height,
      packer.mirror(),
      sampler,
      TextureFormat::Rgba8,
      Some(label.to_string()),
    )?;
    Ok(Self { packer, texture })
  }

  pub fn texture(&self) -> u64 {
    self.texture
  }

  pub fn packer(&self) -> &AtlasPacker {
    &self.packer
  }

  /// Add a cell (see `AtlasPacker::insert`); `flush` uploads it.
  pub fn insert(&mut self, cell: Cell) -> InsertOutcome {
    self.packer.insert(cell)
  }

  /// Upload what changed: the whole mirror after a growth (a resize at the
  /// same id), else the dirty rects. Returns whether anything was sent.
  pub fn flush(&mut self, ctx: &Context) -> Result<bool, String> {
    match self.packer.take_dirty() {
      None => Ok(false),
      Some(None) => {
        let (width, height) = self.packer.size();
        ctx.resize_texture(self.texture, width, height, self.packer.mirror())?;
        Ok(true)
      }
      Some(Some(dirty)) => {
        let rects = dirty
          .into_iter()
          .map(|(x, y, width, height)| TextureRect {
            x,
            y,
            width,
            height,
            pixels: self.packer.copy_out(x, y, width, height),
          })
          .collect();
        ctx.update_texture_rects(self.texture, rects)?;
        Ok(true)
      }
    }
  }

  /// Free the texture. The atlas is unusable after this.
  pub fn destroy(&mut self, ctx: &Context) {
    ctx.destroy_texture(self.texture);
  }
}
