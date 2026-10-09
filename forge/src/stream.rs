//! Engine-free byte-stream primitives.
//!
//! The common body stream type that capability cores produce and the
//! marshalling layer consumes. Names no scripting-engine types: a producer
//! crate's stream (reqwest for fetch responses, hyper for incoming request
//! bodies, tokio for child stdout) is adapted into one `ByteStream` whose error
//! is flattened to `io::Error`, so the rest of the code stays producer-agnostic.
//!
//! Beside it, what a byte duplex (a TCP `net::Conn`, a p2p `Stream`) is built
//! from: `Closing`, how the duplex ends from the local side, shared by its two
//! halves; `ReadSlot` + `read_half`, its read half as a `ByteStream` the
//! duplex can still end. The read half is a view the consumer owns and drops
//! when it stops reading; the duplex keeps the reader itself, so `close()`
//! releases it whether or not anyone is reading.

use bytes::Bytes;
use futures_core::Stream;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

/// A network-sourced byte stream (e.g. a fetch response, an incoming request
/// body), with its error flattened to `io::Error` so consumers stay
/// producer-crate-free. `Send` so a body can feed a worker thread (the
/// streamed byte source in [`source`](crate::source) pumps one from the I/O
/// runtime); every producer behind it is Send anyway.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send>>;

/// Adapts a foreign byte stream into the common `ByteStream`, flattening its error
/// to `io::Error`. The single bridge from a producer crate's stream (reqwest for
/// fetch responses, hyper for incoming request bodies) into our engine-internal
/// body type.
struct MapErrStream<E> {
  inner: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send>>,
}

impl<E: Into<Box<dyn std::error::Error + Send + Sync>>> Stream for MapErrStream<E> {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.inner.as_mut().poll_next(cx).map(|chunk| chunk.map(|r| r.map_err(io::Error::other)))
  }
}

pub fn to_byte_stream<S, E>(stream: S) -> ByteStream
where
  S: Stream<Item = Result<Bytes, E>> + Send + 'static,
  E: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
  Box::pin(MapErrStream { inner: Box::pin(stream) })
}

/// A `ByteStream` over already-buffered bytes: the whole buffer as one chunk,
/// then the end. Empty bytes end at once, so a consumer never sees an empty
/// chunk. Lets a buffered body and a network body share one consumer.
pub fn from_bytes(bytes: impl Into<Bytes>) -> ByteStream {
  Box::pin(OneShot(Some(bytes.into()).filter(|b| !b.is_empty())))
}

struct OneShot(Option<Bytes>);

impl Stream for OneShot {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    Poll::Ready(self.0.take().map(Ok))
  }
}

/// A stream that keeps `handle` alive as long as it lives: for a read half
/// whose transport is only kept up by a handle (a QUIC connection), so a
/// consumer still reading keeps it up after the duplex itself is gone.
pub fn holding<H: Unpin + Send + 'static>(stream: ByteStream, handle: H) -> ByteStream {
  Box::pin(Holding { inner: stream, _handle: handle })
}

struct Holding<H> {
  inner: ByteStream,
  _handle: H,
}

impl<H: Unpin + Send + 'static> Stream for Holding<H> {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.inner.as_mut().poll_next(cx)
  }
}

// ---- duplex ends ------------------------------------------------------------

/// How a byte duplex ends from the local side, shared by its read and write
/// halves. `close()` ends it gracefully: a read in flight or later ends as
/// end-of-stream, a write fails. `abort(reason)` ends it with an error: reads
/// and writes report `reason`, so a consumer piping the duplex sees the error
/// rather than a clean end. The first call wins; both are idempotent.
#[derive(Clone, Default)]
pub struct Closing {
  token: CancellationToken,
  reason: Arc<Mutex<Option<String>>>,
}

impl Closing {
  pub fn new() -> Self {
    Self::default()
  }

  /// End the duplex gracefully.
  pub fn close(&self) {
    self.token.cancel();
  }

  /// End the duplex with an error every half reports from now on.
  pub fn abort(&self, reason: String) {
    if !self.token.is_cancelled() {
      self.reason.lock().expect("closing reason lock").get_or_insert(reason);
    }
    self.token.cancel();
  }

