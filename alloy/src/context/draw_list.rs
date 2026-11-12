//! The draw lists of the instance-order registry: an entry with an
//! instance order over records the app writes draws, per view, the list
//! of the records its view sees (okf/design/draw-order-and-culling.md, D3;
//! the pure half in gpu/draw_list.rs). The records stay in slot order in
//! the buffer mirrors (context/order.rs), where every write lands as a
//! dirty range, never the GPU. At the rebuild point each list whose view,
//! key or population changed, or whose drawn set or keys a write changed,
//! culls, sorts and gathers its visible records into its own buffers, and
//! the raster thread draws `[0, visible)` from them as a plain instanced
//! draw; a write that changed neither re-gathers only the records it
//! touched, at their list positions. Every list is its own: two views of
//! one population cull and order apart.
//!
//! A list's write into its own buffers carries the dirt it leaves on its
//! target (D8 of okf/design/draw-order-and-culling.md): for a culled list
//! whose view held still, the boxes of the slots the point touched, as they
//! were at the last point and as they are now, projected into the target's
//! pixels (`ListDamage`); in full otherwise. The mirror keeps each changed slot's
//! box as of the last point for it (`Mirror::old_boxes`). Writes, range and
//! dirt reach the raster thread as one command (`RasterCmd::WriteDrawList`).

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Instant;

use crate::gpu::{
  check_direction, gather_list, BufferIds, CellBox, CellIndex, DrawList, DrawListFacts, DrawRange, DrawSpec,
  InstanceOrder, ListChange, ListDamage, ListOrder, OrderKey, OrderScratch, OrderView, RecordExtent, TargetDirt,
  CELL_BOX_FLOATS, MAX_BUFFERS,
};
use crate::raster::{ListWrite, RasterCmd};

use super::order::{entry_ids, entry_range, instance_strides, mark_changed, InstanceOrders, Mirror};
use super::Context;

/// The records a draw list's own buffers hold at first; they grow by
/// doubling past it. About a phone screen of small sprites.
const LIST_INITIAL_RECORDS: usize = 256;

/// The debug label of a draw list's own buffers (`/gpu`).
const LIST_BUFFER_LABEL: &str = "draw list";

/// One entry's draw list.
pub(crate) struct ListEntry {
  // The key the list sorts by (None = slot order) and its direction.
  key: Option<OrderKey>,
  descending: bool,
  // The pipeline buffer index holding the key.
  key_slot: usize,
  // The pipeline buffer index whose mirror holds the cull boxes: the
  // first instance-step buffer, whatever buffer holds the key.
  box_slot: usize,
  // The population's cells, when the list culls.
  cells: Option<CellIndex>,
  // The box buffer's record fields the cull boxes are read from; None
  // when a spatial sink stages them beside the records.
  extent: Option<RecordExtent>,
  // Per-index record strides in bytes of the entry's instance-step
  // buffers (0 = a vertex-step buffer or none).
  strides: [usize; MAX_BUFFERS],
  // The population's buffers per index, the entry's declared ones.
  sources: [u64; MAX_BUFFERS],
  // The list's own buffers per index, what the raster thread binds.
  private: [u64; MAX_BUFFERS],
  // The records the own buffers hold.
  capacity: usize,
  list: DrawList,
  // The view the list culls under and a position key orders by (None
  // until fed: no culling), and the pixels a screen reach is measured in
  // (None = the target's size).
  view: Option<OrderView>,
  viewport: Option<[f32; 2]>,
  // The entry's instance count: the population, slots [0, instances).
  instances: usize,
  // The slots its buffers changed since the last rebuild point, gathered
  // at the point (ascending, each once); reused.
  touched: Vec<u32>,
  // The population of the last build; None before the first, and after
  // the box buffer changed (the cells refill).
  built: Option<usize>,
  // A rebuild is due whatever the writes say: the view, the viewport or
  // the key changed.
  stale: bool,
  rebuilds: u64,
  patches: u64,
  build_ms: f32,
  // The range the raster thread holds for the entry, as this side last
  // sent it (`upload_list`, `Context::update_draw`); None until known. A
  // range update that would send it again is skipped (`hold_range`).
  held: Option<DrawRange>,
}

// What the rebuild point did to a list (`ListEntry::refresh`), with the
// dirt it leaves on the list's target.
pub(crate) enum Refresh {
  Unchanged,
  Rebuilt(TargetDirt),
  // Kept; the records in this span of list positions are re-gathered.
  Patched(Range<usize>, TargetDirt),
}

impl ListEntry {
  pub(super) fn strides(&self) -> [usize; MAX_BUFFERS] {
    self.strides
  }

  pub(super) fn sources(&self) -> [u64; MAX_BUFFERS] {
    self.sources
  }

  /// The records the raster thread draws: the visible ones, never past
  /// the list's own buffers.
  pub(super) fn drawn(&self) -> usize {
    self.list.len().min(self.capacity)
  }

