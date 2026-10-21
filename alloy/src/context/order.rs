use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::gpu::{
  gather_ordered, gather_permuted, materialize_indices, materialize_indices_through, order_packed, order_permutation,
  BufferIds, BufferStride, DrawRange, InstanceOrder, KeyColumn, OrderCounters, OrderScratch, OrderView, OrderWorker,
  PackedJob, RunTable, Selection, SortJob, StepMode, MAX_BUFFERS,
};
use crate::raster::RasterCmd;

use super::Context;

// Above this many keys an index-materialized re-sort goes to the sort
// thread (gpu/order_worker.rs): the ids land a sort later and the frame
// never waits. Below it the sort runs in the flush, exact for the frame:
// a few milliseconds on a phone core, less than a frame of lag is worth.
const SYNC_SORT_MAX_KEYS: usize = 16384;

// The instance-order registry (see gpu/order.rs for the primitive): which
// draw entries declared an instance order, and - derived - which buffers are
// ordered, for the publish hooks. Entirely UI-side state: the raster thread
// receives already-gathered blocks and never learns ordering exists.
pub(super) struct InstanceOrders {
  // (target, draw) -> the entry's declared order. Draw 0 is a fixed-kind
  // target's single entry (draw-target entry ids start at 1, so 0 is
  // unambiguous).
  entries: HashMap<(u64, u64), OrderedEntry>,
  // Instance buffer id -> the one entry ordering it (one ordered entry per
  // buffer; the publish hooks resolve through this). Every instance-step
  // buffer of an ordered entry appears here.
  by_buffer: HashMap<u64, (u64, u64)>,
  // Buffer id -> the slot-order copy of its records. BUFFER-lifetime, not
  // entry-lifetime: it survives remove_draw (freed with the buffer), so a
  // re-attached ordered entry re-sorts from it with no republish from the
  // app, and a handed-off population (instance_order_records) needs no
  // app-side copy at all.
  mirrors: HashMap<u64, Mirror>,
  // The sort's reusable working memory, shared by every ordered entry (the
  // UI thread publishes one buffer at a time).
  scratch: OrderScratch,
  // The sort thread, started at the first re-sort that leaves the UI
  // thread; None until then, and None after a spawn failure (the registry
  // then sorts in place).
  worker: Option<OrderWorker>,
  worker_failed: bool,
  // The frame-request latch a finished job sets before waking the loop,
  // installed by the embedder (`Context::set_frame_request_latch`).
  latch: Option<Arc<AtomicBool>>,
  counters: OrderCounters,
}

struct OrderedEntry {
  order: InstanceOrder,
  // The pipeline buffer index holding the key (the declared one, or the
  // first instance-step buffer), resolved at registration.
  key: usize,
  // Per-index record strides in bytes of the entry's INSTANCE-step buffers
  // (pipeline layout state, fixed for the entry's life - a buffer swap
  // never changes them; 0 = a vertex-step buffer or none).
  strides: [usize; MAX_BUFFERS],
  // The currently ordered buffer per index (0 after a destroy with no
  // swap, and at every vertex-step index).
  buffers: [u64; MAX_BUFFERS],
  // The retained permutation (perm[i] = the slot drawn i-th) from the key
  // buffer's last publish. Retained only where `retains()` says so: with
  // several instance-step buffers the siblings must gather under the key
  // buffer's permutation, and a `retain: true` entry keeps it to re-sort
  // on a direction change; a single-buffer gather entry recomputes at
  // each publish and retains nothing (stage 1's contract, unchanged).
  perm: Vec<u32>,
  // The asynchronous re-sort (index entries above SYNC_SORT_MAX_KEYS):
  // the submit counter, the generation in flight (one job at a time per
  // entry), whether a newer re-sort was asked for while it ran (the
  // result's landing resubmits: the newest direction wins, nothing
  // queues up), and the permutation vector the next job fills.
  generation: u64,
  in_flight: Option<u64>,
  wanted: bool,
  spare: Vec<u32>,
  // A packed population's selection state (the mirror's run table says
  // whether the entry is one): the view it is selected under (None until
  // fed: every run whole), the budget in records (the entry's instance
  // count times its ids per instance, capped at the population), the
  // record ids the last selection kept in the slot order `perm` indexes
  // (what a republish materializes through) and the vector the next job
  // fills, and the records the last landed selection drew - the draw
  // range the raster thread holds, which is the registry's to set here,
  // never above the budget the entry's range carries.
  view: Option<OrderView>,
  budget: usize,
  ids: Vec<u32>,
  spare_ids: Vec<u32>,
  drawn: Option<usize>,
}

#[derive(Default)]
struct Mirror {
  // Shared with a sort job in flight (an index mirror never changes after
  // its hand-off; a publish onto a gathered mirror makes its own copy).
  data: Arc<Vec<u8>>,
  // Record bytes held. A hand-off may hold more records than draw; a
  // publish holds exactly what it published.
  held: usize,
  // The published prefix in bytes - the sorted population, what a
  // re-sort re-gathers and republishes. Equal to `held` on the publish
  // paths; on an owned mirror the entry's instance count picks it.
  published: usize,
  // True when the records were handed off (instance_order_records): the
  // core owns the full record set and the entry's instance count decides
  // the published prefix. False when the app's publishes write it.
  owned: bool,
  // Some when the hand-off declared a key column: the mirror holds that
  // column only (`len` bytes per record, the key at offset 0), for an
  // index-materialized entry whose attach must declare the same column.
  // None holds whole records, what a gathered entry republishes.
  column: Option<KeyColumn>,
  // The run table of a packed column (shared with a sort job in flight);
  // None otherwise. With one the mirror holds the packed words in
  // storage order, whole (`published` = `held`), and the entry's
  // selection decides what sorts.
  runs: Option<Arc<RunTable>>,
  // Whether an id write ever reached the buffer (index form): a fresh
  // buffer is zeroed, which fetches record 0 everywhere, so the first
  // asynchronous seed writes the sentinel first.
  ids_written: bool,
  // Whether that seed is the only id write so far (see `seeded_only`).
  seeded: bool,
}

