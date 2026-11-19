// The glyph atlas: cells packed into one texture that grows as glyphs
// arrive. Two layers: `AtlasPacker` is the pure half (etagere's shelf
// allocator placing cells, every cell's pixels, the cells a flush uploads,
// growth as a repack from the cells), checkable without a GPU; `GlyphAtlas`
// puts a packer behind a registered texture in `crate::Context` and turns
// its dirty state into uploads. etagere is the allocator behind
// WebRender's texture cache: fast, and shelves keep
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
//
// What the packer keeps is every cell's pixels (`sources`), for a repack and
// for an upload; it keeps no copy of the texture. A whole upload (`texels`)
// is built from the cells on demand and dropped after it. That rests on one
// invariant: the GPU texture's free texels are zero. The texture is created
// from a zero-filled whole upload, every whole upload (growth, repack,
// clear, context loss) is zero where no cell sits, and an eviction, the one
// way a cell's texels are freed, is always followed by a repack (`insert`).
// So a cell placed in place lands on texels that are zero on the GPU, its
// padding ring needs no write, and a newly placed cell uploads only its own
// texels (`cell_texels`).
use super::cells::{Cell, CellKind};
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

/// What changed in the atlas since the last `take_dirty`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dirty<K> {
  /// The whole atlas is new (a growth, a repack, a clear, a lost texture);
  /// `resized` says the texture's dimensions changed with it.
  Whole { resized: bool },
  /// Only these cells were placed, in place, onto free (zero) texels.
  Cells(Vec<K>),
}

// A packed cell: where it sits, its allocation, and when it was last used.
struct Slot {
  placement: CellPlacement,
  // None for a blank cell, which takes no space.
  alloc: Option<AllocId>,
  last_used: u64,
}

/// The pure half: placements, every cell's pixels and what changed.
pub struct AtlasPacker<K: CellKey> {
  kind: CellKind,
  /// What the texture holds: R8 for a mask atlas the glyph pass reads
  /// (one byte a texel, the cell's own form), rgba8 for a distance field
  /// and for a mask atlas sampled as a plain texture, where a mask cell
  /// expands to premultiplied white on upload.
  format: TextureFormat,
  /// Bytes per texel of `format`.
  texel: usize,
  width: u32,
  height: u32,
  max_side: u32,
  allocator: AtlasAllocator,
  cells: HashMap<K, Slot>,
  /// Every cell's pixels, for repacks and uploads.
  sources: HashMap<K, Cell<K>>,
  /// Cells placed in place since the last `take_dirty`.
  dirty: Vec<K>,
  /// The whole atlas is new since the last `take_dirty` (and whether the
  /// size changed with it).
  whole: Option<bool>,
  /// The owner's frame counter, stamped on every cell used.
  frame: u64,
  /// How many frames a cell may go unused before an eviction takes it.
  evict_after: u64,
  /// Keys an eviction took since the last `take_evicted`.
  evicted: Vec<K>,
}

impl<K: CellKey> AtlasPacker<K> {
  /// An empty packer at the initial side (capped by `max_side`). `kind`
  /// decides the padding around a cell and the texel format: R8 for a
  /// mask, rgba8 for a distance field (see `with_format`). Without
  /// `evict_after` (see `with_eviction`) a full atlas never evicts.
  pub fn new(kind: CellKind, max_side: u32) -> Self {
    let side = INITIAL_SIDE.min(max_side);
    let format = match kind {
      CellKind::Mask { .. } => TextureFormat::R8,
      CellKind::Msdf { .. } => TextureFormat::Rgba8,
    };
    Self {
      kind,
      format,
      texel: format.byte_len(1, 1),
      width: side,
      height: side,
      max_side,
      allocator: AtlasAllocator::new(size2(side as i32, side as i32)),
      cells: HashMap::new(),
      sources: HashMap::new(),
      dirty: Vec::new(),
      whole: None,
      frame: 0,
      evict_after: u64::MAX,
      evicted: Vec::new(),
    }
  }

  /// Hold the texels in `format` instead of the kind's own: rgba8 for a
  /// mask atlas that is sampled as a plain texture (a mask cell expands
  /// to premultiplied white, so white text to tint). R8 holds a mask
  /// only; a distance field has three channels and the true distance.
  /// For an empty packer, before anything is placed.
  pub fn with_format(mut self, format: TextureFormat) -> Self {
    let holds = match format {
      TextureFormat::Rgba8 => true,
      TextureFormat::R8 => matches!(self.kind, CellKind::Mask { .. }),
      _ => false,
    };
    assert!(holds, "a {} atlas cannot hold {:?} cells", format.name(), self.kind);
    assert!(self.cells.is_empty(), "the format is fixed before the first cell");
    self.format = format;
    self.texel = format.byte_len(1, 1);
    self
  }