  /// Point `entry` at the list's own buffers, drawing nothing until the
  /// list's first rebuild.
  pub(super) fn point(&self, entry: &mut DrawSpec) {
    for (i, &id) in self.private.iter().enumerate() {
      if id != 0 {
        entry.buffers[i] = id;
      }
    }
    entry.draw.instance_count = 0;
  }

  // `ids` with the list's own buffers in place of the population's.
  fn listed(&self, mut ids: BufferIds) -> BufferIds {
    for (i, &id) in self.private.iter().enumerate() {
      if id != 0 {
        ids.buffers[i] = id;
      }
    }
    ids
  }

  pub(crate) fn set_instances(&mut self, instances: usize) {
    self.instances = instances;
  }

  /// Take a view: a position key orders by its depth direction, and a
  /// culled list culls under it.
  pub(crate) fn set_view(&mut self, view: OrderView) -> Result<(), String> {
    if let Some(OrderKey::Projected { offset, .. }) = self.key {
      let Some(direction) = view.direction() else {
        return Err("The view has no depth direction (its w and z rows are zero or not finite)".to_string());
      };
      let key = Some(OrderKey::Projected { offset, direction });
      if self.key != key {
        self.key = key;
        self.stale = true;
      }
    }
    if self.cells.is_some() && self.view != Some(view) {
      self.stale = true;
    }
    self.view = Some(view);
    Ok(())
  }

  pub(super) fn set_viewport(&mut self, viewport: [f32; 2]) {
    if self.viewport != Some(viewport) {
      self.viewport = Some(viewport);
      self.stale |= self.cells.is_some();
    }
  }

  /// The target's size changed: a list measuring its reach in it culls
  /// again.
  pub(super) fn target_resized(&mut self) {
    if self.viewport.is_none() && self.cells.is_some() {
      self.stale = true;
    }
  }

  pub(super) fn set_direction(&mut self, direction: [f32; 3]) -> Result<(), String> {
    let Some(OrderKey::Projected { offset, .. }) = self.key else {
      return Err(
        "the entry's instance order has no position key; orderDirection applies to position keys".to_string(),
      );
    };
    check_direction(direction)?;
    self.key = Some(OrderKey::Projected { offset, direction });
    self.stale = true;
    Ok(())
  }

  fn facts(&self) -> DrawListFacts {
    DrawListFacts {
      population: self.built.unwrap_or(0),
      visible: self.drawn(),
      rebuilds: self.rebuilds,
      patches: self.patches,
      build_ms: self.build_ms,
    }
  }

  // The dirt this point leaves on the list's target (`ListDamage`), read
  // from the list as it stands before the point updates it: `mirrors`
  // hold its records and boxes, `target` is its target's size in pixels.
  // Full while a source has no mirror (a destroyed buffer the entry still
  // declares: its records read as zero, so every drawn record may change)
  // and while the list is capped below its visible set (a rebuild moves
  // untouched slots across the capacity cut).
  fn damage(
    &self,
    mirrors: &HashMap<u64, Mirror>,
    population: usize,
    viewport: [f32; 2],
    target: Option<[f32; 2]>,
  ) -> TargetDirt {
    let mirrored =
      self.strides.iter().zip(self.sources.iter()).all(|(&stride, id)| stride == 0 || mirrors.contains_key(id));
    if !mirrored || self.capped() {
      return TargetDirt::Full;
    }
    let box_mirror = mirrors.get(&self.sources[self.box_slot]);
    let old_box = |slot: u32| box_mirror.and_then(|m| m.old_box(slot));
    ListDamage {
      index: self.cells.as_ref(),
      view: self.view.as_ref(),
      stale: self.stale,
      built: self.built,
      population,
      touched: &self.touched,
      boxes: box_mirror.map_or(&[][..], |m| &m.boxes[..]),
      old_box: &old_box,
      viewport,
      target,
    }
    .dirt()
  }

  // Whether the list's own buffers hold fewer records than it lists.
  fn capped(&self) -> bool {
    self.drawn() < self.list.len()
  }

  // Whether the raster thread already holds `range` for the entry; when
  // not, it is recorded as held (the caller sends it).
  fn hold_range(&mut self, range: DrawRange) -> bool {
    if self.held == Some(range) {
      return true;
    }
    self.held = Some(range);
    false
  }

