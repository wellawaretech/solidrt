//! A view's draw list over a population the app writes: the pure half
//! (okf/design/draw-order-and-culling.md, D3). The records stay in slot
//! order where they are written; each view draws its own list of the
//! records it sees, in draw order, gathered into its own buffers. Storage,
//! visibility and order stay three things:
//!
//! - Visibility. A `CellIndex` keeps which slots sit in which cell of a
//!   residency `Lattice`, by their cull box center, updated from a dirty
//!   slot range. Under a view, the cells the view misses skip whole, the
//!   cells wholly inside it take every member, and in the cells its edge
//!   crosses each record's own box decides (with its screen reach).
//! - Order. Without a key the visible slots are taken in slot order (a bit
//!   per slot, scanned), so culling never changes what draws over what;
//!   with a key they are radix-sorted by it, stable on slot order.
//! - Materialize. `gather_list` copies one buffer's records into the
//!   view's buffer in list order, so the view draws `[0, visible)`.
//!
//! A write that leaves the list's membership and keys as they were
//! (`DrawList::change`) patches the records it touched at their list
//! positions instead of rebuilding: a moved record that stays in view
//! costs its own bytes, as on an unculled entry.
//!
//! The cost follows the visible set, not the population. This module knows
//! nothing about buffers, entries or threads (the Context does).

use std::collections::HashMap;
use std::ops::Range;

use super::order::{contains, quantize, record_key, CellBox, OrderKey, OrderScratch, OrderView, VIEW_W_FLOOR};
use crate::spatial::Lattice;

// The cell of a slot that is no cell's member (its box is empty).
const NO_CELL: u32 = u32::MAX;

// Bits per word of a draw list's visible set.
const WORD_BITS: usize = 64;

// One cell of the index: its lattice key, its member slots (unordered) and
// its bound, the union of every box its members had since the cell last
// emptied, with the largest screen reach among them. The bound grows on
// every update and never shrinks while the cell has members: too large a
// bound only costs per-record tests, never a record.
struct Cell {
  key: [i64; 3],
  slots: Vec<u32>,
  bound: [f32; 6],
  px: f32,
}

// Where a slot sits: its cell's index in `CellIndex::cells` and its
// position in that cell's slot list.
#[derive(Clone, Copy)]
struct Member {
  cell: u32,
  at: u32,
}

const NOT_A_MEMBER: Member = Member { cell: NO_CELL, at: 0 };

/// Which slots of a population sit in which cell of a lattice, by the
/// center of each slot's cull box (`CellBox`), grown by the index's
/// margin. A slot with an empty box draws nothing and is no cell's
/// member. The index is updated from the boxes of each dirty slot range
/// (`update`) before any list is built over it.
pub struct CellIndex {
  lattice: Lattice,
  margin: f32,
  cells: Vec<Cell>,
  by_key: HashMap<[i64; 3], u32>,
  members: Vec<Member>,
}

impl CellIndex {
  /// An empty index over `lattice` (`Lattice::validate`'s contract).
  pub fn new(lattice: Lattice) -> CellIndex {
    CellIndex { lattice, margin: 0.0, cells: Vec::new(), by_key: HashMap::new(), members: Vec::new() }
  }

  /// This index with every box grown by `margin` record units on every
  /// side (`Cells::margin`): what a vertex stage may draw beyond the box.
  pub fn with_margin(self, margin: f32) -> CellIndex {
    CellIndex { margin, ..self }
  }

  /// Box `slot` of `boxes` as the index sees it: grown by its margin.
  pub fn read(&self, boxes: &[f32], slot: usize) -> CellBox {
    CellBox::read(boxes, slot).grown(self.margin)
  }

  /// The lattice the index cuts by.
  #[cfg(test)]
  pub fn lattice(&self) -> &Lattice {
    &self.lattice
  }

  /// The slots the index spans: one past the highest member slot.
  pub fn span(&self) -> usize {
    self.members.len()
  }

  /// The cells with members.
  #[cfg(test)]
  pub fn cell_count(&self) -> usize {
    self.cells.len()
  }

  /// The lattice cell slot `slot` sits in; None for a slot with an empty
  /// box.
  #[cfg(test)]
  pub fn cell_of(&self, slot: usize) -> Option<[i64; 3]> {
    let m = self.members.get(slot)?;
    (m.cell != NO_CELL).then(|| self.cells[m.cell as usize].key)
  }