  /// Evict cells unused for `frames` frames when the atlas is full at its
  /// cap (see `begin_frame` and `touch`).
  pub fn with_eviction(mut self, frames: u64) -> Self {
    self.evict_after = frames;
    self
  }

  /// Set the growth cap to `max_side` (the device's texture size limit,
  /// known once there is a GPU context, or an owner's lower policy cap);
  /// never lowered below the current size, which cells already occupy.
  pub fn set_max_side(&mut self, max_side: u32) {
    self.max_side = max_side.max(self.width).max(self.height);
  }

  /// The keys evictions took since the last call: what an owner that
  /// remembers more than the atlas holds (a warmed style) reconciles
  /// against.
  pub fn take_evicted(&mut self) -> Vec<K> {
    std::mem::take(&mut self.evicted)
  }

  pub fn kind(&self) -> CellKind {
    self.kind
  }

  /// The texture format the atlas is in (see `with_format`).
  pub fn format(&self) -> TextureFormat {
    self.format
  }

  pub fn size(&self) -> (u32, u32) {
    (self.width, self.height)
  }

  /// The whole atlas, rows top to bottom, in `format`: every non-blank
  /// cell's pixels at its placement, zero elsewhere. What the texture holds
  /// after a flush; built on demand for a whole upload, never kept.
  pub fn texels(&self) -> Vec<u8> {
    let stride = self.width as usize * self.texel;
    let mut out = vec![0u8; stride * self.height as usize];
    for (key, slot) in &self.cells {
      let Some(cell) = self.sources.get(key) else { continue };
      if slot.alloc.is_none() {
        continue;
      }
      self.expand(cell, &mut out, stride, slot.placement.x, slot.placement.y);
    }
    out
  }

  /// One cell's placement and its pixels in the atlas format, rows top to
  /// bottom, no padding: what an in-place upload writes. None for a key not
  /// in the atlas.
  pub fn cell_texels(&self, key: K) -> Option<(CellPlacement, Vec<u8>)> {
    let placement = self.cells.get(&key)?.placement;
    let cell = self.sources.get(&key)?;
    let stride = cell.width as usize * self.texel;
    let mut out = vec![0u8; stride * cell.height as usize];
    self.expand(cell, &mut out, stride, 0, 0);
    Some((placement, out))
  }

  /// Start the owner's frame `frame`: what `insert` and `touch` stamp
  /// cells with from here on.
  pub fn begin_frame(&mut self, frame: u64) {
    self.frame = frame;
  }

  /// Drop every cell, keeping the size: the owner's keys are void (its
  /// fonts changed). The whole atlas uploads afterwards.
  pub fn clear(&mut self) {
    self.cells.clear();
    self.sources.clear();
    self.allocator = AtlasAllocator::new(size2(self.width as i32, self.height as i32));
    self.dirty.clear();
    if self.whole.is_none() {
      self.whole = Some(false);
    }
  }

  /// The GPU side is void (its context was lost and the texture came back
  /// with undefined content): the whole atlas, built from the cells,
  /// uploads at the next flush.
  pub fn mark_whole(&mut self) {
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
  pub fn take_dirty(&mut self) -> Option<Dirty<K>> {
    if let Some(resized) = self.whole.take() {
      self.dirty.clear();
      return Some(Dirty::Whole { resized });
    }
    if self.dirty.is_empty() {
      return None;
    }
    Some(Dirty::Cells(std::mem::take(&mut self.dirty)))
  }

  // Allocate `cell` at the current size and note it for upload.
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
    // The padding ring is zero on the GPU already (the free texels are, see
    // the module header): only the cell's own texels upload.
    self.dirty.push(cell.key);
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
    let count = stale.len();
    self.evicted.extend(stale);
    count
  }

