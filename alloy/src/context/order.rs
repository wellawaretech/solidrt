use std::collections::HashMap;

use crate::gpu::{gather_ordered, BufferIds, InstanceOrder, OrderScratch, MAX_INSTANCE_SLOTS};

use super::Context;

// The instance-order registry (see gpu/order.rs for the primitive): which
// draw entries declared an instance order, and - derived - which buffers are
// ordered, for the publish hook in end_buffer_write. Entirely UI-side state:
// the raster thread receives already-gathered blocks and never learns
// ordering exists.
pub(super) struct InstanceOrders {
  // (target, draw) -> the entry's declared order. Draw 0 is a fixed-kind
  // target's single entry (draw-target entry ids start at 1, so 0 is
  // unambiguous).
  entries: HashMap<(u64, u64), OrderedEntry>,
  // Instance buffer id -> the one entry ordering it (one ordered entry per
  // buffer; the publish hook resolves through this).
  by_buffer: HashMap<u64, (u64, u64)>,
  // The sort's reusable working memory, shared by every ordered entry (the
  // UI thread publishes one buffer at a time).
  scratch: OrderScratch,
}

struct OrderedEntry {
  order: InstanceOrder,
  // The instance record stride in bytes (pipeline layout state, fixed for
  // the entry's life - a buffer swap never changes it).
  stride: usize,
  // The currently ordered buffer (slot 0; ordered entries bind exactly one).
  buffer: u64,
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
  /// stage-1 contract: exactly one instance buffer, ordered by exactly one
  /// entry, distinct from the entry's vertex and index buffers.
  pub(super) fn check_instance_order(
    &self,
    order: &InstanceOrder,
    instance_strides: [usize; MAX_INSTANCE_SLOTS],
    ids: BufferIds,
  ) -> Result<(), String> {
    if instance_strides[0] == 0 {
      return Err("instanceOrder needs an instance buffer (the pipeline declares no instanceAttributes)".to_string());
    }
    let slots = instance_strides.iter().filter(|&&s| s > 0).count();
    if slots > 1 {
      return Err(format!(
        "instanceOrder supports exactly one instance buffer for now; the pipeline declares {slots} instance slots"
      ));
    }
    order.check_stride(instance_strides[0])?;
    let buffer = ids.instance_buffers[0];
    if buffer == ids.buffer || ids.index.is_some_and(|(i, _)| i == buffer) {
      return Err(format!(
        "instance buffer {buffer} is also the entry's {} buffer; an ordered buffer holds instance records only",
        if buffer == ids.buffer { "vertex" } else { "index" }
      ));
    }
    if let Some((t, d)) = self.orders.borrow().by_buffer.get(&buffer) {
      return Err(format!("buffer {buffer} is already ordered by draw {d} of target {t}; one ordered entry per buffer"));
    }
    Ok(())
  }

  /// Commit a checked instance order for entry (`target`, `draw`).
  pub(super) fn insert_instance_order(&self, target: u64, draw: u64, order: InstanceOrder, stride: usize, buffer: u64) {
    let mut orders = self.orders.borrow_mut();
    orders.entries.insert((target, draw), OrderedEntry { order, stride, buffer });
    orders.by_buffer.insert(buffer, (target, draw));
  }

  /// Drop one entry's order (remove_draw).
  pub(super) fn unregister_instance_order(&self, target: u64, draw: u64) {
    let mut orders = self.orders.borrow_mut();
    if let Some(entry) = orders.entries.remove(&(target, draw)) {
      orders.by_buffer.remove(&entry.buffer);
    }
  }

  /// Drop every order of a reclaimed target.
  pub(super) fn unregister_target_orders(&self, target: u64) {
    let mut orders = self.orders.borrow_mut();
    let removed: Vec<(u64, u64)> = orders.entries.keys().filter(|(t, _)| *t == target).copied().collect();
    for key in removed {
      if let Some(entry) = orders.entries.remove(&key) {
        orders.by_buffer.remove(&entry.buffer);
      }
    }
  }

  /// The instance-buffer-swap half of the update transaction, split
  /// check/commit so a rejected swap changes nothing. A swap on an
  /// unordered entry passes untouched; on an ordered one the order follows
  /// the entry to the new buffer.
  pub(super) fn check_order_rekey(&self, target: u64, draw: u64, new_buffer: u64, ids: BufferIds) -> Result<(), String> {
    let orders = self.orders.borrow();
    if !orders.entries.contains_key(&(target, draw)) {
      return Ok(());
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
    Ok(())
  }

  pub(super) fn commit_order_rekey(&self, target: u64, draw: u64, new_buffer: u64) {
    let mut orders = self.orders.borrow_mut();
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return;
    };
    let old = entry.buffer;
    entry.buffer = new_buffer;
    orders.by_buffer.remove(&old);
    orders.by_buffer.insert(new_buffer, (target, draw));
  }

  /// Replace an ordered entry's projected-key direction (the `orderDirection`
  /// update). Takes effect at the entry's next publish. Errs on an entry
  /// with no instance order, or one whose key is a field.
  pub(super) fn set_instance_order_direction(&self, target: u64, draw: u64, direction: [f32; 3]) -> Result<(), String> {
    let mut orders = self.orders.borrow_mut();
    let Some(entry) = orders.entries.get_mut(&(target, draw)) else {
      return Err("the entry has no instance order (declare instanceOrder at creation)".to_string());
    };
    entry.order.set_direction(direction)
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
        if entry.buffer == id {
          entry.buffer = 0;
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

  /// The publish hook (see `end_buffer_write`): when `id` is ordered, gather
  /// the block's records into key order via a second pooled block and return
  /// that; an unordered `id` returns the block untouched. On an error the
  /// block is already cancelled back to the pool - the lease is closed
  /// either way, matching end_buffer_write's contract.
  pub(super) fn gather_for_publish(&self, id: u64, block: Vec<u8>, len: usize) -> Result<Vec<u8>, String> {
    let mut orders = self.orders.borrow_mut();
    let Some(&key) = orders.by_buffer.get(&id) else {
      return Ok(block);
    };
    let orders = &mut *orders;
    let entry = orders.entries.get(&key).expect("by_buffer names a registered entry");
    if len % entry.stride != 0 {
      let stride = entry.stride;
      self.write_leases.borrow_mut().cancel(id, block);
      return Err(format!(
        "publish of {len} bytes is not a whole number of {stride}-byte instance records (the buffer has an instance order)"
      ));
    }
    let mut dst = self.write_leases.borrow_mut().take_free(id, block.len());
    gather_ordered(&entry.order, entry.stride, &block[..len], &mut dst[..len], &mut orders.scratch);
    self.write_leases.borrow_mut().cancel(id, block);
    Ok(dst)
  }
}