  // Bring the list up to date at the rebuild point: its cells follow the
  // boxes written since the last point and the slots the population
  // gained or lost, and the list rebuilds when it is stale, its
  // population changed, or a write changed its drawn set or a drawn
  // record's key (`DrawList::change`); a write that changed neither is
  // patched in place. Either way the result carries the target's dirt,
  // `target` being its size in pixels.
  fn refresh(
    &mut self,
    mirrors: &HashMap<u64, Mirror>,
    scratch: &mut OrderScratch,
    viewport: [f32; 2],
    target: Option<[f32; 2]>,
  ) -> Refresh {
    let population = self.instances;
    let key_mirror = mirrors.get(&self.sources[self.key_slot]);
    let box_mirror = mirrors.get(&self.sources[self.box_slot]);
    let boxes: &[f32] = box_mirror.map_or(&[], |m| &m.boxes[..(population * CELL_BOX_FLOATS).min(m.boxes.len())]);
    self.touched.clear();
    for (i, &stride) in self.strides.iter().enumerate() {
      if stride > 0 {
        if let Some(m) = mirrors.get(&self.sources[i]) {
          self.touched.extend_from_slice(m.changed.slots());
        }
      }
    }
    self.touched.sort_unstable();
    self.touched.dedup();
    let resized = match self.built {
      None => Some(0..population),
      Some(built) if built != population => Some(built.min(population)..built.max(population)),
      Some(_) => None,
    };
    if let Some(index) = &mut self.cells {
      if let Some(m) = box_mirror {
        index.update(boxes, m.changed.slots().iter().map(|&s| s as usize));
      }
      if let Some(r) = resized.clone() {
        index.update(boxes, r);
      }
    }
    let view = self.view.as_ref();
    let records = key_mirror.map_or(&[][..], |m| &m.data[..m.held]);
    let order =
      self.key.map(|key| ListOrder { key, descending: self.descending, stride: self.strides[self.key_slot], records });
    if !(self.stale || resized.is_some()) {
      // A slot past the population is never drawn.
      let d = &self.touched[..self.touched.partition_point(|&s| (s as usize) < population)];
      if d.is_empty() {
        return Refresh::Unchanged;
      }
      let change = match &self.cells {
        Some(index) => self.list.change_culled(d, index, boxes, view, viewport, order.as_ref()),
        None => self.list.change_all(d, order.as_ref()),
      };
      match change {
        ListChange::None => return Refresh::Unchanged,
        ListChange::Patch(at) => {
          self.patches += 1;
          return Refresh::Patched(at, self.damage(mirrors, population, viewport, target));
        }
        ListChange::Rebuild => {}
      }
    }
    let mut dirt = self.damage(mirrors, population, viewport, target);
    let started = Instant::now();
    match &self.cells {
      Some(index) => self.list.build(index, boxes, view, viewport, order.as_ref(), scratch),
      None => self.list.build_all(population, order.as_ref(), scratch),
    };
    // Built past its buffers: until they grow, untouched slots may sit on
    // either side of the cut.
    if self.capped() {
      dirt = TargetDirt::Full;
    }
    self.stale = false;
    self.built = Some(population);
    self.rebuilds += 1;
    self.build_ms = started.elapsed().as_secs_f32() * 1000.0;
    Refresh::Rebuilt(dirt)
  }
}

/// The seam the list tests drive (tests/target_damage.rs).
#[cfg(test)]
impl ListEntry {
  /// A culling list over `lattice` with no own buffers: `strides` and
  /// `sources` per buffer index (its cull boxes from the first
  /// instance-step one, read off its records with `extent` or staged by a
  /// sink), keyed by `key` in buffer `key_slot` when given, `instances`
  /// records, `capacity` of them drawn at most.
  #[allow(clippy::too_many_arguments)]
  pub(crate) fn for_test(
    key: Option<OrderKey>,
    key_slot: usize,
    strides: [usize; MAX_BUFFERS],
    sources: [u64; MAX_BUFFERS],
    lattice: crate::spatial::Lattice,
    extent: Option<RecordExtent>,
    instances: usize,
    capacity: usize,
  ) -> ListEntry {
    ListEntry {
      key,
      descending: false,
      key_slot,
      box_slot: strides.iter().position(|&s| s > 0).unwrap_or(key_slot),
      cells: Some(CellIndex::new(lattice)),
      extent,
      strides,
      sources,
      private: [0; MAX_BUFFERS],
      capacity,
      list: DrawList::default(),
      view: None,
      viewport: None,
      instances,
      touched: Vec::new(),
      built: None,
      stale: true,
      rebuilds: 0,
      patches: 0,
      build_ms: 0.0,
      held: None,
    }
  }

  /// The slots drawn, in draw order.
  pub(crate) fn listed_slots(&self) -> &[u32] {
    &self.list.list()[..self.drawn()]
  }
}

/// The draw lists' rebuild point without its sends (`flush_draw_lists`):
/// every mirror's boxes re-read from its records, every list in `lists`
/// refreshed over `mirrors` (`ListEntry::refresh`, `target_of` giving a
/// target's size in pixels), then every mirror's changed slots forgotten.
/// Returns what the point did to each list that changed.
pub(crate) fn rebuild_point(
  lists: &mut HashMap<(u64, u64), ListEntry>,
  mirrors: &mut HashMap<u64, Mirror>,
  scratch: &mut OrderScratch,
  target_of: impl Fn(u64) -> Option<[f32; 2]>,
) -> Vec<((u64, u64), Refresh)> {
  for mirror in mirrors.values_mut() {
    mirror.refresh_boxes();
  }
  let mut changed = Vec::new();
  for (&key, list) in lists.iter_mut() {
    let target = target_of(key.0);
    let viewport = list.viewport.or(target).unwrap_or([1.0, 1.0]);
    match list.refresh(mirrors, scratch, viewport, target) {
      Refresh::Unchanged => {}
      refresh => changed.push((key, refresh)),
    }
  }
  for mirror in mirrors.values_mut() {
    mirror.clear_changed();
  }
  changed
}