impl Mirror {
  // The sentinel seed went out and no real order has followed it yet:
  // the entry draws nothing until the sort lands, and a render measured
  // in between (the submission budget's probe, gl::slicing) stands for
  // nothing. Cleared by the first real publish, which tells the raster
  // thread to forget that measurement.
  fn seeded_only(&mut self) -> bool {
    std::mem::replace(&mut self.seeded, false)
  }
}

impl OrderedEntry {
  fn slots(&self) -> usize {
    self.strides.iter().filter(|&&s| s > 0).count()
  }

  fn slot_of(&self, buffer: u64) -> Option<usize> {
    self.buffers.iter().position(|&b| b != 0 && b == buffer)
  }

  // Whether the entry keeps the permutation between publishes (and so
  // re-sorts from the buffer mirrors): multi-buffer coherence needs it,
  // `retain: true` opts a single-buffer entry in (the write-once
  // strategy), and an index stream has nothing but the mirror to sort.
  fn retains(&self) -> bool {
    self.order.retain || self.slots() > 1 || self.order.indices.is_some()
  }

  // The published prefix in bytes for an instance count over an owned
  // mirror (see `published_prefix`).
  fn published_for(&self, instances: usize, mirror: &Mirror) -> usize {
    published_prefix(&self.order, self.strides[self.key], instances, mirror)
  }
}

impl Mirror {
  // The byte stride of the records the sort reads off this mirror: the
  // key column's record (a key, or a packed pair) under the index
  // materialization, the key buffer's instance record otherwise.
  fn record_len(&self, entry: &OrderedEntry) -> usize {
    match self.column {
      Some(column) => column.mirror_len(),
      None => entry.strides[entry.key],
    }
  }

  // Whether the mirror holds a packed population's words with its run
  // table, whose permutation maps through the entry's selected ids.
  fn packed(&self) -> bool {
    self.runs.is_some()
  }

  // The records held.
  fn records(&self, entry: &OrderedEntry) -> usize {
    let len = self.record_len(entry);
    if len == 0 {
      0
    } else {
      self.held / len
    }
  }
}

// The published prefix of a handed-off mirror for an instance count: the
// first count records - count x K under the index materialization, whose
// instance record is K u32 ids and whose mirror record is the key column -
// capped at what was handed off. A packed mirror publishes whole: its
// count is a budget (`record_budget`).
fn published_prefix(order: &InstanceOrder, instance_stride: usize, instances: usize, mirror: &Mirror) -> usize {
  if mirror.packed() {
    return mirror.held;
  }
  let per_instance = if order.indices.is_some() { instance_stride / 4 } else { 1 };
  let record = match mirror.column {
    Some(column) => column.mirror_len(),
    None => order.mirror_stride(instance_stride),
  };
  (instances * per_instance * record).min(mirror.held)
}

// A packed entry's budget in records for an instance count: count x K,
// capped at the population.
fn record_budget(instance_stride: usize, instances: usize, records: usize) -> usize {
  instances.saturating_mul(instance_stride / 4).min(records)
}

// The id materialization of an entry's permutation into `dst`: the slots
// themselves, or the record ids a packed entry's selected slots name.
fn materialize(entry: &OrderedEntry, mirror: &Mirror, dst: &mut [u8]) {
  if mirror.packed() {
    materialize_indices_through(&entry.perm, &entry.ids, dst);
  } else {
    materialize_indices(&entry.perm, dst);
  }
}

// The instance-step strides of a pipeline's buffers by index (0 at a
// vertex-step index or past the declared layouts): the order registry's
// view of a layout.
fn instance_strides(strides: [BufferStride; MAX_BUFFERS]) -> [usize; MAX_BUFFERS] {
  let mut out = [0usize; MAX_BUFFERS];
  for (i, s) in strides.iter().enumerate() {
    if s.step == StepMode::Instance {
      out[i] = s.stride;
    }
  }
  out
}

impl InstanceOrders {
  pub(super) fn new() -> Self {
    Self {
      entries: HashMap::new(),
      by_buffer: HashMap::new(),
      mirrors: HashMap::new(),
      scratch: OrderScratch::default(),
      worker: None,
      worker_failed: false,
      latch: None,
      counters: OrderCounters::default(),
    }
  }

  // Start the sort thread on first use; a spawn failure is logged once and
  // leaves `worker` None (the registry then sorts in place).
  fn ensure_worker(&mut self) {
    if self.worker.is_some() || self.worker_failed {
      return;
    }
    match OrderWorker::spawn() {
      Ok(worker) => self.worker = Some(worker),
      Err(e) => {
        log::warn!("[order] sort thread unavailable, sorting in the flush: {e}");
        self.worker_failed = true;
      }
    }
  }
}

