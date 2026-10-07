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
// Cells are keyed by the owner's choice of `K`: a `flux:font` handle keys
// on the glyph id alone (one face, one size per atlas), the text atlas on
// the whole (face, size, weight, style, phase, glyph) tuple. Every cell
// carries the frame it was last used in; an owner that stamps its frames
// (`begin_frame`, `touch`) gets eviction of what it stopped using when the
// atlas is otherwise full. Growth doubles the smaller side up to
// `max_side`; at the cap the atlas evicts, and only when nothing can go is
// it full: `insert` says so and the glyph stays missing, which a consumer
// draws as nothing at the right advance. Every placement moves on a
// growth or a repack, so the outcome names it; an owner that hands
// placements out over a frame adds cells with `insert_in_place`, which
// never repacks, and keeps what did not fit for an `insert` at its next
// frame start, when nobody holds a placement.
use super::cells::{Cell, CellKind, BYTES_PER_TEXEL};
use crate::gpu::{SamplerState, TextureFormat, TextureRect};
use crate::Context;
use etagere::{size2, AllocId, AtlasAllocator};
use std::collections::HashMap;
use std::hash::Hash;

/// The side of a fresh atlas. Small enough to cost nothing for a font that
/// shows a few labels, one doubling away from holding ASCII at 48 texels
/// per em.
const INITIAL_SIDE: u32 = 512;
/// Transparent texels around a mask cell, so a linear tap at its edge never
/// reaches the neighbour. A distance-field cell carries its range as
/// padding of its own.
const MASK_PADDING: u32 = 1;

/// What a cell is keyed on: the owner's identity for it, plain data.
pub trait CellKey: Copy + Eq + Hash + std::fmt::Debug {}
impl<T: Copy + Eq + Hash + std::fmt::Debug> CellKey for T {}

/// Where a glyph's cell sits in the atlas, in texels, plus the cell's
/// placement relative to the glyph origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPlacement {
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
  /// Placed after a growth or an eviction repack: EVERY cell may have
  /// moved, re-read all placements.
  Moved,
  /// The atlas is at its maximum side, nothing could be evicted, and the
  /// cell does not fit.
  Full,
}

/// A dirty rect of the mirror, in texels: x, y, width, height.
pub type DirtyRect = (u32, u32, u32, u32);

/// What changed in the mirror since the last `take_dirty`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dirty {
  /// The whole mirror is new (a growth or a repack); `resized` says the
  /// texture's dimensions changed with it.
  Whole { resized: bool },
  /// Only these rects were written.
  Rects(Vec<DirtyRect>),
}

// A packed cell: where it sits, its allocation, and when it was last used.
struct Slot {
  placement: CellPlacement,
  // None for a blank cell, which takes no space.
  alloc: Option<AllocId>,
  last_used: u64,
}

/// The pure half: placements, the texel mirror and what changed.
pub struct AtlasPacker<K: CellKey> {
  kind: CellKind,
  width: u32,
  height: u32,
  max_side: u32,
  allocator: AtlasAllocator,
  cells: HashMap<K, Slot>,
  /// Every cell's pixels, for repacks.
  sources: HashMap<K, Cell<K>>,
  /// The texels as uploaded, rows top to bottom, rgba8.
  mirror: Vec<u8>,
  /// Rects the mirror changed since the last `take_dirty`.
  dirty: Vec<DirtyRect>,
  /// The whole mirror is new since the last `take_dirty` (and whether the
  /// size changed with it).
  whole: Option<bool>,
  /// The owner's frame counter, stamped on every cell used.
  frame: u64,
  /// How many frames a cell may go unused before an eviction takes it.
  evict_after: u64,
}