  /// Re-read the boxes of the slots in `dirty` from `boxes` (a
  /// `CELL_BOX_FLOATS` column in slot order; a slot past its end reads as
  /// empty, so a shrunk population passes the range it lost): a slot whose
  /// box is empty leaves its cell, any other joins the cell of its box
  /// center, and its cell's bound grows to cover the box.
  pub fn update(&mut self, boxes: &[f32], dirty: impl IntoIterator<Item = usize>) {
    for slot in dirty {
      if self.members.len() <= slot {
        self.members.resize(slot + 1, NOT_A_MEMBER);
      }
      let b = self.read(boxes, slot);
      if b.is_empty() {
        self.leave(slot);
        continue;
      }
      let key = self.cell_key(&b);
      let m = self.members[slot];
      let cell = if m.cell != NO_CELL && self.cells[m.cell as usize].key == key {
        m.cell
      } else {
        self.leave(slot);
        self.join(slot, key)
      };
      let cell = &mut self.cells[cell as usize];
      for a in 0..3 {
        cell.bound[a] = cell.bound[a].min(b.bounds[a]);
        cell.bound[a + 3] = cell.bound[a + 3].max(b.bounds[a + 3]);
      }
      cell.px = cell.px.max(b.px);
    }
    while self.members.last().is_some_and(|m| m.cell == NO_CELL) {
      self.members.pop();
    }
  }

  // The lattice cell of a box's center.
  fn cell_key(&self, b: &CellBox) -> [i64; 3] {
    let mut key = [0i64; 3];
    for (a, k) in key.iter_mut().enumerate() {
      let center = (b.bounds[a] as f64 + b.bounds[a + 3] as f64) * 0.5;
      *k = self.lattice.index(center, a);
    }
    key
  }

  // Put `slot` (no cell's member) into the cell `key`, opening the cell
  // when it has no members yet; returns the cell's index.
  fn join(&mut self, slot: usize, key: [i64; 3]) -> u32 {
    let cells = &mut self.cells;
    let c = *self.by_key.entry(key).or_insert_with(|| {
      cells.push(Cell { key, slots: Vec::new(), bound: CellBox::EMPTY.bounds, px: 0.0 });
      (cells.len() - 1) as u32
    });
    let cell = &mut self.cells[c as usize];
    self.members[slot] = Member { cell: c, at: cell.slots.len() as u32 };
    cell.slots.push(slot as u32);
    c
  }

  // Take `slot` out of its cell (if any), closing the cell when it was the
  // last member.
  fn leave(&mut self, slot: usize) {
    let m = self.members[slot];
    if m.cell == NO_CELL {
      return;
    }
    self.members[slot] = NOT_A_MEMBER;
    let cell = &mut self.cells[m.cell as usize];
    cell.slots.swap_remove(m.at as usize);
    if let Some(&moved) = cell.slots.get(m.at as usize) {
      self.members[moved as usize].at = m.at;
    }
    if cell.slots.is_empty() {
      self.close(m.cell);
    }
  }

  // Remove the empty cell `c`; the last cell takes its place.
  fn close(&mut self, c: u32) {
    let key = self.cells[c as usize].key;
    self.by_key.remove(&key);
    self.cells.swap_remove(c as usize);
    if let Some(moved) = self.cells.get(c as usize) {
      self.by_key.insert(moved.key, c);
      for &slot in &moved.slots {
        self.members[slot as usize].cell = c;
      }
    }
  }
}

/// The key a draw list is sorted by: `key` read from `records` (the key
/// buffer's records in slot order, `stride` bytes each), ascending or
/// descending. A slot past `records` reads as a zero record, as
/// `gather_list` writes it.
#[derive(Clone, Copy, Debug)]
pub struct ListOrder<'a> {
  pub key: OrderKey,
  pub descending: bool,
  pub stride: usize,
  pub records: &'a [u8],
}

impl ListOrder<'_> {
  // The sort key of slot `slot`.
  fn slot_key(&self, slot: u32) -> u32 {
    let at = slot as usize * self.stride;
    if at + self.stride <= self.records.len() {
      record_key(&self.key, self.descending, self.records, at)
    } else {
      let zero = quantize(0.0);
      if self.descending {
        !zero
      } else {
        zero
      }
    }
  }
}

/// What a write does to a draw list whose view and population did not
/// change (`DrawList::change`).
#[derive(Clone, Debug, PartialEq)]
pub enum ListChange {
  /// No written slot is drawn or becomes drawn.
  None,
  /// The list keeps its slots and order; the records in this span of list
  /// positions (from the first changed one through the last) are
  /// re-gathered there, as one write per buffer: a buffer write into a
  /// buffer the GPU still reads costs per write, not per byte (an Adreno
  /// 610 drew a third fewer frames with 28 writes of 3,200 records a
  /// frame than with one of 5,700).
  Patch(Range<usize>),
  /// A written slot joined or left the view, or a drawn one changed its
  /// key: the list is built again.
  Rebuild,
}

