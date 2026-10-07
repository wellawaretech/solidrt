use std::collections::HashMap;

use crate::gpu::{
  gather_ordered, gather_permuted, materialize_indices, order_permutation, BufferIds, BufferStride, InstanceOrder,
  OrderScratch, StepMode, MAX_BUFFERS,
};
use crate::raster::RasterCmd;

use super::Context;

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
}

#[derive(Default)]
struct Mirror {
  data: Vec<u8>,
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

  // The byte stride of the sorted records: the handed-off record under the
  // index materialization, the key buffer's instance record otherwise.
  fn record_stride(&self) -> usize {
    self.order.record_stride(self.strides[self.key])
  }

  // The published prefix in bytes for an instance count over an owned
  // mirror holding `held` bytes (see `published_prefix`).
  fn published_for(&self, instances: usize, held: usize) -> usize {
    published_prefix(&self.order, self.strides[self.key], instances, held)
  }
}

// The published prefix of a handed-off mirror for an instance count: the
// first count records - count x K under the index materialization, whose
// instance record is K u32 ids - capped at what was handed off.
fn published_prefix(order: &InstanceOrder, instance_stride: usize, instances: usize, held: usize) -> usize {
  let per_instance = if order.indices.is_some() { instance_stride / 4 } else { 1 };
  (instances * per_instance * order.record_stride(instance_stride)).min(held)
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
  /// buffer whose record is whole u32 ids, over handed-off records only.
  /// Returns the resolved key buffer index.
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
    if let Some(record_stride) = order.indices {
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
        if mirror.held % record_stride != 0 {
          return Err(format!(
            "buffer {} holds {} handed-off bytes, not a whole number of {record_stride}-byte records",
            ids.buffers[key], mirror.held
          ));
        }
        // The id stream must hold every handed-off record's id, groups
        // rounded up.
        let records = mirror.held / record_stride;
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
    if let Some(mirror) = orders.mirrors.get_mut(&buffers[key]) {
      if mirror.owned {
        mirror.published = published_prefix(&order, strides[key], instances, mirror.held);
        seed = mirror.published > 0;
      }
    }
    orders.entries.insert((target, draw), OrderedEntry { order, key, strides, buffers, perm: Vec::new() });
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
  /// form): the mirror takes ownership of the bytes, so the app keeps no
  /// copy, partial rewrites are off the table, and the ordered entry -
  /// attached now or later - sorts and republishes from here. Hand off
  /// before the entry attaches; the attach seeds the first publish from
  /// this mirror (nothing draws until then) and checks the records
  /// against the buffer for the order's form (gathered records must fit
  /// it; an index stream must hold their ids). A second hand-off replaces
  /// the records wholesale.
  pub fn instance_order_records(&self, id: u64, data: &[u8]) -> Result<(), String> {
    self.gpu_buffer_len(id)?;
    if data.is_empty() {
      return Err(format!("record hand-off to buffer {id} is empty"));
    }
    let mut orders = self.orders.borrow_mut();
    let mirror = orders.mirrors.entry(id).or_default();
    mirror.data.clear();
    mirror.data.extend_from_slice(data);
    mirror.held = data.len();
    mirror.published = mirror.published.min(mirror.held);
    mirror.owned = true;
    Ok(())
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
    let published = entry.published_for(instances, mirror.held);
    if published == mirror.published {
      return false;
    }
    mirror.published = published;
    true
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
    // unread - the leak that killed a phone at splat scale.
    self.drain_recycled_blocks();
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return;
    };
    if !entry.retains() {
      return;
    }
    let key_buffer = entry.buffers[entry.key];
    if key_buffer == 0 {
      return;
    }
    let Some(mirror) = orders.mirrors.get(&key_buffer) else {
      return;
    };
    let len = mirror.published;
    if len == 0 {
      return;
    }
    order_permutation(&entry.order, entry.record_stride(), &mirror.data[..len], &mut orders.scratch);
    if orders.scratch.perm() == entry.perm.as_slice() {
      return;
    }
    entry.perm.clear();
    entry.perm.extend_from_slice(orders.scratch.perm());
    self.republish_slots(entry, &orders.mirrors, None);
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
    mirror.data.resize(len.max(mirror.data.len()), 0);
    mirror.data[..len].copy_from_slice(&block[..len]);
    mirror.held = len;
    mirror.published = len;
    mirror.owned = false;
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
    mirror.data.resize(len.max(mirror.data.len()), 0);
    for (v, out) in values.iter().zip(mirror.data.chunks_exact_mut(4)) {
      out.copy_from_slice(&v.to_ne_bytes());
    }
    mirror.held = len;
    mirror.published = len;
    mirror.owned = false;
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
        materialize_indices(&entry.perm, &mut dst[..size]);
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
