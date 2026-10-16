// A byte sink a test keeps a handle on after a writer took it: what the
// writer and recorder tests read their output back from.

use std::io::{self, Cursor, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Clone, Default)]
pub(super) struct SharedSink(Arc<Mutex<Cursor<Vec<u8>>>>);

impl SharedSink {
  pub(super) fn bytes(&self) -> Vec<u8> {
    self.0.lock().unwrap_or_else(PoisonError::into_inner).get_ref().clone()
  }
}

impl Write for SharedSink {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.0.lock().unwrap_or_else(PoisonError::into_inner).write(buf)
  }
  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl Seek for SharedSink {
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
    self.0.lock().unwrap_or_else(PoisonError::into_inner).seek(pos)
  }
}