/// Which mirrors keep old boxes: those some culling list reads its cull
/// boxes from (`Mirror::set_captures`). Recomputed whenever a list comes,
/// goes or swaps its buffers.
fn refresh_box_captures(orders: &mut InstanceOrders) {
  let sources: HashSet<u64> =
    orders.lists.values().filter(|l| l.cells.is_some()).map(|l| l.sources[l.box_slot]).collect();
  for (id, mirror) in orders.mirrors.iter_mut() {
    mirror.set_captures(sources.contains(id));
  }
}

impl Mirror {
  // Write `new` at byte `offset`: the records become the app's (a
  // hand-off's ownership ends), and each record the write left different
  // is changed until the next rebuild point. A record written as it was
  // costs the comparison only, so the rebuild point follows what changed,
  // not the range the app marked.
  pub(crate) fn write_bytes(&mut self, offset: usize, new: &[u8]) {
    let end = offset + new.len();
    let held = self.held;
    // The fields apart, so a record can be marked (its old box kept) while
    // the records are held mutably: one copy-on-write per write.
    let Mirror { data, stride, changed, captures, old_boxes, boxes, .. } = self;
    let (stride, captures) = (*stride, *captures);
    let bytes = std::sync::Arc::make_mut(data);
    if bytes.len() < end {
      bytes.resize(end, 0);
    }
    if stride == 0 {
      bytes[offset..end].copy_from_slice(new);
    } else {
      let mut at = offset;
      while at < end {
        let slot = at / stride;
        let next = ((slot + 1) * stride).min(end);
        let src = &new[at - offset..next - offset];
        if at >= held || bytes[at..next] != *src {
          bytes[at..next].copy_from_slice(src);
          mark_changed(changed, old_boxes, boxes, captures, slot);
        }
        at = next;
      }
    }
    self.held = self.held.max(end);
    self.take_back();
  }

  // A spatial record sink's publish (`Context::list_write_instances`): the
  // floats [lo, hi) of `values` (the sink's staging, whole, in slot order)
  // as records, then with `boxes` the cull boxes of the slots they cover.
  pub(crate) fn write_instances(&mut self, lo: usize, hi: usize, values: &[f32], boxes: Option<&[f32]>) {
    let bytes: Vec<u8> = values[lo..hi].iter().flat_map(|v| v.to_ne_bytes()).collect();
    self.write_bytes(lo * 4, &bytes);
    if let Some(boxes) = boxes {
      self.write_boxes(lo, hi, boxes);
    }
  }

  // Take the cull boxes a sink staged for floats [lo, hi) of its records
  // (`boxes`, whole, in slot order), marking each slot whose box changed
  // before its box is overwritten (so the slot's first mark keeps the box
  // it had at the last point, `old_boxes`). A slot the column loses is
  // marked too: its box goes with it.
  pub(crate) fn write_boxes(&mut self, lo: usize, hi: usize, boxes: &[f32]) {
    let slots = if self.boxes.len() != boxes.len() {
      0..boxes.len() / CELL_BOX_FLOATS
    } else {
      let floats = (self.stride / 4).max(1);
      lo / floats..hi.div_ceil(floats).min(boxes.len() / CELL_BOX_FLOATS)
    };
    for slot in boxes.len() / CELL_BOX_FLOATS..self.boxes.len() / CELL_BOX_FLOATS {
      self.mark_slot(slot);
    }
    // A slot past the column had no box: it grows by the empty one.
    let empty = CellBox::EMPTY.floats();
    while self.boxes.len() < boxes.len() {
      self.boxes.extend_from_slice(&empty);
    }
    self.boxes.truncate(boxes.len());
    for slot in slots {
      let span = slot * CELL_BOX_FLOATS..(slot + 1) * CELL_BOX_FLOATS;
      if self.boxes[span.clone()] != boxes[span.clone()] {
        self.mark_slot(slot);
        self.boxes[span.clone()].copy_from_slice(&boxes[span]);
      }
    }
  }

  // Re-read the cull boxes of the changed slots from the records, on a
  // buffer whose boxes are its records' own fields (`extent`). Every slot
  // it overwrites is marked already, its old box kept at that mark.
  pub(crate) fn refresh_boxes(&mut self) {
    let Some(extent) = self.extent else {
      return;
    };
    if self.stride == 0 || self.changed.is_empty() {
      return;
    }
    let need = self.changed.slots().iter().max().map_or(0, |&s| s as usize + 1) * CELL_BOX_FLOATS;
    if self.boxes.len() < need {
      let floats = CellBox::EMPTY.floats();
      while self.boxes.len() < need {
        self.boxes.extend_from_slice(&floats);
      }
    }
    for &slot in self.changed.slots() {
      let slot = slot as usize;
      let at = slot * self.stride;
      let cell = if at + self.stride <= self.held { extent.cell_box(&self.data, at) } else { CellBox::EMPTY };
      self.boxes[slot * CELL_BOX_FLOATS..(slot + 1) * CELL_BOX_FLOATS].copy_from_slice(&cell.floats());
    }
  }
}

