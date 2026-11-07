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

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Instant;

use crate::gpu::{
  check_direction, gather_list, BufferIds, CellBox, CellIndex, DrawList, DrawListFacts, DrawRange, DrawSpec,
  InstanceOrder, ListChange, ListOrder, OrderKey, OrderScratch, OrderView, RecordExtent, CELL_BOX_FLOATS, MAX_BUFFERS,
};
use crate::raster::RasterCmd;

use super::order::{entry_ids, entry_range, instance_strides, InstanceOrders, Mirror};
use super::Context;

/// The records a draw list's own buffers hold at first; they grow by
/// doubling past it. About a phone screen of small sprites.
const LIST_INITIAL_RECORDS: usize = 256;

/// The debug label of a draw list's own buffers (`/gpu`).
const LIST_BUFFER_LABEL: &str = "draw list";

/// One entry's draw list.
pub(super) struct ListEntry {
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
}

// What the rebuild point did to a list (`ListEntry::refresh`).
enum Refresh {
  Unchanged,
  Rebuilt,
  // Kept; the records in this span of list positions are re-gathered.
  Patched(Range<usize>),
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

  pub(super) fn set_instances(&mut self, instances: usize) {
    self.instances = instances;
  }

  /// Take a view: a position key orders by its depth direction, and a
  /// culled list culls under it.
  pub(super) fn set_view(&mut self, view: OrderView) -> Result<(), String> {
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

  // Bring the list up to date at the rebuild point: its cells follow the
  // boxes written since the last point and the slots the population
  // gained or lost, and the list rebuilds when it is stale, its
  // population changed, or a write changed its drawn set or a drawn
  // record's key (`DrawList::change`); a write that changed neither is
  // patched in place.
  fn refresh(&mut self, mirrors: &HashMap<u64, Mirror>, scratch: &mut OrderScratch, viewport: [f32; 2]) -> Refresh {
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
          return Refresh::Patched(at);
        }
        ListChange::Rebuild => {}
      }
    }
    let started = Instant::now();
    match &self.cells {
      Some(index) => self.list.build(index, boxes, view, viewport, order.as_ref(), scratch),
      None => self.list.build_all(population, order.as_ref(), scratch),
    };
    self.stale = false;
    self.built = Some(population);
    self.rebuilds += 1;
    self.build_ms = started.elapsed().as_secs_f32() * 1000.0;
    Refresh::Rebuilt
  }
}

impl Mirror {
  // Write `new` at byte `offset`: the records become the app's (a
  // hand-off's ownership ends), and each record the write left different
  // is changed until the next rebuild point. A record written as it was
  // costs the comparison only, so the rebuild point follows what changed,
  // not the range the app marked.
  fn write_bytes(&mut self, offset: usize, new: &[u8]) {
    let end = offset + new.len();
    let held = self.held;
    let bytes = std::sync::Arc::make_mut(&mut self.data);
    if bytes.len() < end {
      bytes.resize(end, 0);
    }
    if self.stride == 0 {
      bytes[offset..end].copy_from_slice(new);
    } else {
      let mut at = offset;
      while at < end {
        let slot = at / self.stride;
        let next = ((slot + 1) * self.stride).min(end);
        let src = &new[at - offset..next - offset];
        if at >= held || bytes[at..next] != *src {
          bytes[at..next].copy_from_slice(src);
          self.changed.mark(slot);
        }
        at = next;
      }
    }
    self.held = self.held.max(end);
    self.take_back();
  }

  // Take the cull boxes a sink staged for floats [lo, hi) of its records
  // (`boxes`, whole, in slot order), marking each slot whose box changed.
  fn write_boxes(&mut self, lo: usize, hi: usize, boxes: &[f32]) {
    let slots = if self.boxes.len() != boxes.len() {
      0..boxes.len() / CELL_BOX_FLOATS
    } else {
      let floats = (self.stride / 4).max(1);
      lo / floats..hi.div_ceil(floats).min(boxes.len() / CELL_BOX_FLOATS)
    };
    if self.boxes.len() < boxes.len() {
      self.boxes.resize(boxes.len(), 0.0);
    }
    self.boxes.truncate(boxes.len());
    for slot in slots {
      let span = slot * CELL_BOX_FLOATS..(slot + 1) * CELL_BOX_FLOATS;
      if self.boxes[span.clone()] != boxes[span.clone()] {
        self.boxes[span.clone()].copy_from_slice(&boxes[span]);
        self.changed.mark(slot);
      }
    }
  }

  // Re-read the cull boxes of the changed slots from the records, on a
  // buffer whose boxes are its records' own fields (`extent`).
  fn refresh_boxes(&mut self) {
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
      mirror.changed.clear();
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
  }

  /// `ids` as the raster thread binds them for entry (`target`, `draw`):
  /// a draw list's own buffers in place of its population's.
  pub(super) fn listed_ids(&self, target: u64, draw: u64, ids: BufferIds) -> BufferIds {
    self.orders.borrow().lists.get(&(target, draw)).map_or(ids, |list| list.listed(ids))
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
    let mirror = orders.mirrors.entry(id).or_default();
    let bytes: Vec<u8> = values[lo..hi].iter().flat_map(|v| v.to_ne_bytes()).collect();
    mirror.write_bytes(lo * 4, &bytes);
    if let Some(boxes) = boxes {
      mirror.write_boxes(lo, hi, boxes);
    }
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
      for mirror in mirrors.values_mut() {
        mirror.refresh_boxes();
      }
      let mut rebuilt = Vec::new();
      let mut patched = Vec::new();
      for (&key, list) in lists.iter_mut() {
        let viewport = list.viewport.or(viewports.get(&key.0).copied().flatten()).unwrap_or([1.0, 1.0]);
        match list.refresh(mirrors, scratch, viewport) {
          Refresh::Unchanged => {}
          Refresh::Rebuilt => rebuilt.push(key),
          Refresh::Patched(at) => patched.push((key, at)),
        }
      }
      for mirror in mirrors.values_mut() {
        mirror.changed.clear();
      }
      (rebuilt, patched)
    };
    for &(target, draw) in &rebuilt {
      self.fit_list(target, draw);
      self.upload_list(target, draw);
    }
    for ((target, draw), span) in &patched {
      self.patch_list(*target, *draw, span.clone());
    }
    let mut orders = self.orders.borrow_mut();
    orders.counters.list_rebuilds += rebuilt.len() as u64;
    orders.counters.list_patches += patched.len() as u64;
    orders.counters.list_visible = orders.lists.values().map(ListEntry::drawn).sum();
    let worked = !(rebuilt.is_empty() && patched.is_empty());
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
  // population into the list's own, and draw them.
  fn upload_list(&self, target: u64, draw: u64) {
    let started = Instant::now();
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
        self.send(RasterCmd::WriteBufferLease { id, block, len, recycle: self.block_recycle_tx.clone() });
      }
      list.build_ms += started.elapsed().as_secs_f32() * 1000.0;
      entry_range(&self.targets.borrow(), target, draw).map(|r| DrawRange { instance_count: drawn as i32, ..r })
    };
    if let Some(range) = range {
      self.send_entry_range(target, draw, range);
    }
  }

  // Re-gather the records in the span of list positions `span` of a
  // patched list from every buffer of the population into the list's own,
  // in place: one write per buffer, and the target paints again.
  fn patch_list(&self, target: u64, draw: u64, span: Range<usize>) {
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
        self.send(RasterCmd::WriteBuffer { id: list.private[i], data, byte_offset: at.start * stride });
      }
    }
    self.note_target_content(target);
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
