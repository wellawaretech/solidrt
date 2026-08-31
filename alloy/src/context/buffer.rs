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
  /// changes reach the screen even when no new params arrive. The caller
  /// must request a frame.
  pub fn write_gpu_buffer(&self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let size = *self.buffer_sizes.borrow().get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    // An ordered buffer's GPU contents are in key order, so a byte-offset
    // write would land on the wrong records; ordered buffers publish whole
    // record sets through the lease, where the gather runs.
    if self.buffer_has_order(id) {
      return Err(format!(
        "buffer {id} has an instance order; publish whole records through beginBufferWrite/endBufferWrite"
      ));
    }
    let end = byte_offset.checked_add(data.len()).ok_or_else(|| "offset overflow".to_string())?;
    if end > size {
      return Err(format!("write of {} bytes at offset {byte_offset} exceeds buffer size {size}", data.len()));
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
    let mut leases = self.write_leases.borrow_mut();
    // Blocks the raster thread finished with, back into the pool (retired
    // ids drop). Lazy: nothing else needs to observe a recycle promptly.
    while let Ok((rid, block)) = self.block_recycle_rx.try_recv() {
      let sizes = self.buffer_sizes.borrow();
      leases.recycle(rid, block, |i| sizes.contains_key(&i));
    }
    leases.begin(id, size)
  }

  /// Publish the open lease's first `len` bytes at offset 0: the block moves
  /// to the raster thread (no copy) and comes back through the recycle
  /// channel. `len` 0 cancels - the lease closes, nothing is sent. Always
  /// closes the lease, error or not. The caller must request a frame on a
  /// non-zero publish (same contract as `write_gpu_buffer`).
  ///
  /// When the buffer is some entry's ordered instance buffer, the publish is
  /// where the order materializes: the records are gathered into key order
  /// through a second pooled block (see `gather_for_publish`), and THAT
  /// block moves to the raster thread - the one copy the ordering costs.
  pub fn end_buffer_write(&self, id: u64, len: usize) -> Result<(), String> {
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
    let block = self.gather_for_publish(id, block, len)?;
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
    // The ordering entry (if any) keeps its declaration - a later buffer
    // swap re-keys it - but the retired id stops resolving as ordered.
    self.drop_order_buffer(id);
    self.send(RasterCmd::DestroyBuffer { id });
  }

  /// The byte size of vertex buffer `id` from the UI-side size mirror: None
  /// for id 0 (no buffer bound), an error for an unknown id - caught here,
  /// before the create RPC, so the mistake throws at the call site. What the
  /// draw-range resolution and the captured draw bound (see
  /// `TargetMirror::bounds`) both read.
  pub(super) fn buffer_size(&self, id: u64) -> Result<Option<usize>, String> {
    if id == 0 {
      return Ok(None);
    }
    self.buffer_sizes.borrow().get(&id).copied().map(Some).ok_or_else(|| format!("buffer {id} not found"))
  }

  /// Read back part of a vertex buffer's contents by registry id. An
  /// ordered instance buffer reads back in gathered (key) order, not slot
  /// order - the GPU-side contents ARE the gathered records.
  pub fn read_gpu_buffer(&self, id: u64, byte_offset: usize, len: usize) -> Result<Vec<u8>, String> {
    if !self.buffer_sizes.borrow().contains_key(&id) {
      return Err(format!("buffer {id} not found"));
    }
    self.rpc(|reply| RasterCmd::ReadBuffer { id, byte_offset, len, reply })?
  }

  /// Byte length of a vertex buffer by registry id.
  pub fn gpu_buffer_len(&self, id: u64) -> Result<usize, String> {
    self.buffer_sizes.borrow().get(&id).copied().ok_or_else(|| format!("buffer {id} not found"))
  }
}