impl Context {
  /// Validate an entry's declared instance order against its pipeline layout
  /// and buffers - the setup half, pure so creates can check before their
  /// RPC and commit with `insert_instance_order` after it succeeds. The
  /// contract: the key reads from an instance-step buffer's records, every
  /// instance-step buffer is ordered by exactly one entry, and each is
  /// distinct from every other buffer the entry binds (a buffer read at
  /// two places, or as the index buffer, cannot hold instance records
  /// only). A handed-off key buffer must hold whole records of the key's
  /// stride. An index-materialized order takes exactly one instance-step
  /// buffer whose record is whole u32 ids, over a handed-off key column
  /// declared as the order's own (stride and key offset); a gathered
  /// order over a hand-off needs whole records. Returns the resolved key
  /// buffer index.
  pub(super) fn check_instance_order(
    &self,
    order: &InstanceOrder,
    strides: [BufferStride; MAX_BUFFERS],
    ids: BufferIds,
  ) -> Result<usize, String> {
    let instance_strides = instance_strides(strides);
    let key = match order.key_buffer {
      Some(index) => index,
      None => match instance_strides.iter().position(|&s| s > 0) {
        Some(index) => index,
        None => return Err("instanceOrder needs an instance-step buffer (the pipeline declares none)".to_string()),
      },
    };
    if instance_strides[key] == 0 {
      return Err(format!("instanceOrder keys on buffer {key}, but the pipeline's buffer {key} is not instance-step"));
    }
    if let Some(column) = order.key_column() {
      let instance_stride = instance_strides[key];
      if instance_stride % 4 != 0 {
        return Err(format!(
          "an indexed instanceOrder's instance record is whole u32 ids; buffer {key}'s stride is {instance_stride} bytes"
        ));
      }
      let slots = instance_strides.iter().filter(|&&s| s > 0).count();
      if slots != 1 {
        return Err(format!(
          "an indexed instanceOrder takes one instance-step buffer (the id stream); the entry declares {slots}"
        ));
      }
      let orders = self.orders.borrow();
      if let Some(mirror) = orders.mirrors.get(&ids.buffers[key]) {
        if !mirror.owned {
          return Err(format!(
            "buffer {} holds app-published records; an indexed instanceOrder sorts handed-off records (transferRecords)",
            ids.buffers[key]
          ));
        }
        match mirror.column {
          None => {
            return Err(format!(
              "buffer {} holds whole handed-off records; an indexed instanceOrder sorts a key column, which transferRecords takes as its key argument ({{ stride, position | field }})",
              ids.buffers[key]
            ))
          }
          Some(held) if !held.accepts(column) => {
            return Err(format!(
              "buffer {} handed off {}-byte records with a {}-byte{} key at float {}; the entry's instanceOrder reads a {}-byte key at float {} of {}-byte records",
              ids.buffers[key],
              held.stride,
              held.len,
              if held.packed.is_some() { " packed" } else { "" },
              held.offset / 4,
              column.len,
              column.offset / 4,
              column.stride
            ))
          }
          Some(_) => {}
        }
        // The id stream must hold every handed-off record's id, groups
        // rounded up.
        let records = mirror.held / mirror.column.map_or(column.len, |c| c.mirror_len());
        let per_instance = instance_stride / 4;
        let needed = records.div_ceil(per_instance) * instance_stride;
        let size = self.gpu_buffer_len(ids.buffers[key])?;
        if needed > size {
          return Err(format!(
            "buffer {} is {size} bytes; the ids of {records} handed-off records ({per_instance} per instance record of {instance_stride} bytes) need {needed}",
            ids.buffers[key]
          ));
        }
      }
    } else {
      order.check_stride(instance_strides[key])?;
    }
    let orders = self.orders.borrow();
    for (index, &stride) in instance_strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      let buffer = ids.buffers[index];
      if ids.index.is_some_and(|(i, _)| i == buffer) {
        return Err(format!(
          "buffer {buffer} is also the entry's index buffer; an ordered buffer holds instance records only"
        ));
      }
      if ids.buffers.iter().enumerate().any(|(j, &b)| j != index && b == buffer) {
        return Err(format!(
          "buffer {buffer} is bound twice on the entry; an ordered buffer holds instance records only"
        ));
      }
      if let Some((t, d)) = orders.by_buffer.get(&buffer) {
        return Err(format!(
          "buffer {buffer} is already ordered by draw {d} of target {t}; one ordered entry per buffer"
        ));
      }
      if let Some(mirror) = orders.mirrors.get(&buffer) {
        if mirror.owned && order.indices.is_none() {
          if mirror.column.is_some() {
            return Err(format!(
              "buffer {buffer} holds a handed-off key column; a gathered instanceOrder republishes whole records, so hand them off without a key"
            ));
          }
          if mirror.held % stride != 0 {
            return Err(format!(
              "buffer {buffer} holds {} handed-off bytes, not a whole number of {stride}-byte instance records",
              mirror.held
            ));
          }
          let size = self.gpu_buffer_len(buffer)?;
          if mirror.held > size {
            return Err(format!(
              "buffer {buffer} is {size} bytes; its {} handed-off record bytes do not fit",
              mirror.held
            ));
          }
        }
      }
    }
    Ok(key)
  }

  /// Commit a checked instance order for entry (`target`, `draw`).
  /// `instances` is the entry's resolved instance count: on a handed-off
  /// key buffer it picks the published prefix (the first `instances`
  /// records of the mirror). Returns whether the entry should SEED - sort
  /// and publish from a handed-off mirror, with no publish from the app -
  /// which the caller runs as `rematerialize_retained_order` once its
  /// target borrow dropped.
  pub(super) fn insert_instance_order(
    &self,
    target: u64,
    draw: u64,
    order: InstanceOrder,
    key: usize,
    strides: [BufferStride; MAX_BUFFERS],
    ids: BufferIds,
    instances: usize,
  ) -> bool {
    let strides = instance_strides(strides);
    let mut buffers = [0u64; MAX_BUFFERS];
    let mut orders = self.orders.borrow_mut();
    for (index, &stride) in strides.iter().enumerate() {
      if stride > 0 {
        buffers[index] = ids.buffers[index];
        orders.by_buffer.insert(buffers[index], (target, draw));
      }
    }
    let mut seed = false;
    let mut budget = 0;
    if let Some(mirror) = orders.mirrors.get_mut(&buffers[key]) {
      if mirror.owned {
        mirror.published = published_prefix(&order, strides[key], instances, mirror);
        if mirror.packed() {
          budget = record_budget(strides[key], instances, mirror.held / mirror.column.map_or(1, |c| c.mirror_len()));
          seed = budget > 0;
        } else {
          seed = mirror.published > 0;
        }
      }
    }
    orders.entries.insert(
      (target, draw),
      OrderedEntry {
        order,
        key,
        strides,
        buffers,
        perm: Vec::new(),
        generation: 0,
        in_flight: None,
        wanted: false,
        spare: Vec::new(),
        view: None,
        budget,
        ids: Vec::new(),
        spare_ids: Vec::new(),
        drawn: None,
      },
    );
    seed
  }

  /// Drop one entry's order (remove_draw). The buffer mirrors STAY - they
  /// are buffer-lifetime, so a re-created entry over the same buffers
  /// re-sorts from them (see `insert_instance_order`'s seed).
  pub(super) fn unregister_instance_order(&self, target: u64, draw: u64) {
    let mut orders = self.orders.borrow_mut();
    if let Some(entry) = orders.entries.remove(&(target, draw)) {
      for &buffer in entry.buffers.iter().filter(|&&b| b != 0) {
        orders.by_buffer.remove(&buffer);
      }
    }
  }

  /// Drop every order of a reclaimed target.
  pub(super) fn unregister_target_orders(&self, target: u64) {
    let removed: Vec<(u64, u64)> = self.orders.borrow().entries.keys().filter(|(t, _)| *t == target).copied().collect();
    for (t, d) in removed {
      self.unregister_instance_order(t, d);
    }
  }

  /// Hand a buffer's full record set to the core (the app-side `transfer`
  /// form): the mirror takes ownership of a copy, so the app keeps none,
  /// partial rewrites are off the table, and the ordered entry - attached
  /// now or later - sorts and republishes from here. With a `key` the
  /// mirror keeps the records' key column only (the index materialization:
  /// the sort reads nothing else and a re-sort writes ids), so a 1M-record
  /// hand-off is resident as 12 MB of positions, not the record block;
  /// without one it keeps the whole records a gathered order republishes.
  /// Hand off before the entry attaches; the attach seeds the first publish
  /// from this mirror (nothing draws until then) and checks the hand-off
  /// against the order's form (gathered records must fit the buffer; an
  /// index stream must hold their ids and its order must declare the
  /// handed-off key column). A second hand-off replaces the records
  /// wholesale.
  pub fn instance_order_records(
    &self,
    id: u64,
    data: &[u8],
    key: Option<KeyColumn>,
    runs: Option<RunTable>,
  ) -> Result<(), String> {
    self.gpu_buffer_len(id)?;
    if data.is_empty() {
      return Err(format!("record hand-off to buffer {id} is empty"));
    }
    if let Some(column) = key {
      column.check_records(data.len()).map_err(|e| format!("record hand-off to buffer {id}: {e}"))?;
    }
    let mut held = Vec::new();
    let mut table = None;
    match (key, runs) {
      (Some(column), Some(runs)) => {
        column.check_packed(data.len(), &runs).map_err(|e| format!("record hand-off to buffer {id}: {e}"))?;
        column.extract(data, &mut held);
        table = Some(Arc::new(runs));
      }
      (Some(column), None) => {
        if column.packed.is_some() {
          return Err(format!("record hand-off to buffer {id}: a packed key needs its run table"));
        }
        column.extract(data, &mut held);
      }
      (None, Some(_)) => {
        return Err(format!("record hand-off to buffer {id}: a run table goes with a packed record key"));
      }
      (None, None) => held.extend_from_slice(data),
    }
    let mut orders = self.orders.borrow_mut();
    let mirror = orders.mirrors.entry(id).or_default();
    mirror.held = held.len();
    mirror.data = Arc::new(held);
    mirror.published = if table.is_some() { mirror.held } else { mirror.published.min(mirror.held) };
    mirror.owned = true;
    mirror.column = key;
    mirror.runs = table;
    Ok(())
  }

  /// Install the frame-request latch a finished sort job sets before it
  /// wakes the loop (`PlatformContext::frame_request_handle`), so the ids
  /// a job made land in a frame even when the app is otherwise idle.
  pub fn set_frame_request_latch(&self, latch: Arc<AtomicBool>) {
    self.orders.borrow_mut().latch = Some(latch);
  }

  /// The sort worker's counters: jobs pending, of them the jobs still
  /// sorting (what the settle gate waits out on the wall; the rest land in
  /// the next frame), and the drained jobs' timing.
  pub fn order_counters(&self) -> OrderCounters {
    let orders = self.orders.borrow();
    OrderCounters { running: orders.worker.as_ref().map_or(0, |worker| worker.running()), ..orders.counters }
  }

  /// Land the sort thread's finished jobs: a changed permutation becomes
  /// the entry's and its ids block goes to the raster thread as the key
  /// buffer's write; an unchanged one gives the block back to the pool.
  /// An entry that asked for another re-sort while the job ran resubmits
  /// here, on the newest direction. Runs once per frame before the spatial
  /// tick (so a job that finished between frames draws in this one) and at
  /// every re-sort entry point.
  pub fn drain_order_results(&self) {
    let results = {
      let orders = self.orders.borrow();
      match &orders.worker {
        Some(worker) => worker.drain(),
        None => return,
      }
    };
    if results.is_empty() {
      return;
    }
    let mut resubmit = Vec::new();
    let mut drawn = Vec::new();
    {
      let mut orders = self.orders.borrow_mut();
      let orders = &mut *orders;
      for done in results {
        orders.counters.pending = orders.counters.pending.saturating_sub(1);
        orders.counters.sorts += 1;
        orders.counters.last_ms = done.work_ms;
        orders.counters.max_ms = orders.counters.max_ms.max(done.work_ms);
        let Some(entry) = orders.entries.get_mut(&done.entry) else {
          self.recycle_block(done.buffer, done.block);
          continue;
        };
        if entry.in_flight != Some(done.generation) {
          self.recycle_block(done.buffer, done.block);
          continue;
        }
        entry.in_flight = None;
        let buffer = entry.buffers[entry.key];
        if buffer != done.buffer || buffer == 0 {
          // The entry moved to another buffer while the job ran: the ids
          // belong to the old one, so sort again for the new.
          self.recycle_block(done.buffer, done.block);
          entry.wanted = buffer != 0;
        } else if done.perm != entry.perm || done.ids != entry.ids {
          entry.spare = std::mem::replace(&mut entry.perm, done.perm);
          entry.spare_ids = std::mem::replace(&mut entry.ids, done.ids);
          let len = done.block.len();
          self.send(RasterCmd::WriteBufferLease {
            id: buffer,
            block: done.block,
            len,
            recycle: self.block_recycle_tx.clone(),
          });
          self.note_buffer_content(buffer);
          if orders.mirrors.get_mut(&buffer).is_some_and(Mirror::seeded_only) {
            self.send(RasterCmd::ResetDrawGauge { target: done.entry.0, draw: done.entry.1 });
          }
          if orders.mirrors.get(&buffer).is_some_and(Mirror::packed) {
            drawn.push((done.entry, entry.perm.len()));
          }
        } else {
          entry.spare = done.perm;
          entry.spare_ids = done.ids;
          self.recycle_block(done.buffer, done.block);
        }
        if entry.wanted {
          entry.wanted = false;
          resubmit.push(done.entry);
        }
      }
    }
    for ((target, draw), records) in drawn {
      self.apply_drawn(target, draw, records);
    }
    for (target, draw) in resubmit {
      self.rematerialize_retained_order(target, draw);
    }
  }

  // The selection a packed entry sorts under now: its view (None until
  // fed), the target's size in pixels as the contribution's unit (a
  // target without a known size selects as without a view), the order's
  // LOD threshold and the entry's budget.
  fn selection_of(&self, target: u64, entry: &OrderedEntry) -> Selection {
    let viewport = self.target_viewport(target);
    Selection {
      view: if viewport.is_some() { entry.view } else { None },
      viewport: viewport.unwrap_or([1.0, 1.0]),
      threshold: entry.order.lod_threshold,
      budget: entry.budget,
    }
  }

  // Set the draw range the raster thread holds for a packed entry to
  // what its last selection drew: ceil(records / K) instances, K the ids
  // per instance record. The entry's own range keeps the app's count (the
  // budget); every send of that range applies this override
  // (`drawn_range`). Runs with no target borrow held.
  fn apply_drawn(&self, target: u64, draw: u64, records: usize) {
    let range = {
      let mut orders = self.orders.borrow_mut();
      let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
        return;
      };
      let per_instance = (entry.strides[entry.key] / 4).max(1);
      entry.drawn = Some(records);
      let instances = records.div_ceil(per_instance) as i32;
      let targets = self.targets.borrow();
      let Some(mirror) = targets.get(&target) else {
        return;
      };
      let current = match (draw, mirror.entries.as_ref()) {
        (0, None) => mirror.draw,
        (d, Some(list)) => list.entries.get(&d).map(|e| e.draw),
        _ => None,
      };
      let Some(current) = current else {
        return;
      };
      DrawRange { instance_count: instances.min(current.instance_count.max(0)), ..current }
    };
    match draw {
      0 => self.send(RasterCmd::SetDraw { id: target, range }),
      d => self.send(RasterCmd::SetDrawRange { target, draw: d, range }),
    }
    self.note_target_content(target);
  }

  /// `range` as the raster thread should hold it for entry (`target`,
  /// `draw`): a packed entry draws its last selection, never more than the
  /// range's own instance count (the budget); any other entry draws the
  /// range as given.
  pub(super) fn drawn_range(&self, target: u64, draw: u64, range: DrawRange) -> DrawRange {
    let orders = self.orders.borrow();
    let Some(entry) = orders.entries.get(&(target, draw)) else {
      return range;
    };
    let Some(drawn) = entry.drawn else {
      return range;
    };
    let per_instance = (entry.strides[entry.key] / 4).max(1);
    let instances = drawn.div_ceil(per_instance) as i32;
    DrawRange { instance_count: instances.min(range.instance_count.max(0)), ..range }
  }

  // A target's size in pixels, the unit a selection's contribution is
  // measured in: a sub-target's rectangle, a texture target's own size.
  fn target_viewport(&self, target: u64) -> Option<[f32; 2]> {
    if let Some(tile) = self.sub_targets.borrow().get(&target) {
      return Some([tile.width as f32, tile.height as f32]);
    }
    self.textures.get(target).map(|e| [e.width as f32, e.height as f32])
  }

  /// Re-select every packed entry of `target` (its size changed: the
  /// contribution's unit moved). Runs with no target borrow held.
  pub(super) fn reselect_target_orders(&self, target: u64) {
    let entries: Vec<(u64, u64)> = {
      let orders = self.orders.borrow();
      orders
        .entries
        .iter()
        .filter(|((t, _), e)| *t == target && orders.mirrors.get(&e.buffers[e.key]).is_some_and(Mirror::packed))
        .map(|(k, _)| *k)
        .collect()
    };
    for (t, d) in entries {
      self.rematerialize_retained_order(t, d);
    }
  }

  // A block back to its buffer's pool (dropped when the buffer is gone).
  fn recycle_block(&self, buffer: u64, block: Vec<u8>) {
    let sizes = self.buffer_sizes.borrow();
    self.write_leases.borrow_mut().recycle(buffer, block, |i| sizes.contains_key(&i));
  }

  // Hand an index entry's re-sort to the sort thread: one job in flight
  // per entry, a second ask marks the entry wanted. A fresh id buffer gets
  // the sentinel written in place first, so nothing draws record 0 while
  // the seed sorts. False when no thread is available (the caller sorts in
  // the flush).
  fn submit_order_sort(&self, orders: &mut InstanceOrders, target: u64, draw: u64) -> bool {
    let latch = orders.latch.clone();
    let wake = self.frame_wake.clone();
    orders.ensure_worker();
    let Some(worker) = orders.worker.as_ref() else {
      return false;
    };
    let entry = orders.entries.get_mut(&(target, draw)).expect("the caller resolved the entry");
    if entry.in_flight.is_some() {
      entry.wanted = true;
      return true;
    }
    let buffer = entry.buffers[entry.key];
    let Ok(size) = self.gpu_buffer_len(buffer) else {
      return false;
    };
    let mirror = orders.mirrors.get_mut(&buffer).expect("the caller resolved the mirror");
    if !mirror.ids_written {
      mirror.ids_written = true;
      mirror.seeded = true;
      let mut dst = self.write_leases.borrow_mut().take_free(buffer, size);
      materialize(entry, mirror, &mut dst[..size]);
      self.send(RasterCmd::WriteBufferLease {
        id: buffer,
        block: dst,
        len: size,
        recycle: self.block_recycle_tx.clone(),
      });
      self.note_buffer_content(buffer);
    }
    entry.generation += 1;
    let generation = entry.generation;
    let done: Box<dyn FnOnce() + Send> = Box::new(move || {
      if let Some(latch) = &latch {
        latch.store(true, Ordering::Relaxed);
      }
      if let Some(wake) = &wake {
        wake();
      }
    });
    let selection = self.selection_of(target, entry);
    let job = SortJob {
      entry: (target, draw),
      buffer,
      generation,
      order: entry.order.mirror_order(),
      stride: mirror.record_len(entry),
      packed: match (mirror.column.and_then(|c| c.packed), &mirror.runs) {
        (Some(key), Some(table)) => Some(PackedJob { key, table: table.clone(), selection }),
        _ => None,
      },
      keys: mirror.data.clone(),
      len: mirror.published,
      perm: std::mem::take(&mut entry.spare),
      ids: std::mem::take(&mut entry.spare_ids),
      block: self.write_leases.borrow_mut().take_free(buffer, size),
      done: Some(done),
    };
    if !worker.submit(job) {
      return false;
    }
    entry.in_flight = Some(generation);
    orders.counters.pending += 1;
    true
  }

  /// Follow an ordered entry's instance count into the published prefix of
  /// its handed-off records: the first `instances` records of the mirror
  /// become the sorted population. Returns whether the prefix changed -
  /// the caller follows up with `rematerialize_retained_order` once its
  /// target borrow dropped (same two-step as a direction change). A no-op
  /// on entries without an order, without a handed-off key mirror (the
  /// app's own republish carries a count change there), or at the same
  /// prefix.
  pub(super) fn set_instance_order_count(&self, target: u64, draw: u64, instances: usize) -> bool {
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let Some(entry) = orders.entries.get(&(target, draw)) else {
      return false;
    };
    if !entry.retains() {
      return false;
    }
    let key_buffer = entry.buffers[entry.key];
    if key_buffer == 0 {
      return false;
    }
    let Some(mirror) = orders.mirrors.get_mut(&key_buffer) else {
      return false;
    };
    if !mirror.owned {
      return false;
    }
    if mirror.packed() {
      let budget = record_budget(entry.strides[entry.key], instances, mirror.records(entry));
      let entry = orders.entries.get_mut(&(target, draw)).expect("resolved above");
      if budget == entry.budget {
        return false;
      }
      entry.budget = budget;
      return true;
    }
    let published = entry.published_for(instances, mirror);
    if published == mirror.published {
      return false;
    }
    mirror.published = published;
    true
  }

  /// Set an ordered entry's view (the `orderView` update, the feed's
  /// form): the projected key's direction follows the matrix, and a
  /// packed entry keeps the view to select its runs under. Returns
  /// whether the order's inputs changed - a direction the matrix did not
  /// turn, on an entry with no runs to select, is nothing to re-sort for.
  /// Errs on an entry with no instance order, one whose key is a field,
  /// or a matrix without a depth direction.
  pub(super) fn set_instance_order_view(&self, target: u64, draw: u64, view: OrderView) -> Result<bool, String> {
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return Err("the entry has no instance order (declare instanceOrder at creation)".to_string());
    };
    let Some(direction) = view.direction() else {
      return Err("orderView has no depth direction (its w and z rows are zero or not finite)".to_string());
    };
    let before = entry.order.key;
    entry.order.set_direction(direction)?;
    let packed = orders.mirrors.get(&entry.buffers[entry.key]).is_some_and(Mirror::packed);
    entry.view = Some(view);
    Ok(packed || entry.order.key != before)
  }

  /// Replace an ordered entry's LOD threshold (the `orderLod` update).
  /// Returns whether a selection depends on it (a packed entry).
  pub(super) fn set_instance_order_lod(&self, target: u64, draw: u64, threshold: f64) -> Result<bool, String> {
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return Err("the entry has no instance order (declare instanceOrder at creation)".to_string());
    };
    entry.order.set_lod(threshold)?;
    Ok(orders.mirrors.get(&entry.buffers[entry.key]).is_some_and(Mirror::packed))
  }

  /// The buffer-swap half of the update transaction, split check/commit so
  /// a rejected swap changes nothing. A swap on an unordered entry passes
  /// untouched; on an ordered one the order follows the entry to the new
  /// buffers, every swapped instance-step buffer at once.
  pub(super) fn check_order_swap(&self, target: u64, draw: u64, ids: BufferIds) -> Result<(), String> {
    let orders = self.orders.borrow();
    let Some(entry) = orders.entries.get(&(target, draw)) else {
      return Ok(());
    };
    for (index, &stride) in entry.strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      let new_buffer = ids.buffers[index];
      if ids.buffers.iter().enumerate().any(|(j, &b)| j != index && b == new_buffer) {
        return Err(format!(
          "buffer {new_buffer} is bound twice on the entry; an ordered buffer holds instance records only"
        ));
      }
      if ids.index.is_some_and(|(i, _)| i == new_buffer) {
        return Err(format!(
          "buffer {new_buffer} is also the entry's index buffer; an ordered buffer holds instance records only"
        ));
      }
      if new_buffer == entry.buffers[index] {
        continue;
      }
      if let Some((t, d)) = orders.by_buffer.get(&new_buffer) {
        return Err(format!(
          "buffer {new_buffer} is already ordered by draw {d} of target {t}; one ordered entry per buffer"
        ));
      }
    }
    Ok(())
  }

  pub(super) fn commit_order_swap(&self, target: u64, draw: u64, ids: BufferIds) {
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return;
    };
    for (index, &stride) in entry.strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      let new_buffer = ids.buffers[index];
      let old = entry.buffers[index];
      if new_buffer == old {
        continue;
      }
      if old != 0 {
        orders.by_buffer.remove(&old);
      }
      entry.buffers[index] = new_buffer;
      orders.by_buffer.insert(new_buffer, (target, draw));
    }
  }

  /// Replace an ordered entry's projected-key direction (the `orderDirection`
  /// update). On a gather entry it takes effect at the entry's next publish;
  /// on a retained one `update_draw` follows up with
  /// `rematerialize_retained_order` once its target borrow drops. Errs on an
  /// entry with no instance order, or one whose key is a field.
  pub(super) fn set_instance_order_direction(&self, target: u64, draw: u64, direction: [f32; 3]) -> Result<(), String> {
    let mut orders = self.orders.borrow_mut();
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return Err("the entry has no instance order (declare instanceOrder at creation)".to_string());
    };
    entry.order.set_direction(direction)
  }

  /// The retained re-sort path: re-order the entry's retained copy under
  /// its current key state and, when the permutation actually changed,
  /// republish every slot from its mirror - no publish from the app
  /// anywhere. Reached from a direction change, from a count change on a
  /// handed-off mirror, and from an attach over a surviving mirror (the
  /// seed - a fresh entry's empty permutation always differs). A no-op on
  /// a gather entry (direction takes effect at its next publish), on a
  /// retained entry with nothing published, and on an unchanged
  /// permutation - the parked-camera gate: same order, no upload. Runs
  /// after the caller drops its target borrow, because the republish
  /// notes content on the reading targets.
  pub(super) fn rematerialize_retained_order(&self, target: u64, draw: u64) {
    // The republish takes pooled blocks; give it the raster thread's
    // returns first, or a camera-driven re-sort stream (no lease call
    // anywhere) mints a fresh block per re-sort while the returns pile up
    // unread - the leak that killed a phone at splat scale. A finished
    // sort lands first too, so a resubmit compares against the newest
    // permutation.
    self.drain_recycled_blocks();
    self.drain_order_results();
    if let Some(records) = self.rematerialize_in_flush(target, draw) {
      self.apply_drawn(target, draw, records);
    }
  }

  // The in-flush half of `rematerialize_retained_order`, under the
  // registry borrow: Some(records drawn) when a packed entry's selection
  // landed here, None otherwise (nothing to do, or a job submitted).
  fn rematerialize_in_flush(&self, target: u64, draw: u64) -> Option<usize> {
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let entry = orders.entries.get_mut(&(target, draw))?;
    if !entry.retains() {
      return None;
    }
    let key_buffer = entry.buffers[entry.key];
    if key_buffer == 0 {
      return None;
    }
    let mirror = orders.mirrors.get(&key_buffer)?;
    let len = mirror.published;
    if len == 0 {
      return None;
    }
    let keys = len / mirror.record_len(entry);
    if entry.order.indices.is_some() && keys > SYNC_SORT_MAX_KEYS && self.submit_order_sort(orders, target, draw) {
      return None;
    }
    let entry = orders.entries.get_mut(&(target, draw)).expect("resolved above");
    let mirror = orders.mirrors.get(&key_buffer).expect("resolved above");
    let packed = match (mirror.column.and_then(|c| c.packed), &mirror.runs) {
      (Some(key), Some(table)) => {
        let selection = self.selection_of(target, entry);
        order_packed(&entry.order.mirror_order(), key, &mirror.data[..len], table, &selection, &mut orders.scratch);
        true
      }
      _ => {
        order_permutation(
          &entry.order.mirror_order(),
          mirror.record_len(entry),
          &mirror.data[..len],
          &mut orders.scratch,
        );
        false
      }
    };
    if orders.scratch.perm() == entry.perm.as_slice() && (!packed || orders.scratch.ids() == entry.ids.as_slice()) {
      return None;
    }
    entry.perm.clear();
    entry.perm.extend_from_slice(orders.scratch.perm());
    if packed {
      entry.ids.clear();
      entry.ids.extend_from_slice(orders.scratch.ids());
    }
    let indexed = entry.order.indices.is_some();
    self.republish_slots(entry, &orders.mirrors, None);
    if indexed {
      if let Some(mirror) = orders.mirrors.get_mut(&key_buffer) {
        mirror.ids_written = true;
        if mirror.seeded_only() {
          self.send(RasterCmd::ResetDrawGauge { target, draw });
        }
      }
    }
    if packed {
      Some(entry.perm.len())
    } else {
      None
    }
  }

  /// A destroyed buffer stops resolving as ordered (its id is retired), and
  /// its mirror goes with it - the mirror is buffer-lifetime. The entry
  /// keeps its declaration: the growth pattern destroys the old buffer
  /// right after the swap, and the swap already re-keyed the order.
  pub(super) fn drop_order_buffer(&self, id: u64) {
    let mut orders = self.orders.borrow_mut();
    orders.mirrors.remove(&id);
    if let Some(key) = orders.by_buffer.remove(&id) {
      // Only clear the back-pointer when it still names this id (a swap
      // that already moved the entry to a new buffer leaves it alone).
      if let Some(entry) = orders.entries.get_mut(&key) {
        for slot in entry.buffers.iter_mut() {
          if *slot == id {
            *slot = 0;
          }
        }
      }
    }
  }

  /// Whether buffer `id` is some entry's ordered instance buffer - the
  /// `write_gpu_buffer` guard: a partial byte-offset write would land on
  /// gathered, not slot, positions.
  pub(super) fn buffer_has_order(&self, id: u64) -> bool {
    self.orders.borrow().by_buffer.contains_key(&id)
  }

  /// The lease publish hook (see `end_buffer_write`): when `id` is ordered,
  /// gather the block's records into key order via a second pooled block and
  /// return that; an unordered `id` returns the block untouched. On an error
  /// the block is already cancelled back to the pool - the lease is closed
  /// either way, matching end_buffer_write's contract.
  ///
  /// A single-slot gather entry computes its permutation from this very
  /// block and retains nothing. A retaining entry (multi-slot, or
  /// `retain: true`) mirrors: the block is copied in slot order, the key
  /// slot's publish recomputes the shared permutation, and when it changed
  /// the sibling slots republish from their mirrors in the same frame -
  /// every buffer always describes the same draw order. A publish onto a
  /// handed-off mirror takes the records back to the app's cadence
  /// (`owned` drops).
  pub(super) fn gather_for_publish(&self, id: u64, block: Vec<u8>, len: usize) -> Result<Vec<u8>, String> {
    let mut orders = self.orders.borrow_mut();
    let Some(&key) = orders.by_buffer.get(&id) else {
      return Ok(block);
    };
    let orders = &mut *orders;
    let entry = orders.entries.get_mut(&key).expect("by_buffer names a registered entry");
    if entry.order.indices.is_some() {
      self.write_leases.borrow_mut().cancel(id, block);
      return Err(format!(
        "buffer {id} holds an indexed instance order's sorted ids; its records are handed off (transferRecords), not published"
      ));
    }
    let slot = entry.slot_of(id).expect("an ordered buffer resolves to its slot");
    let stride = entry.strides[slot];
    if len % stride != 0 {
      self.write_leases.borrow_mut().cancel(id, block);
      return Err(format!(
        "publish of {len} bytes is not a whole number of {stride}-byte instance records (the buffer has an instance order)"
      ));
    }
    if !entry.retains() {
      let mut dst = self.write_leases.borrow_mut().take_free(id, block.len());
      gather_ordered(&entry.order, stride, &block[..len], &mut dst[..len], &mut orders.scratch);
      self.write_leases.borrow_mut().cancel(id, block);
      return Ok(dst);
    }
    let mirror = orders.mirrors.entry(id).or_default();
    let data = Arc::make_mut(&mut mirror.data);
    data.resize(len.max(data.len()), 0);
    data[..len].copy_from_slice(&block[..len]);
    mirror.held = len;
    mirror.published = len;
    mirror.owned = false;
    mirror.column = None;
    let mut changed = false;
    if slot == entry.key {
      order_permutation(&entry.order, stride, &block[..len], &mut orders.scratch);
      changed = orders.scratch.perm() != entry.perm.as_slice();
      if changed {
        entry.perm.clear();
        entry.perm.extend_from_slice(orders.scratch.perm());
      }
    }
    let mut dst = self.write_leases.borrow_mut().take_free(id, block.len());
    gather_permuted(&entry.perm, stride, &block[..len], &mut dst[..len]);
    self.write_leases.borrow_mut().cancel(id, block);
    if changed {
      self.republish_slots(entry, &orders.mirrors, Some(slot));
    }
    Ok(dst)
  }

  /// The spatial-sink publish path: `values` is the whole staging mirror of
  /// an ordered instance buffer (the core's slot-order truth), published as
  /// one full gathered write - a partial range cannot land on gathered
  /// positions, which is exactly why `write_gpu_buffer` rejects ordered
  /// buffers. Same retention and sibling-republish contract as the lease
  /// hook above.
  pub(super) fn ordered_instance_publish(&self, id: u64, values: &[f32]) -> Result<(), String> {
    self.drain_recycled_blocks();
    let mut orders = self.orders.borrow_mut();
    let Some(&key) = orders.by_buffer.get(&id) else {
      return Err(format!("buffer {id} has no instance order"));
    };
    let orders = &mut *orders;
    let entry = orders.entries.get_mut(&key).expect("by_buffer names a registered entry");
    if entry.order.indices.is_some() {
      return Err(format!(
        "buffer {id} holds an indexed instance order's sorted ids; its records are handed off (transferRecords), not published"
      ));
    }
    let slot = entry.slot_of(id).expect("an ordered buffer resolves to its slot");
    let stride = entry.strides[slot];
    let len = values.len() * 4;
    if len == 0 {
      return Ok(());
    }
    if len % stride != 0 {
      return Err(format!(
        "instance publish of {len} bytes is not a whole number of {stride}-byte records on buffer {id}"
      ));
    }
    let size = self.gpu_buffer_len(id)?;
    if len > size {
      return Err(format!("instance publish of {len} bytes exceeds buffer {id} size {size}"));
    }
    // The mirror doubles as the byte staging for the f32 values.
    let mirror = orders.mirrors.entry(id).or_default();
    let data = Arc::make_mut(&mut mirror.data);
    data.resize(len.max(data.len()), 0);
    for (v, out) in values.iter().zip(data.chunks_exact_mut(4)) {
      out.copy_from_slice(&v.to_ne_bytes());
    }
    mirror.held = len;
    mirror.published = len;
    mirror.owned = false;
    mirror.column = None;
    let mut changed = false;
    let mirror = orders.mirrors.get(&id).expect("just inserted");
    let perm: &[u32] = if slot == entry.key {
      order_permutation(&entry.order, stride, &mirror.data[..len], &mut orders.scratch);
      if entry.retains() {
        changed = orders.scratch.perm() != entry.perm.as_slice();
        if changed {
          entry.perm.clear();
          entry.perm.extend_from_slice(orders.scratch.perm());
        }
        &entry.perm
      } else {
        orders.scratch.perm()
      }
    } else {
      &entry.perm
    };
    let mut dst = self.write_leases.borrow_mut().take_free(id, size);
    gather_permuted(perm, stride, &mirror.data[..len], &mut dst[..len]);
    self.send(RasterCmd::WriteBufferLease { id, block: dst, len, recycle: self.block_recycle_tx.clone() });
    self.note_buffer_content(id);
    if changed {
      self.republish_slots(entry, &orders.mirrors, Some(slot));
    }
    Ok(())
  }

  /// Republish every slot but `skip` from its mirror under the entry's
  /// retained permutation - the coherence half of a permutation change: the
  /// key buffer just published in a new order, so every other buffer's GPU
  /// contents must follow in the same frame. `skip` is the slot whose
  /// publish triggered this (already sent); `None` republishes everything -
  /// the retained re-sort path, where no slot published at all. A slot
  /// that never published (empty mirror) or whose buffer is gone
  /// publishes nothing. An index stream (one slot by contract) is written
  /// WHOLE: the sorted ids, then the sentinel to the end of the buffer.
  fn republish_slots(&self, entry: &OrderedEntry, mirrors: &HashMap<u64, Mirror>, skip: Option<usize>) {
    for (slot, &stride) in entry.strides.iter().enumerate() {
      if skip == Some(slot) || stride == 0 || entry.buffers[slot] == 0 {
        continue;
      }
      let id = entry.buffers[slot];
      let Some(mirror) = mirrors.get(&id) else {
        continue;
      };
      if mirror.published == 0 {
        continue;
      }
      let indexed = entry.order.indices.is_some();
      let len = if indexed { entry.perm.len() * 4 } else { mirror.published };
      let size = match self.gpu_buffer_len(id) {
        Ok(size) if len <= size => size,
        _ => {
          log::warn!("[gpu] ordered sibling republish skipped: buffer {id} missing or smaller than its mirror");
          continue;
        }
      };
      let mut dst = self.write_leases.borrow_mut().take_free(id, size);
      let len = if indexed {
        materialize(entry, mirror, &mut dst[..size]);
        size
      } else {
        gather_permuted(&entry.perm, stride, &mirror.data[..len], &mut dst[..len]);
        len
      };
      self.send(RasterCmd::WriteBufferLease { id, block: dst, len, recycle: self.block_recycle_tx.clone() });
      self.note_buffer_content(id);
    }
  }
}