// How a cell meets a view.
enum Reach {
  Outside,
  Inside,
  Edge,
}

// How cell `cell` meets `view`: Inside when its bound lies wholly in it
// (every member's box then does), Outside when its bound grown by its
// screen reach does not reach it (no member's grown box then does), Edge
// otherwise. A bound that is not finite, or that reaches to or behind the
// eye with a screen reach, is Edge: `reaches` tests such a box ungrown,
// which does not cover its members' grown boxes.
fn reach(view: &OrderView, planes: &[[f32; 4]; 6], cell: &Cell, viewport: [f32; 2]) -> Reach {
  let b = &cell.bound;
  if !b.iter().all(|v| v.is_finite()) {
    return Reach::Edge;
  }
  if contains(planes, b) {
    return Reach::Inside;
  }
  if cell.px > 0.0 {
    let (w_min, w_max) = view.w_range(b);
    if !(w_min > VIEW_W_FLOOR && w_max.is_finite()) {
      return Reach::Edge;
    }
  }
  if view.reaches(planes, b, cell.px, viewport) {
    Reach::Edge
  } else {
    Reach::Outside
  }
}

/// One view's draw list: the slots it draws, in draw order, and what a
/// later write is checked against (`change`): the visible set of its last
/// build, each visible slot's list position, and the key each position
/// was sorted by. Its memory is reused across builds.
#[derive(Default)]
pub struct DrawList {
  visible: Vec<u64>,
  list: Vec<u32>,
  // The list position of each visible slot; stale for the others.
  position: Vec<u32>,
  // The key of each list position at the last build; empty without a key.
  keys: Vec<u32>,
}

impl DrawList {
  /// The slots to draw, in draw order.
  pub fn list(&self) -> &[u32] {
    &self.list
  }

  /// The visible record count of the last build.
  pub fn len(&self) -> usize {
    self.list.len()
  }

  /// Build the list of `index`'s population under `view` (the node's
  /// model-to-clip matrix; None draws every member) over `viewport` (the
  /// pixels a screen reach is measured in): the visible set from the
  /// cells, then from `boxes` (the column `index` was last updated from)
  /// in the cells the view's edge crosses; then the visible slots in slot
  /// order, sorted by `order`'s key when there is one, stable. Returns the
  /// visible count.
  pub fn build(
    &mut self,
    index: &CellIndex,
    boxes: &[f32],
    view: Option<&OrderView>,
    viewport: [f32; 2],
    order: Option<&ListOrder>,
    scratch: &mut OrderScratch,
  ) -> usize {
    let visible = &mut self.visible;
    visible.clear();
    visible.resize(index.span().div_ceil(WORD_BITS), 0);
    let mut set = |slot: u32| visible[slot as usize / WORD_BITS] |= 1 << (slot as usize % WORD_BITS);
    match view {
      None => index.cells.iter().flat_map(|c| &c.slots).for_each(|&s| set(s)),
      Some(view) => {
        let planes = view.planes();
        for cell in &index.cells {
          match reach(view, &planes, cell, viewport) {
            Reach::Outside => {}
            Reach::Inside => cell.slots.iter().for_each(|&s| set(s)),
            Reach::Edge => {
              for &s in &cell.slots {
                let b = index.read(boxes, s as usize);
                if view.reaches(&planes, &b.bounds, b.px, viewport) {
                  set(s);
                }
              }
            }
          }
        }
      }
    }
    self.list.clear();
    for (w, &word) in self.visible.iter().enumerate() {
      let mut bits = word;
      while bits != 0 {
        self.list.push((w * WORD_BITS) as u32 + bits.trailing_zeros());
        bits &= bits - 1;
      }
    }
    self.sort(order, scratch);
    self.index_positions(order);
    self.list.len()
  }

  /// Build the list of a population drawn unculled: every slot below
  /// `population` visible, in slot order, sorted by `order`'s key when
  /// there is one, stable. Returns the visible count.
  pub fn build_all(&mut self, population: usize, order: Option<&ListOrder>, scratch: &mut OrderScratch) -> usize {
    self.visible.clear();
    self.visible.resize(population.div_ceil(WORD_BITS), u64::MAX);
    if population % WORD_BITS != 0 {
      if let Some(last) = self.visible.last_mut() {
        *last = u64::MAX >> (WORD_BITS - population % WORD_BITS);
      }
    }
    self.list.clear();
    self.list.extend(0..population as u32);
    self.sort(order, scratch);
    self.index_positions(order);
    self.list.len()
  }

