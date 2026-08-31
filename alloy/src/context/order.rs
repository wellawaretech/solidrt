use std::collections::HashMap;

use crate::gpu::{gather_ordered, gather_permuted, order_permutation, BufferIds, InstanceOrder, OrderScratch, MAX_INSTANCE_SLOTS};
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
  // buffer; the publish hooks resolve through this). Every instance buffer
  // of a multi-slot ordered entry appears here.
  by_buffer: HashMap<u64, (u64, u64)>,
  // The sort's reusable working memory, shared by every ordered entry (the
  // UI thread publishes one buffer at a time).
  scratch: OrderScratch,
}

struct OrderedEntry {
  order: InstanceOrder,
  // Per-slot instance record strides in bytes (pipeline layout state, fixed
  // for the entry's life - a buffer swap never changes them; 0 = unused).
  strides: [usize; MAX_INSTANCE_SLOTS],
  // The currently ordered buffer per slot (0 after a destroy with no swap).
  buffers: [u64; MAX_INSTANCE_SLOTS],
  // The retained permutation (perm[i] = the slot drawn i-th) from the key
  // slot's last publish. Retained only where `retains()` says so: on
  // multi-slot entries sibling buffers must gather under the key buffer's
  // permutation, and a `retain: true` entry keeps it to re-sort on a
  // direction change; a single-slot gather entry recomputes at each
  // publish and retains nothing (stage 1's contract, unchanged).
  perm: Vec<u32>,
  // Slot-order copies of each slot's last published records, so a
  // permutation change can republish every buffer coherently with no
  // publish from the app. Kept where `retains()` says so (plus the byte
  // staging of any spatial-sink publish); empty otherwise.
  mirrors: [Mirror; MAX_INSTANCE_SLOTS],
}

#[derive(Default)]
struct Mirror {
  data: Vec<u8>,
  // Published bytes (data may be larger after a shrink; never happens today
  // but the length is the truth either way).
  len: usize,
}

impl OrderedEntry {
  fn slots(&self) -> usize {
    self.strides.iter().filter(|&&s| s > 0).count()
  }

  fn slot_of(&self, buffer: u64) -> Option<usize> {
    self.buffers.iter().position(|&b| b != 0 && b == buffer)
  }

  // Whether the entry keeps mirrors and the permutation between publishes:
  // multi-slot coherence needs them, and `retain: true` opts a single-slot
  // entry in (the write-once strategy).
  fn retains(&self) -> bool {
    self.order.retain || self.slots() > 1
  }
}

impl InstanceOrders {
  pub(super) fn new() -> Self {
    Self { entries: HashMap::new(), by_buffer: HashMap::new(), scratch: OrderScratch::default() }
  }
}