impl<K: CellKey> AtlasPacker<K> {
  /// An empty packer at the initial side (capped by `max_side`). `kind`
  /// decides the padding around a cell. Without `evict_after` (see
  /// `with_eviction`) a full atlas never evicts.
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
      whole: None,
      frame: 0,
      evict_after: u64::MAX,
    }
  }

  /// Evict cells unused for `frames` frames when the atlas is full at its
  /// cap (see `begin_frame` and `touch`).
  pub fn with_eviction(mut self, frames: u64) -> Self {
    self.evict_after = frames;
    self
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

  /// Start the owner's frame `frame`: what `insert` and `touch` stamp
  /// cells with from here on.
  pub fn begin_frame(&mut self, frame: u64) {
    self.frame = frame;
  }

  /// Drop every cell, keeping the size: the owner's keys are void (its
  /// fonts changed). The whole mirror is new afterwards.
  pub fn clear(&mut self) {
    self.cells.clear();
    self.sources.clear();
    self.allocator = AtlasAllocator::new(size2(self.width as i32, self.height as i32));
    self.mirror.fill(0);
    self.dirty.clear();
    if self.whole.is_none() {
      self.whole = Some(false);
    }
  }

  /// Note a use of `key`'s cell in the current frame. False when the cell
  /// is not in the atlas.
  pub fn touch(&mut self, key: K) -> bool {
    match self.cells.get_mut(&key) {
      Some(slot) => {
        slot.last_used = self.frame;
        true
      }
      None => false,
    }
  }

  /// The placement of a cell already packed.
  pub fn placement(&self, key: K) -> Option<CellPlacement> {
    self.cells.get(&key).map(|slot| slot.placement)
  }

  /// Every placement, for an owner re-reading after a move.
  pub fn placements(&self) -> impl Iterator<Item = (K, CellPlacement)> + '_ {
    self.cells.iter().map(|(key, slot)| (*key, slot.placement))
  }

  pub fn len(&self) -> usize {
    self.cells.len()
  }

  pub fn is_empty(&self) -> bool {
    self.cells.is_empty()
  }

  /// Add a cell at the current size, or hand it back when it does not
  /// fit: never grows, evicts or repacks, so every placement stays where
  /// it is. A key already present is a no-op (the cell counts as used
  /// now).
  pub fn insert_in_place(&mut self, cell: Cell<K>) -> Result<(), Cell<K>> {
    if self.touch(cell.key) {
      return Ok(());
    }
    if !self.place(&cell) {
      return Err(cell);
    }
    self.sources.insert(cell.key, cell);
    Ok(())
  }

  /// Add a cell, growing, evicting and repacking as needed (see
  /// `InsertOutcome`). A key already present is a no-op (`Placed`, and the
  /// cell counts as used now).
  pub fn insert(&mut self, cell: Cell<K>) -> InsertOutcome {
    if self.touch(cell.key) {
      return InsertOutcome::Placed;
    }
    if self.place(&cell) {
      self.sources.insert(cell.key, cell);
      return InsertOutcome::Placed;
    }
    // Grow until it fits or the cap is reached, repacking what is there.
    while self.grow() {
      if self.place(&cell) {
        self.sources.insert(cell.key, cell);
        return InsertOutcome::Moved;
      }
    }
    // At the cap: let go of what the owner stopped using, repack, retry.
    if self.evict() > 0 {
      self.repack();
      if self.place(&cell) {
        self.sources.insert(cell.key, cell);
        return InsertOutcome::Moved;
      }
    }
    InsertOutcome::Full
  }

  /// What changed since the last call, None when nothing did.
  pub fn take_dirty(&mut self) -> Option<Dirty> {
    if let Some(resized) = self.whole.take() {
      self.dirty.clear();
      return Some(Dirty::Whole { resized });
    }
    if self.dirty.is_empty() {
      return None;
    }
    Some(Dirty::Rects(std::mem::take(&mut self.dirty)))
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
  fn place(&mut self, cell: &Cell<K>) -> bool {
    // A blank glyph (a space) has no texels: it is in the atlas at a
    // zero-sized spot, drawn as nothing at its advance.
    if cell.width == 0 || cell.height == 0 {
      let placement = CellPlacement { x: 0, y: 0, width: 0, height: 0, left: cell.left, top: cell.top };
      self.cells.insert(cell.key, Slot { placement, alloc: None, last_used: self.frame });
      return true;
    }
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
    // ever placed, but a repack and an eviction reuse texels.
    self.fill_rect(x, y, w, h, 0);
    self.blit(cell, x + padding, y + padding);
    self.dirty.push((x, y, w, h));
    let placement = CellPlacement {
      x: x + padding,
      y: y + padding,
      width: cell.width,
      height: cell.height,
      left: cell.left,
      top: cell.top,
    };
    self.cells.insert(cell.key, Slot { placement, alloc: Some(alloc.id), last_used: self.frame });
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
    self.repack();
    self.whole = Some(true);
    true
  }

  // Free every cell unused for longer than `evict_after`, oldest first;
  // how many went. Blank cells hold no space and stay.
  fn evict(&mut self) -> usize {
    let Some(cutoff) = self.frame.checked_sub(self.evict_after) else {
      return 0;
    };
    let stale: Vec<K> = self
      .cells
      .iter()
      .filter(|(_, slot)| slot.alloc.is_some() && slot.last_used < cutoff)
      .map(|(key, _)| *key)
      .collect();
    for key in &stale {
      if let Some(Slot { alloc: Some(id), .. }) = self.cells.remove(key) {
        self.allocator.deallocate(id);
      }
      self.sources.remove(key);
    }
    stale.len()
  }

  // Lay every cell out again at the current size, from the sources. The
  // whole mirror is new afterwards.
  fn repack(&mut self) {
    self.allocator = AtlasAllocator::new(size2(self.width as i32, self.height as i32));
    self.mirror = vec![0u8; self.width as usize * self.height as usize * BYTES_PER_TEXEL];
    let stamps: HashMap<K, u64> = self.cells.iter().map(|(key, slot)| (*key, slot.last_used)).collect();
    self.cells.clear();
    // Tallest first packs shelves tightest; the sources map has no order,
    // so the key breaks ties for a deterministic layout.
    let mut cells: Vec<Cell<K>> = self.sources.values().cloned().collect();
    cells.sort_by(|a, b| {
      b.height
        .cmp(&a.height)
        .then(b.width.cmp(&a.width))
        .then_with(|| format!("{:?}", a.key).cmp(&format!("{:?}", b.key)))
    });
    for cell in &cells {
      // A cell that fit before fits in an atlas at least as large with the
      // same or fewer cells; this cannot fail.
      self.place(cell);
      if let (Some(slot), Some(stamp)) = (self.cells.get_mut(&cell.key), stamps.get(&cell.key)) {
        slot.last_used = *stamp;
      }
    }
    self.dirty.clear();
    if self.whole.is_none() {
      self.whole = Some(false);
    }
  }

  fn blit(&mut self, cell: &Cell<K>, x: u32, y: u32) {
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
pub struct GlyphAtlas<K: CellKey> {
  packer: AtlasPacker<K>,
  texture: u64,
}

impl<K: CellKey> GlyphAtlas<K> {
  /// Create the atlas texture (rgba8, `sampler`) in the registry over
  /// `packer`, which caps growth at the device's texture size limit, or
  /// less.
  pub fn new(ctx: &Context, packer: AtlasPacker<K>, sampler: SamplerState, label: &str) -> Result<Self, String> {
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

  pub fn packer(&self) -> &AtlasPacker<K> {
    &self.packer
  }

  pub fn packer_mut(&mut self) -> &mut AtlasPacker<K> {
    &mut self.packer
  }

  /// Add a cell (see `AtlasPacker::insert`); `flush` uploads it.
  pub fn insert(&mut self, cell: Cell<K>) -> InsertOutcome {
    self.packer.insert(cell)
  }

  /// Add a cell without moving any other (see
  /// `AtlasPacker::insert_in_place`); `flush` uploads it.
  pub fn insert_in_place(&mut self, cell: Cell<K>) -> Result<(), Cell<K>> {
    self.packer.insert_in_place(cell)
  }

  /// Upload what changed: the whole mirror after a growth (a resize at the
  /// same id) or a repack, else the dirty rects. Returns whether anything
  /// was sent.
  pub fn flush(&mut self, ctx: &Context) -> Result<bool, String> {
    let (width, height) = self.packer.size();
    match self.packer.take_dirty() {
      None => Ok(false),
      Some(Dirty::Whole { resized: true }) => {
        ctx.resize_texture(self.texture, width, height, self.packer.mirror())?;
        Ok(true)
      }
      Some(Dirty::Whole { resized: false }) => {
        let rect = TextureRect { x: 0, y: 0, width, height, pixels: self.packer.mirror().to_vec() };
        ctx.update_texture_rects(self.texture, vec![rect])?;
        Ok(true)
      }
      Some(Dirty::Rects(dirty)) => {
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