  pub fn is_closed(&self) -> bool {
    self.token.is_cancelled()
  }

  /// The abort reason, `None` when the duplex is open or was closed cleanly.
  pub fn reason(&self) -> Option<String> {
    self.reason.lock().expect("closing reason lock").clone()
  }

  /// Resolves once the duplex is closed or aborted; what a pending read or
  /// write races against.
  pub async fn closed(&self) {
    self.token.cancelled().await;
  }

  /// What a write reports once the duplex ended: the abort reason, else
  /// `closed_message`.
  pub fn write_error(&self, closed_message: &str) -> String {
    self.reason().unwrap_or_else(|| closed_message.to_string())
  }

  /// What a read reports once the duplex ended: an error carrying the abort
  /// reason, or `None` (end-of-stream) after a plain close.
  pub fn read_end(&self) -> Option<io::Error> {
    self.reason().map(io::Error::other)
  }
}

/// A duplex's reader, kept by the duplex and read through the `read_half`
/// view. `close()` drops the reader: `read_half` then ends, and a view that
/// is dropped closes the slot itself, so a consumer that stops reading
/// releases the reader (for QUIC, telling the peer to stop sending).
pub struct ReadSlot<R>(Arc<Mutex<Option<R>>>);

impl<R> Clone for ReadSlot<R> {
  fn clone(&self) -> Self {
    ReadSlot(self.0.clone())
  }
}

impl<R> ReadSlot<R> {
  pub fn new(reader: R) -> Self {
    ReadSlot(Arc::new(Mutex::new(Some(reader))))
  }

  /// Drop the reader. Idempotent.
  pub fn close(&self) {
    self.0.lock().expect("read slot lock").take();
  }
}

/// The read half of a duplex as a `ByteStream`: one chunk of at most `chunk`
/// bytes per read, pulled only as the consumer polls, so the transport
/// advances no further than it is read. Ends at end-of-stream, when the slot
/// is closed, or when `closing` ends the duplex (as an error after an
/// abort). Dropping the stream closes the slot.
pub fn read_half<R>(slot: &ReadSlot<R>, closing: &Closing, chunk: usize) -> ByteStream
where
  R: AsyncRead + Unpin + Send + 'static,
{
  Box::pin(ReadHalf {
    slot: slot.clone(),
    closing: closing.clone(),
    closed: Box::pin(closing.token.clone().cancelled_owned()),
    buf: vec![0u8; chunk],
    done: false,
  })
}

struct ReadHalf<R> {
  slot: ReadSlot<R>,
  closing: Closing,
  closed: Pin<Box<WaitForCancellationFutureOwned>>,
  buf: Vec<u8>,
  done: bool,
}

impl<R: AsyncRead + Unpin + Send + 'static> Stream for ReadHalf<R> {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = &mut *self;
    if this.done {
      return Poll::Ready(None);
    }
    // The duplex ended: the reader is gone (or going), so this is what the
    // read reports, whether or not a read was pending on it.
    if this.closed.as_mut().poll(cx).is_ready() {
      this.done = true;
      this.slot.close();
      return Poll::Ready(this.closing.read_end().map(Err));
    }
    let mut guard = this.slot.0.lock().expect("read slot lock");
    let Some(reader) = guard.as_mut() else {
      this.done = true;
      return Poll::Ready(None);
    };
    let mut read_buf = ReadBuf::new(&mut this.buf);
    match Pin::new(reader).poll_read(cx, &mut read_buf) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(Err(e)) => {
        this.done = true;
        guard.take();
        Poll::Ready(Some(Err(e)))
      }
      Poll::Ready(Ok(())) => {
        let filled = read_buf.filled();
        if filled.is_empty() {
          this.done = true;
          guard.take();
          return Poll::Ready(None);
        }
        Poll::Ready(Some(Ok(Bytes::copy_from_slice(filled))))
      }
    }
  }
}

impl<R> Drop for ReadHalf<R> {
  fn drop(&mut self) {
    self.slot.close();
  }
}