  // Lay every cell out again at the current size, from the sources. The
  // whole atlas uploads afterwards.
  fn repack(&mut self) {
    self.allocator = AtlasAllocator::new(size2(self.width as i32, self.height as i32));
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

  // Write the cell's pixels into `out` (rows `stride` bytes apart, in
  // `format`) at texel (x, y); a cell in a narrower form than the atlas (a
  // mask in an rgba8 atlas) fills every channel of a texel with its one
  // byte: premultiplied white. What `texels` and `cell_texels` share.
  fn expand(&self, cell: &Cell<K>, out: &mut [u8], stride: usize, x: u32, y: u32) {
    let cell_texel = cell.bytes_per_texel();
    for row in 0..cell.height as usize {
      let from = row * cell.width as usize * cell_texel;
      let to = (y as usize + row) * stride + x as usize * self.texel;
      if cell_texel == self.texel {
        let row_bytes = cell.width as usize * self.texel;
        out[to..to + row_bytes].copy_from_slice(&cell.pixels[from..from + row_bytes]);
      } else {
        debug_assert_eq!(cell_texel, 1, "a cell is one byte a texel or the atlas's own form");
        for (texel, &value) in cell.pixels[from..from + cell.width as usize].iter().enumerate() {
          let at = to + texel * self.texel;
          out[at..at + self.texel].fill(value);
        }
      }
    }
  }
}

/// A packer behind a registered texture.
pub struct GlyphAtlas<K: CellKey> {
  packer: AtlasPacker<K>,
  texture: u64,
}

impl<K: CellKey> GlyphAtlas<K> {
  /// Create the atlas texture (the packer's format, `sampler`) in the
  /// registry over `packer`, which caps growth at the device's texture
  /// size limit, or less.
  pub fn new(ctx: &Context, mut packer: AtlasPacker<K>, sampler: SamplerState, label: &str) -> Result<Self, String> {
    let texture = create_texture(ctx, &mut packer, sampler, label)?;
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

  /// Upload what changed: the whole atlas, built from the cells, after a
  /// growth or a repack (onto a new texture at the same id, see
  /// `flush_texture`), else the cells placed in place. Returns whether anything was sent.
  pub fn flush(&mut self, ctx: &Context) -> Result<bool, String> {
    flush_texture(ctx, self.texture, &mut self.packer)
  }

  /// Free the texture. The atlas is unusable after this.
  pub fn destroy(&mut self, ctx: &Context) {
    ctx.destroy_texture(self.texture);
  }
}

/// Create the registry texture for `packer` (its format, `sampler`) from
/// its cells as they stand (`texels`, zero where no cell sits): whatever
/// was packed before there was a GPU context is on the texture at once, and
/// the packer's dirty state is taken, so the next `flush_texture` uploads
/// only what changes from here.
pub fn create_texture<K: CellKey>(
  ctx: &Context,
  packer: &mut AtlasPacker<K>,
  sampler: SamplerState,
  label: &str,
) -> Result<u64, String> {
  let (width, height) = packer.size();
  let texture = ctx.create_texture_from_pixels(
    width,
    height,
    &packer.texels(),
    sampler,
    packer.format(),
    Some(label.to_string()),
  )?;
  packer.take_dirty();
  Ok(texture)
}

/// Upload what changed in `packer` to `texture`: the whole atlas, built
/// from the cells, after a growth or a repack, else one rect per cell
/// placed in place. Returns whether anything was sent. A whole upload goes
/// onto a new texture at the same id (a resize, at the same size for a
/// repack): a repack moves cells as a growth does, and a recording holds
/// placements into the texture it was made against, so that texture must
/// stay whole under every handle that holds it until those recordings
/// re-record. A placed cell writes free texels only, which no placement
/// names, so it goes into the texture in place; its padding ring is zero
/// there already (see the module header).
pub fn flush_texture<K: CellKey>(ctx: &Context, texture: u64, packer: &mut AtlasPacker<K>) -> Result<bool, String> {
  let (width, height) = packer.size();
  match packer.take_dirty() {
    None => Ok(false),
    Some(Dirty::Whole { .. }) => {
      ctx.resize_texture(texture, width, height, &packer.texels())?;
      Ok(true)
    }
    Some(Dirty::Cells(keys)) => {
      let rects: Vec<TextureRect> = keys
        .into_iter()
        .filter_map(|key| packer.cell_texels(key))
        .filter(|(p, _)| p.width > 0 && p.height > 0)
        .map(|(p, pixels)| TextureRect { x: p.x, y: p.y, width: p.width, height: p.height, pixels })
        .collect();
      if rects.is_empty() {
        return Ok(false);
      }
      ctx.update_texture_rects(texture, rects)?;
      Ok(true)
    }
  }
}
