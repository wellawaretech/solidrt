use crate::raster::RasterCmd;

use super::Context;

impl Context {
  /// Create an interleaved vertex buffer from raw bytes, returning its id.
  /// Buffer ids are their own space (not texture ids); pipelines reference the
  /// buffer via `PipelineSpec::buffer_id`.
  pub fn create_gpu_buffer(&self, data: &[u8], label: Option<String>) -> Result<u64, String> {
    let id = self.next_buffer_id.get();
    self.rpc(|reply| RasterCmd::CreateBuffer { id, data: data.to_vec(), label, reply })??;
    self.next_buffer_id.set(id + 1);
    self.buffer_sizes.borrow_mut().insert(id, data.len());
    Ok(id)
  }

  /// Create a zeroed vertex buffer of `size` bytes - the natural create for
  /// buffers filled through the write lease (begin_buffer_write), where
  /// initial contents would be dead weight.
  pub fn create_gpu_buffer_zeroed(&self, size: usize, label: Option<String>) -> Result<u64, String> {
    let id = self.next_buffer_id.get();
    self.rpc(|reply| RasterCmd::CreateBuffer { id, data: vec![0u8; size], label, reply })??;
    self.next_buffer_id.set(id + 1);
    self.buffer_sizes.borrow_mut().insert(id, size);
    Ok(id)
  }

  /// Overwrite part of a vertex buffer (`data` at `byte_offset`, within the
  /// buffer's original size); every pipeline drawing from it re-renders with
  /// its last-applied params at the next dirty flush, so geometry-only
  /// changes reach the screen even when no new params arrive. A buffer a
  /// draw list reads takes the write into its mirror, in slot order, and
  /// the lists it touches rebuild at the next rebuild point. The caller
  /// must request a frame.
  pub fn write_gpu_buffer(&self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let size = *self.buffer_sizes.borrow().get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    self.check_indexed_publish(id)?;
    let end = byte_offset.checked_add(data.len()).ok_or_else(|| "offset overflow".to_string())?;
    if end > size {
      return Err(format!("write of {} bytes at offset {byte_offset} exceeds buffer size {size}", data.len()));
    }
    if self.list_write(id, data, byte_offset) == Some(false) {
      return Ok(());
    }
    self.send(RasterCmd::WriteBuffer { id, data: data.to_vec(), byte_offset });
    self.note_buffer_content(id);
    Ok(())
  }

  /// Open a zero-copy write into a vertex buffer: returns a staging block
  /// exactly the buffer's size for the caller to fill in place, published by
  /// `end_buffer_write`. Contents are UNSPECIFIED (a recycled block holds
  /// what was published the time before last), so fill everything you
  /// publish. The pointer stays valid until end/destroy for this id; no Rust
  /// reference into the block is formed while the lease is open - the caller
  /// owns the bytes exclusively.
  pub fn begin_buffer_write(&self, id: u64) -> Result<(*mut u8, usize), String> {
    let size = *self.buffer_sizes.borrow().get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    self.drain_recycled_blocks();
    self.write_leases.borrow_mut().begin(id, size)
  }

  /// Blocks the raster thread finished with, back into the pool (retired
  /// ids drop). Called wherever a staging block is about to be taken - the
  /// lease pair AND the core-side order writes (an indexed re-sort, the
  /// draw lists' gathers), which otherwise mint a fresh block per write
  /// while the returns pile up unread in the channel: a camera-driven
  /// re-sort stream with no JS publish anywhere leaked the full buffer
  /// size per re-sort until the process died (found at gaussian-splats
  /// stage B scale, ~250 MB/s on a phone).
  pub(super) fn drain_recycled_blocks(&self) {
    let mut leases = self.write_leases.borrow_mut();
    while let Ok((rid, block)) = self.block_recycle_rx.try_recv() {
      let sizes = self.buffer_sizes.borrow();
      leases.recycle(rid, block, |i| sizes.contains_key(&i));
    }
  }

  /// Publish the open lease's first `len` bytes at offset 0: the block moves
  /// to the raster thread (no copy) and comes back through the recycle
  /// channel. `len` 0 cancels - the lease closes, nothing is sent. Always
  /// closes the lease, error or not. The caller must request a frame on a
  /// non-zero publish (same contract as `write_gpu_buffer`).
  ///
  /// When a draw list reads the buffer, the publish lands in its mirror,
  /// in slot order, and the block goes back to the pool; it moves on to the
  /// raster thread only when an entry also draws the buffer directly.
  pub fn end_buffer_write(&self, id: u64, len: usize) -> Result<(), String> {
    self.drain_recycled_blocks();
    let block = self.write_leases.borrow_mut().end(id)?;
    if len == 0 {
      self.write_leases.borrow_mut().cancel(id, block);
      return Ok(());
    }
    if len > block.len() {
      let size = block.len();
      self.write_leases.borrow_mut().cancel(id, block);
      return Err(format!("publish of {len} bytes exceeds buffer size {size}"));
    }
    if let Err(e) = self.check_indexed_publish(id) {
      self.write_leases.borrow_mut().cancel(id, block);
      return Err(e);
    }
    if self.list_write(id, &block[..len], 0) == Some(false) {
      self.write_leases.borrow_mut().cancel(id, block);
      return Ok(());
    }
    self.send(RasterCmd::WriteBufferLease { id, block, len, recycle: self.block_recycle_tx.clone() });
    self.note_buffer_content(id);
    Ok(())
  }

  /// Free a vertex buffer: the id retires immediately (further writes error),
  /// while targets drawing from it hold their own reference - like their
  /// pipeline - so either destruction order is safe; the GL buffer is deleted
  /// once the last such target is destroyed.
  pub fn destroy_gpu_buffer(&self, id: u64) {
    self.buffer_sizes.borrow_mut().remove(&id);
    self.write_leases.borrow_mut().destroy(id);
    // An ordered entry keeps its declaration - a later buffer swap re-keys
    // it - but the retired id stops resolving as ordered.
    self.drop_order_buffer(id);
    self.spatial().forget_buffer(id);
    self.send(RasterCmd::DestroyBuffer { id });
  }

  /// The byte size of vertex buffer `id` from the UI-side size mirror: None
  /// for id 0 (no buffer bound), an error for an unknown id - caught here,
  /// before the create RPC, so the mistake throws at the call site. What the
  /// draw-range resolution and the captured draw bound (see
  /// `TargetMirror::bounds`) both read.

  /// Read back part of a vertex buffer's contents by registry id. A buffer
  /// only draw lists draw reads back from its mirror, in slot order: its
  /// GPU copy is not kept.
  pub fn read_gpu_buffer(&self, id: u64, byte_offset: usize, len: usize) -> Result<Vec<u8>, String> {
    let size = *self.buffer_sizes.borrow().get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    if byte_offset.checked_add(len).is_some_and(|end| end <= size) {
      if let Some(bytes) = self.list_read(id, byte_offset, len) {
        return Ok(bytes);
      }
    }
    self.rpc(|reply| RasterCmd::ReadBuffer { id, byte_offset, len, reply })?
  }

  /// Byte length of a vertex buffer by registry id.
  pub fn gpu_buffer_len(&self, id: u64) -> Result<usize, String> {
    self.buffer_sizes.borrow().get(&id).copied().ok_or_else(|| format!("buffer {id} not found"))
  }
}