impl Context {
  // Open an entry's draw list, its own buffers created (`begin_entry_order`).
  pub(super) fn open_list(
    &self,
    order: &InstanceOrder,
    key: usize,
    strides: [crate::gpu::BufferStride; MAX_BUFFERS],
    ids: BufferIds,
    instances: usize,
  ) -> Result<ListEntry, String> {
    let strides = instance_strides(strides);
    let extent = order.cells.filter(|_| order.culls()).and_then(|c| c.extent);
    let box_slot = strides.iter().position(|&s| s > 0).unwrap_or(key);
    let private = self.create_list_buffers(&strides, LIST_INITIAL_RECORDS)?;
    let mut sources = [0u64; MAX_BUFFERS];
    for (i, &stride) in strides.iter().enumerate() {
      if stride > 0 {
        sources[i] = ids.buffers[i];
      }
    }
    Ok(ListEntry {
      key: order.key,
      descending: order.descending,
      key_slot: key,
      box_slot,
      cells: order.cells.filter(|_| order.culls()).map(|c| CellIndex::new(c.lattice).with_margin(c.margin)),
      extent,
      strides,
      sources,
      private,
      capacity: LIST_INITIAL_RECORDS,
      list: DrawList::default(),
      view: None,
      viewport: None,
      instances,
      touched: Vec::new(),
      built: None,
      stale: true,
      rebuilds: 0,
      patches: 0,
      build_ms: 0.0,
      held: None,
    })
  }