impl Context {
  /// Validate an entry's declared instance order against its pipeline layout
  /// and buffers - the setup half, pure so creates can check before their
  /// RPC and commit with `insert_instance_order` after it succeeds. The
  /// contract: the key reads from the declared key slot's records, every
  /// instance buffer is ordered by exactly one entry, the buffers are
  /// pairwise distinct and distinct from the entry's vertex and index
  /// buffers.
  pub(super) fn check_instance_order(
    &self,
    order: &InstanceOrder,
    instance_strides: [usize; MAX_INSTANCE_SLOTS],
    ids: BufferIds,
  ) -> Result<(), String> {
    if instance_strides.iter().all(|&s| s == 0) {
      return Err("instanceOrder needs an instance buffer (the pipeline declares no instanceAttributes)".to_string());
    }
    if instance_strides[order.key_slot] == 0 {
      return Err(format!(
        "instanceOrder keys on slot {}, but the pipeline declares no instance attributes there",
        order.key_slot
      ));
    }
    order.check_stride(instance_strides[order.key_slot])?;
    let orders = self.orders.borrow();
    for (slot, &stride) in instance_strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      let buffer = ids.instance_buffers[slot];
      if buffer == ids.buffer || ids.index.is_some_and(|(i, _)| i == buffer) {
        return Err(format!(
          "instance buffer {buffer} is also the entry's {} buffer; an ordered buffer holds instance records only",
          if buffer == ids.buffer { "vertex" } else { "index" }
        ));
      }
      if ids.instance_buffers[..slot].contains(&buffer) {
        return Err(format!(
          "instance buffer {buffer} is bound to two slots; an ordered entry's instance buffers must be distinct"
        ));
      }
      if let Some((t, d)) = orders.by_buffer.get(&buffer) {
        return Err(format!("buffer {buffer} is already ordered by draw {d} of target {t}; one ordered entry per buffer"));
      }
    }
    Ok(())
  }

  /// Commit a checked instance order for entry (`target`, `draw`).
  pub(super) fn insert_instance_order(
    &self,
    target: u64,
    draw: u64,
    order: InstanceOrder,
    strides: [usize; MAX_INSTANCE_SLOTS],
    buffers: [u64; MAX_INSTANCE_SLOTS],
  ) {
    let mut orders = self.orders.borrow_mut();
    for (slot, &stride) in strides.iter().enumerate() {
      if stride > 0 {
        orders.by_buffer.insert(buffers[slot], (target, draw));
      }
    }
    orders
      .entries
      .insert((target, draw), OrderedEntry { order, strides, buffers, perm: Vec::new(), mirrors: Default::default() });
  }

  /// Drop one entry's order (remove_draw).
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
    let removed: Vec<(u64, u64)> =
      self.orders.borrow().entries.keys().filter(|(t, _)| *t == target).copied().collect();
    for (t, d) in removed {
      self.unregister_instance_order(t, d);
    }
  }

  /// The instance-buffer-swap half of the update transaction, split
  /// check/commit so a rejected swap changes nothing. A swap on an
  /// unordered entry passes untouched; on an ordered one the order follows
  /// the entry to the new buffers, every swapped slot at once.
  pub(super) fn check_order_swap(&self, target: u64, draw: u64, ids: BufferIds) -> Result<(), String> {
    let orders = self.orders.borrow();
    let Some(entry) = orders.entries.get(&(target, draw)) else {
      return Ok(());
    };
    for (slot, &stride) in entry.strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      let new_buffer = ids.instance_buffers[slot];
      if ids.instance_buffers[..slot].contains(&new_buffer) {
        return Err(format!(
          "instance buffer {new_buffer} is bound to two slots; an ordered entry's instance buffers must be distinct"
        ));
      }
      if new_buffer == entry.buffers[slot] {
        continue;
      }
      if let Some((t, d)) = orders.by_buffer.get(&new_buffer) {
        return Err(format!(
          "buffer {new_buffer} is already ordered by draw {d} of target {t}; one ordered entry per buffer"
        ));
      }
      if new_buffer == ids.buffer || ids.index.is_some_and(|(i, _)| i == new_buffer) {
        return Err(format!(
          "instance buffer {new_buffer} is also the entry's {} buffer; an ordered buffer holds instance records only",
          if new_buffer == ids.buffer { "vertex" } else { "index" }
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
    for (slot, &stride) in entry.strides.iter().enumerate() {
      if stride == 0 {
        continue;
      }
      let new_buffer = ids.instance_buffers[slot];
      let old = entry.buffers[slot];
      if new_buffer == old {
        continue;
      }
      if old != 0 {
        orders.by_buffer.remove(&old);
      }
      entry.buffers[slot] = new_buffer;
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

  /// The retained direction-change path: re-sort the entry's retained copy
  /// under its just-updated direction and, when the permutation actually
  /// changed, republish every slot from its mirror - no publish from the
  /// app anywhere. A no-op on a gather entry (direction takes effect at its
  /// next publish), on a retained entry that never published (empty key
  /// mirror), and on an unchanged permutation - the parked-camera gate:
  /// same order, no upload. Runs after `update_draw` drops its target
  /// borrow, because the republish notes content on the reading targets.
  pub(super) fn rematerialize_retained_order(&self, target: u64, draw: u64) {
    let mut orders = self.orders.borrow_mut();
    let orders = &mut *orders;
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return;
    };
    let key_slot = entry.order.key_slot;
    let len = entry.mirrors[key_slot].len;
    if !entry.retains() || len == 0 {
      return;
    }
    order_permutation(&entry.order, entry.strides[key_slot], &entry.mirrors[key_slot].data[..len], &mut orders.scratch);
    if orders.scratch.perm() == entry.perm.as_slice() {
      return;
    }
    entry.perm.clear();
    entry.perm.extend_from_slice(orders.scratch.perm());
    self.republish_slots(entry, None);
  }

  /// A destroyed buffer stops resolving as ordered (its id is retired), but
  /// the entry keeps its declaration: the growth pattern destroys the old
  /// buffer right after the swap, and the swap already re-keyed the order.
  pub(super) fn drop_order_buffer(&self, id: u64) {
    let mut orders = self.orders.borrow_mut();
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
  /// every buffer always describes the same draw order.
  pub(super) fn gather_for_publish(&self, id: u64, block: Vec<u8>, len: usize) -> Result<Vec<u8>, String> {
    let mut orders = self.orders.borrow_mut();
    let Some(&key) = orders.by_buffer.get(&id) else {
      return Ok(block);
    };
    let orders = &mut *orders;
    let entry = orders.entries.get_mut(&key).expect("by_buffer names a registered entry");
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
    let mirror = &mut entry.mirrors[slot];
    mirror.data.resize(len.max(mirror.data.len()), 0);
    mirror.data[..len].copy_from_slice(&block[..len]);
    mirror.len = len;
    let mut changed = false;
    if slot == entry.order.key_slot {
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
      self.republish_slots(entry, Some(slot));
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
    let mut orders = self.orders.borrow_mut();
    let Some(&key) = orders.by_buffer.get(&id) else {
      return Err(format!("buffer {id} has no instance order"));
    };
    let orders = &mut *orders;
    let entry = orders.entries.get_mut(&key).expect("by_buffer names a registered entry");
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
    let mirror = &mut entry.mirrors[slot];
    mirror.data.resize(len.max(mirror.data.len()), 0);
    for (v, out) in values.iter().zip(mirror.data.chunks_exact_mut(4)) {
      out.copy_from_slice(&v.to_ne_bytes());
    }
    mirror.len = len;
    let mut changed = false;
    let perm: &[u32] = if slot == entry.order.key_slot {
      order_permutation(&entry.order, stride, &entry.mirrors[slot].data[..len], &mut orders.scratch);
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
    gather_permuted(perm, stride, &entry.mirrors[slot].data[..len], &mut dst[..len]);
    self.send(RasterCmd::WriteBufferLease { id, block: dst, len, recycle: self.block_recycle_tx.clone() });
    self.note_buffer_content(id);
    if changed {
      self.republish_slots(entry, Some(slot));
    }
    Ok(())
  }

  /// Republish every slot but `skip` from its mirror under the entry's
  /// retained permutation - the coherence half of a permutation change: the
  /// key buffer just published in a new order, so every other buffer's GPU
  /// contents must follow in the same frame. `skip` is the slot whose
  /// publish triggered this (already sent); `None` republishes everything -
  /// the retained direction-change path, where no slot published at all.
  /// A slot that never published (empty mirror) or whose buffer is gone
  /// publishes nothing.
  fn republish_slots(&self, entry: &OrderedEntry, skip: Option<usize>) {
    for (slot, mirror) in entry.mirrors.iter().enumerate() {
      if skip == Some(slot) || entry.strides[slot] == 0 || mirror.len == 0 || entry.buffers[slot] == 0 {
        continue;
      }
      let id = entry.buffers[slot];
      let size = match self.gpu_buffer_len(id) {
        Ok(size) if mirror.len <= size => size,
        _ => {
          log::warn!("[gpu] ordered sibling republish skipped: buffer {id} missing or smaller than its mirror");
          continue;
        }
      };
      let mut dst = self.write_leases.borrow_mut().take_free(id, size);
      gather_permuted(&entry.perm, entry.strides[slot], &mirror.data[..mirror.len], &mut dst[..mirror.len]);
      self.send(RasterCmd::WriteBufferLease { id, block: dst, len: mirror.len, recycle: self.block_recycle_tx.clone() });
      self.note_buffer_content(id);
    }
  }
}