  // Sort the list by `order`'s key, stable on the order it holds.
  fn sort(&mut self, order: Option<&ListOrder>, scratch: &mut OrderScratch) {
    let Some(o) = order else {
      return;
    };
    scratch.sort_slots(&mut self.list, |slot| o.slot_key(slot));
  }

  // Record each listed slot's position and, with a key, each position's
  // key: what `change` compares a later write against.
  fn index_positions(&mut self, order: Option<&ListOrder>) {
    let span = self.visible.len() * WORD_BITS;
    if self.position.len() < span {
      self.position.resize(span, 0);
    }
    for (at, &slot) in self.list.iter().enumerate() {
      self.position[slot as usize] = at as u32;
    }
    self.keys.clear();
    if let Some(o) = order {
      self.keys.extend(self.list.iter().map(|&slot| o.slot_key(slot)));
    }
  }

  // Whether slot `slot` was visible in the last build.
  fn was_visible(&self, slot: usize) -> bool {
    self.visible.get(slot / WORD_BITS).is_some_and(|w| w & (1 << (slot % WORD_BITS)) != 0)
  }

  /// What a write to the slots in `dirty` (ascending, each once) does to a
  /// culled list (`build`),
  /// its view and population unchanged since the last build: the slots'
  /// new boxes in `boxes` (the column `index` was updated from) decide
  /// what is drawn under `view` (None: every member), as the build would,
  /// and `order` reads the keys as written. A slot that joined or left the
  /// drawn set, or a drawn one whose key moved, needs a rebuild; otherwise
  /// the drawn slots written are patched in place.
  pub fn change_culled(
    &self,
    dirty: &[u32],
    index: &CellIndex,
    boxes: &[f32],
    view: Option<&OrderView>,
    viewport: [f32; 2],
    order: Option<&ListOrder>,
  ) -> ListChange {
    match view {
      None => self.change(dirty, |s| !index.read(boxes, s).is_empty(), order),
      Some(view) => {
        let planes = view.planes();
        self.change(
          dirty,
          |s| {
            let b = index.read(boxes, s);
            !b.is_empty() && view.reaches(&planes, &b.bounds, b.px, viewport)
          },
          order,
        )
      }
    }
  }

  /// What a write to the slots in `dirty` (ascending, each once, inside the
  /// population) does to an unculled list (`build_all`): every slot is
  /// drawn, so only a drawn record's key moving rebuilds it.
  pub fn change_all(&self, dirty: &[u32], order: Option<&ListOrder>) -> ListChange {
    self.change(dirty, |_| true, order)
  }

  // The classification both forms share: `drawn(slot)` says whether the
  // slot is drawn as written.
  fn change(&self, dirty: &[u32], drawn: impl Fn(usize) -> bool, order: Option<&ListOrder>) -> ListChange {
    let mut span: Option<Range<usize>> = None;
    for &slot in dirty {
      let slot = slot as usize;
      let was = self.was_visible(slot);
      if was != drawn(slot) {
        return ListChange::Rebuild;
      }
      if !was {
        continue;
      }
      let at = self.position[slot] as usize;
      if order.is_some_and(|o| o.slot_key(slot as u32) != self.keys[at]) {
        return ListChange::Rebuild;
      }
      span = Some(match span {
        Some(s) => s.start.min(at)..s.end.max(at + 1),
        None => at..at + 1,
      });
    }
    match span {
      Some(span) => ListChange::Patch(span),
      None => ListChange::None,
    }
  }
}

/// Gather the records of `list` from `src` (one buffer's records in slot
/// order, `stride` bytes each) into `dst` in list order: record i of `dst`
/// is slot `list[i]`. A slot past `src` writes a zero record, so every
/// buffer of an entry stays aligned on the list. `dst` holds at least
/// `list.len() * stride` bytes.
pub fn gather_list(list: &[u32], stride: usize, src: &[u8], dst: &mut [u8]) {
  debug_assert!(stride > 0 && dst.len() >= list.len() * stride, "gather destination smaller than the list");
  for (out, &slot) in dst.chunks_exact_mut(stride).zip(list) {
    let at = slot as usize * stride;
    match src.get(at..at + stride) {
      Some(record) => out.copy_from_slice(record),
      None => out.fill(0),
    }
  }
}