  // A list's own buffers, `records` records each, per instance-step
  // index; on a failure the ones made go again.
  fn create_list_buffers(&self, strides: &[usize; MAX_BUFFERS], records: usize) -> Result<[u64; MAX_BUFFERS], String> {
    let mut private = [0u64; MAX_BUFFERS];
    for (i, &stride) in strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      match self.create_gpu_buffer_zeroed(stride * records, Some(LIST_BUFFER_LABEL.to_string())) {
        Ok(id) => private[i] = id,
        Err(e) => {
          self.destroy_list_buffers(&private);
          return Err(e);
        }
      }
    }
    Ok(private)
  }

  fn destroy_list_buffers(&self, private: &[u64; MAX_BUFFERS]) {
    for &id in private.iter().filter(|&&id| id != 0) {
      self.destroy_gpu_buffer(id);
    }
  }

  /// Register entry (`target`, `draw`)'s draw list: each of its buffers
  /// writes into a mirror from now on.
  pub(super) fn commit_list(&self, target: u64, draw: u64, list: ListEntry) {
    for (i, &id) in list.sources.iter().enumerate() {
      if id != 0 {
        self.attach_mirror(id, list.strides[i], if i == list.box_slot { list.extent } else { None });
      }
    }
    let mut orders = self.orders.borrow_mut();
    for &id in list.sources.iter().filter(|&&id| id != 0) {
      orders.list_buffers.entry(id).or_default().push((target, draw));
    }
    orders.lists.insert((target, draw), list);
    orders.direct_stale = true;
    refresh_box_captures(&mut orders);
  }

  /// Drop a list that never registered (its create failed).
  pub(super) fn discard_list(&self, list: ListEntry) {
    self.destroy_list_buffers(&list.private);
  }

  // Make buffer `id` a list buffer with a mirror: the one it has (a
  // hand-off, another list's), else its contents read back once. A buffer
  // a spatial sink writes republishes its staging whole at the next
  // flush, so the mirror gets every slot's record and box, not only the
  // next change.
  fn attach_mirror(&self, id: u64, stride: usize, extent: Option<RecordExtent>) {
    let exists = self.orders.borrow().mirrors.contains_key(&id);
    if !exists {
      let data = self.gpu_buffer_len(id).and_then(|size| self.read_gpu_buffer(id, 0, size)).unwrap_or_else(|e| {
        log::warn!("[gpu] draw list over buffer {id} starts without its records: {e}");
        Vec::new()
      });
      self.orders.borrow_mut().mirrors.insert(id, Mirror::seeded(data, stride));
    }
    {
      let mut orders = self.orders.borrow_mut();
      let mirror = orders.mirrors.get_mut(&id).expect("ensured above");
      mirror.stride = stride;
      if extent.is_some() && mirror.extent != extent {
        mirror.extent = extent;
        let held = mirror.held;
        mirror.mark(0..held);
      }
    }
    if let Ok(mut spatial) = self.spatial.try_borrow_mut() {
      spatial.republish_instances(id);
    }
  }

  /// Drop entry (`target`, `draw`)'s draw list, if it has one: its own
  /// buffers go, and a buffer no list reads any more goes back to the GPU.
  pub(super) fn unregister_list(&self, target: u64, draw: u64) {
    let (list, released) = {
      let mut orders = self.orders.borrow_mut();
      let Some(list) = orders.lists.remove(&(target, draw)) else {
        return;
      };
      let mut released = Vec::new();
      for &id in list.sources.iter().filter(|&&id| id != 0) {
        if let Some(readers) = orders.list_buffers.get_mut(&id) {
          readers.retain(|&k| k != (target, draw));
          if readers.is_empty() {
            orders.list_buffers.remove(&id);
            released.push(id);
          }
        }
      }
      orders.direct_stale = true;
      (list, released)
    };
    self.discard_list(list);
    for id in released {
      self.release_mirror(id);
    }
    refresh_box_captures(&mut self.orders.borrow_mut());
  }

  // Hand a buffer no list reads back to the GPU: its records, whole,
  // unless an entry drew it directly all along. The mirror goes with it
  // unless the records were handed off.
  fn release_mirror(&self, id: u64) {
    let mut orders = self.orders.borrow_mut();
    let direct = orders.direct.remove(&id);
    let Some(mirror) = orders.mirrors.get_mut(&id) else {
      return;
    };
    if !direct && mirror.held > 0 {
      if let Ok(size) = self.gpu_buffer_len(id) {
        self.send(RasterCmd::WriteBuffer { id, data: mirror.data[..mirror.held.min(size)].to_vec(), byte_offset: 0 });
        self.note_buffer_content(id);
      }
    }
    if mirror.owned {
      mirror.clear_changed();
      mirror.boxes = Vec::new();
      mirror.extent = None;
    } else {
      orders.mirrors.remove(&id);
    }
  }

  /// A draw list's population moved to the buffers `ids` (the buffer-swap
  /// commit): the list reads the new buffers from its next rebuild, and an
  /// old one no list reads any more goes back to the GPU. Runs with no
  /// target borrow held.
  pub(super) fn swap_list_sources(&self, target: u64, draw: u64, ids: BufferIds) {
    let (changes, strides, box_slot, extent) = {
      let orders = self.orders.borrow();
      let Some(list) = orders.lists.get(&(target, draw)) else {
        return;
      };
      let changes: Vec<(usize, u64, u64)> = (0..MAX_BUFFERS)
        .filter(|&i| list.strides[i] > 0 && ids.buffers[i] != list.sources[i])
        .map(|i| (i, list.sources[i], ids.buffers[i]))
        .collect();
      (changes, list.strides, list.box_slot, list.extent)
    };
    for &(i, _, new) in &changes {
      self.attach_mirror(new, strides[i], if i == box_slot { extent } else { None });
    }
    let mut released = Vec::new();
    {
      let mut orders = self.orders.borrow_mut();
      let orders = &mut *orders;
      for &(i, old, new) in &changes {
        if let Some(readers) = orders.list_buffers.get_mut(&old) {
          readers.retain(|&k| k != (target, draw));
          if readers.is_empty() {
            orders.list_buffers.remove(&old);
            released.push(old);
          }
        }
        orders.list_buffers.entry(new).or_default().push((target, draw));
        if let Some(list) = orders.lists.get_mut(&(target, draw)) {
          list.sources[i] = new;
          list.stale = true;
          if i == box_slot {
            list.built = None;
          }
        }
      }
      orders.direct_stale = true;
    }
    for id in released {
      self.release_mirror(id);
    }
    refresh_box_captures(&mut self.orders.borrow_mut());
  }

  /// `ids` as the raster thread binds them for entry (`target`, `draw`):
  /// a draw list's own buffers in place of its population's.
  pub(super) fn listed_ids(&self, target: u64, draw: u64, ids: BufferIds) -> BufferIds {
    self.orders.borrow().lists.get(&(target, draw)).map_or(ids, |list| list.listed(ids))
  }

  /// Whether the raster thread already holds `range` for entry (`target`,
  /// `draw`)'s draw list, so a range update need not send it (one that
  /// changed only the population: the list's own command at the next
  /// rebuild point carries the range and the damage). False for an entry
  /// without a list, and for a range not held, which is recorded as held:
  /// the caller sends it.
  pub(super) fn list_holds_range(&self, target: u64, draw: u64, range: DrawRange) -> bool {
    self.orders.borrow_mut().lists.get_mut(&(target, draw)).is_some_and(|list| list.hold_range(range))
  }

  /// What entry (`target`, `draw`)'s draw list did, for the inventory.
  pub(super) fn list_facts(&self, target: u64, draw: u64) -> Option<DrawListFacts> {
    self.orders.borrow().lists.get(&(target, draw)).map(ListEntry::facts)
  }

  /// Route a write of `data` at byte `offset` of buffer `id` into its
  /// mirror when a draw list reads it: Some(whether an entry also draws the
  /// buffer directly, so the write goes to the GPU as well); None when no
  /// list reads the buffer.
  pub(super) fn list_write(&self, id: u64, data: &[u8], offset: usize) -> Option<bool> {
    let mut orders = self.orders.borrow_mut();
    if !orders.list_buffers.contains_key(&id) {
      return None;
    }
    let direct = orders.direct.contains(&id);
    orders.mirrors.entry(id).or_default().write_bytes(offset, data);
    Some(direct)
  }

  /// A spatial record sink's publish into a list buffer: the dirty floats
  /// [lo, hi) of `values` (the sink's staging, whole, in slot order) into
  /// the mirror, and with `boxes` the cull boxes of the slots they cover.
  /// Returns as `list_write`.
  pub(super) fn list_write_instances(
    &self,
    id: u64,
    lo: usize,
    hi: usize,
    values: &[f32],
    boxes: Option<&[f32]>,
  ) -> Option<bool> {
    let mut orders = self.orders.borrow_mut();
    if !orders.list_buffers.contains_key(&id) {
      return None;
    }
    let direct = orders.direct.contains(&id);
    orders.mirrors.entry(id).or_default().write_instances(lo, hi, values, boxes);
    Some(direct)
  }

  /// Read `len` bytes at `offset` of a buffer only a draw list draws: its
  /// GPU copy is not kept, so the mirror's records answer, in slot order;
  /// None for any other buffer.
  pub(super) fn list_read(&self, id: u64, offset: usize, len: usize) -> Option<Vec<u8>> {
    let orders = self.orders.borrow();
    if !orders.list_buffers.contains_key(&id) || orders.direct.contains(&id) {
      return None;
    }
    let mirror = orders.mirrors.get(&id)?;
    let mut out = vec![0u8; len];
    let end = (offset + len).min(mirror.held);
    if offset < end {
      out[..end - offset].copy_from_slice(&mirror.data[offset..end]);
    }
    Some(out)
  }

  /// The draw lists' rebuild point: every list whose view, key or
  /// population changed, or whose drawn set or keys a write since the last
  /// point changed, culls, sorts and gathers its visible records into its
  /// own buffers, and the raster thread draws them; a list a write left
  /// otherwise as it was re-gathers only the written records in place. Runs once per frame after the
  /// frame's writes and the spatial flush, before the frame takes its
  /// content changes (`apply_content_changes`), and before a manual target
  /// renders.
  pub fn flush_draw_lists(&self) {
    if self.orders.borrow().lists.is_empty() {
      return;
    }
    self.drain_recycled_blocks();
    self.refresh_direct();
    let started = Instant::now();
    let viewports: HashMap<u64, Option<[f32; 2]>> = {
      let orders = self.orders.borrow();
      orders.lists.keys().map(|&(t, _)| (t, self.target_viewport(t))).collect()
    };
    let (rebuilt, patched) = {
      let mut guard = self.orders.borrow_mut();
      let InstanceOrders { lists, mirrors, scratch, .. } = &mut *guard;
      let mut rebuilt = Vec::new();
      let mut patched = Vec::new();
      for (key, refresh) in rebuild_point(lists, mirrors, scratch, |t| viewports.get(&t).copied().flatten()) {
        match refresh {
          Refresh::Unchanged => {}
          Refresh::Rebuilt(dirt) => rebuilt.push((key, dirt)),
          Refresh::Patched(at, dirt) => patched.push((key, at, dirt)),
        }
      }
      (rebuilt, patched)
    };
    let (rebuilds, patches) = (rebuilt.len(), patched.len());
    for ((target, draw), dirt) in rebuilt {
      self.fit_list(target, draw);
      self.upload_list(target, draw, dirt);
    }
    for ((target, draw), span, dirt) in patched {
      self.patch_list(target, draw, span, dirt);
    }
    let mut orders = self.orders.borrow_mut();
    orders.counters.list_rebuilds += rebuilds as u64;
    orders.counters.list_patches += patches as u64;
    orders.counters.list_visible = orders.lists.values().map(ListEntry::drawn).sum();
    let worked = rebuilds + patches > 0;
    orders.counters.list_ms = if worked { started.elapsed().as_secs_f32() * 1000.0 } else { 0.0 };
  }

  // Grow a list's own buffers to hold its visible records, to the next
  // power of two: new buffers, the entry pointed at them, the old ones
  // destroyed. On a failure the list keeps drawing what its buffers hold.
  fn fit_list(&self, target: u64, draw: u64) {
    let (records, strides, old) = {
      let orders = self.orders.borrow();
      let Some(list) = orders.lists.get(&(target, draw)) else {
        return;
      };
      if list.list.len() <= list.capacity {
        return;
      }
      (list.list.len().next_power_of_two(), list.strides, list.private)
    };
    let private = match self.create_list_buffers(&strides, records) {
      Ok(private) => private,
      Err(e) => {
        log::warn!("[gpu] draw list buffers not grown to {records} records: {e}");
        return;
      }
    };
    let ids = {
      let mut orders = self.orders.borrow_mut();
      match orders.lists.get_mut(&(target, draw)) {
        Some(list) => {
          list.private = private;
          list.capacity = records;
          entry_ids(&self.targets.borrow(), target, draw).map(|ids| list.listed(ids))
        }
        None => None,
      }
    };
    let Some(ids) = ids else {
      self.destroy_list_buffers(&private);
      return;
    };
    self.send(RasterCmd::SetDrawBuffers { target, draw: (draw != 0).then_some(draw), ids });
    self.destroy_list_buffers(&old);
  }

  // Gather a rebuilt list's visible records from every buffer of the
  // population into the list's own, and draw them: the gathers, the range
  // and the target's `dirt` as one command.
  fn upload_list(&self, target: u64, draw: u64, dirt: TargetDirt) {
    let started = Instant::now();
    let mut writes = Vec::new();
    let range = {
      let mut guard = self.orders.borrow_mut();
      let InstanceOrders { lists, mirrors, .. } = &mut *guard;
      let Some(list) = lists.get_mut(&(target, draw)) else {
        return;
      };
      let drawn = list.drawn();
      let slots = &list.list.list()[..drawn];
      for (i, &stride) in list.strides.iter().enumerate() {
        if stride == 0 || drawn == 0 {
          continue;
        }
        let id = list.private[i];
        let src: &[u8] = mirrors.get(&list.sources[i]).map_or(&[], |m| &m.data[..m.held]);
        let len = drawn * stride;
        let mut block = self.write_leases.borrow_mut().take_free(id, list.capacity * stride);
        gather_list(slots, stride, src, &mut block[..len]);
        writes.push(ListWrite::Lease { id, block, len, recycle: self.block_recycle_tx.clone() });
      }
      list.build_ms += started.elapsed().as_secs_f32() * 1000.0;
      let range =
        entry_range(&self.targets.borrow(), target, draw).map(|r| DrawRange { instance_count: drawn as i32, ..r });
      if range.is_some() {
        list.held = range;
      }
      range
    };
    if writes.is_empty() && range.is_none() {
      return;
    }
    let noted = range.is_some().then(|| dirt.clone());
    self.send(RasterCmd::WriteDrawList { target, draw, writes, range, dirt });
    if let Some(dirt) = noted {
      self.note_target_content_dirt(target, &dirt);
    }
  }

  // Re-gather the records in the span of list positions `span` of a
  // patched list from every buffer of the population into the list's own,
  // in place: one write per buffer, sent with the target's `dirt` as one
  // command, and the target paints again.
  fn patch_list(&self, target: u64, draw: u64, span: Range<usize>, dirt: TargetDirt) {
    let mut writes = Vec::new();
    {
      let mut guard = self.orders.borrow_mut();
      let InstanceOrders { lists, mirrors, counters, .. } = &mut *guard;
      let Some(list) = lists.get(&(target, draw)) else {
        return;
      };
      let at = span.start..span.end.min(list.drawn());
      if at.is_empty() {
        return;
      }
      let slots = &list.list.list()[at.clone()];
      counters.list_patch_records += slots.len() as u64;
      for (i, &stride) in list.strides.iter().enumerate() {
        if stride == 0 {
          continue;
        }
        let src: &[u8] = mirrors.get(&list.sources[i]).map_or(&[], |m| &m.data[..m.held]);
        let mut data = vec![0u8; slots.len() * stride];
        gather_list(slots, stride, src, &mut data);
        writes.push(ListWrite::Bytes { id: list.private[i], data, byte_offset: at.start * stride });
      }
    }
    self.note_target_content_dirt(target, &dirt);
    self.send(RasterCmd::WriteDrawList { target, draw, writes, range: None, dirt });
  }

  // Recompute which list buffers an entry draws directly (an entry with
  // no list over them), after entries came, went or swapped buffers. A
  // buffer that became direct gets its records, whole, from its mirror:
  // its GPU copy missed every write since a list took it.
  fn refresh_direct(&self) {
    let mut guard = self.orders.borrow_mut();
    if !guard.direct_stale {
      return;
    }
    guard.direct_stale = false;
    let orders = &mut *guard;
    let mut direct = HashSet::new();
    {
      let targets = self.targets.borrow();
      let mut visit = |key: (u64, u64), ids: &BufferIds| {
        let list = orders.lists.get(&key);
        for (i, &b) in ids.buffers.iter().enumerate() {
          if b != 0 && orders.list_buffers.contains_key(&b) && !list.is_some_and(|l| l.sources[i] == b) {
            direct.insert(b);
          }
        }
        if let Some((b, _)) = ids.index {
          if orders.list_buffers.contains_key(&b) {
            direct.insert(b);
          }
        }
      };
      for (&t, mirror) in targets.iter() {
        match &mirror.entries {
          Some(list) => {
            for (&d, e) in &list.entries {
              visit((t, d), &e.buffers);
            }
          }
          None if mirror.draw.is_some() => visit((t, 0), &mirror.buffers),
          None => {}
        }
      }
    }
    for &id in direct.difference(&orders.direct) {
      let Some(mirror) = orders.mirrors.get(&id) else {
        continue;
      };
      if let Ok(size) = self.gpu_buffer_len(id) {
        self.send(RasterCmd::WriteBuffer { id, data: mirror.data[..mirror.held.min(size)].to_vec(), byte_offset: 0 });
        self.note_buffer_content(id);
      }
    }
    orders.direct = direct;
  }
}
