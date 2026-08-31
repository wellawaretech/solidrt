//! Staging blocks for zero-copy GPU buffer writes (the begin/end lease pair
//! in context.rs). Pure bookkeeping: which buffer id has an open lease, and
//! the recycled blocks waiting for the next begin. No channel, no GL - the
//! caller moves blocks across the raster channel and feeds returns back in,
//! which keeps this testable without a thread in sight.
//!
//! Contract highlights (the JS-facing side documents the same): one open
//! lease per buffer id at a time; a leased block's heap allocation never
//! moves while the lease is open (Vec structs move between maps, their
//! backing storage does not), so a raw pointer handed out at `begin` stays
//! valid until `end`/`destroy`; block contents are UNSPECIFIED at begin (a
//! recycled block holds whatever was published the time before last), so
//! writers rewrite everything they publish.

use std::collections::HashMap;

// Recycled blocks kept per buffer id. Two is the natural double buffer
// (one leased, one in flight); anything beyond that means the raster thread
// is stalled, and holding more memory would only hide it.
const MAX_FREE_PER_ID: usize = 2;

pub struct WriteLeases {
  // Buffer id -> the block currently leased to the writer.
  open: HashMap<u64, Vec<u8>>,
  // Buffer id -> blocks returned by the raster thread, ready to lease again.
  free: HashMap<u64, Vec<Vec<u8>>>,
}

impl WriteLeases {
  pub fn new() -> Self {
    Self { open: HashMap::new(), free: HashMap::new() }
  }

  /// Open a lease for `id`, minting a block of `size` bytes when no recycled
  /// one is waiting. Returns the block's pointer and length; errors on a
  /// double begin. The pointer is valid until `end`/`cancel`/`destroy` for
  /// the same id - no Rust reference into the block may be formed while the
  /// lease is open (the writer owns the bytes).
  pub fn begin(&mut self, id: u64, size: usize) -> Result<(*mut u8, usize), String> {
    if self.open.contains_key(&id) {
      return Err(format!("buffer {id} already has an open write (end it first)"));
    }
    let mut block = match self.free.get_mut(&id).and_then(|blocks| blocks.pop()) {
      Some(block) => block,
      None => vec![0u8; size],
    };
    debug_assert_eq!(block.len(), size, "pooled block size drifted from buffer size");
    let ptr = block.as_mut_ptr();
    let len = block.len();
    self.open.insert(id, block);
    Ok((ptr, len))
  }

  /// Close the lease and take the block (to move across the channel).
  /// Errors when no lease is open.
  pub fn end(&mut self, id: u64) -> Result<Vec<u8>, String> {
    self.open.remove(&id).ok_or_else(|| format!("buffer {id} has no open write"))
  }

  /// Return a block the writer never published: straight back to the pool.
  pub fn cancel(&mut self, id: u64, block: Vec<u8>) {
    self.recycle(id, block, |_| true);
  }

  /// Take a pooled block for `id` without opening a lease (minting `size`
  /// bytes when none waits) - the gather destination for an ordered publish,
  /// where the leased block is the source and a second block receives the
  /// permuted records. Leaves any open lease untouched; the block returns
  /// through `recycle`/`cancel` like every other.
  pub fn take_free(&mut self, id: u64, size: usize) -> Vec<u8> {
    let block = match self.free.get_mut(&id).and_then(|blocks| blocks.pop()) {
      Some(block) => block,
      None => vec![0u8; size],
    };
    debug_assert_eq!(block.len(), size, "pooled block size drifted from buffer size");
    block
  }

  /// Accept a block back from the raster thread. `known` reports whether the
  /// id is still live; blocks for retired ids drop here, and the pool per id
  /// stays capped so a stalled raster thread bounds memory, not grows it.
  pub fn recycle(&mut self, id: u64, block: Vec<u8>, known: impl Fn(u64) -> bool) {
    if !known(id) {
      return;
    }
    let blocks = self.free.entry(id).or_default();
    if blocks.len() < MAX_FREE_PER_ID {
      blocks.push(block);
    }
  }

  /// Drop everything for a destroyed buffer: the open lease (the JS view is
  /// detached by the caller first) and the pool.
  pub fn destroy(&mut self, id: u64) {
    self.open.remove(&id);
    self.free.remove(&id);
  }
}

impl Default for WriteLeases {
  fn default() -> Self {
    Self::new()
  }
}
