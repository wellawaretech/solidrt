//! Engine-free byte-stream primitive.
//!
//! The common body stream type that capability cores produce and the
//! marshalling layer consumes. Names no scripting-engine types: a producer
//! crate's stream (reqwest for fetch responses, hyper for incoming request
//! bodies, tokio for child stdout) is adapted into one `ByteStream` whose error
//! is flattened to `io::Error`, so the rest of the code stays producer-agnostic.

use bytes::Bytes;
use futures_core::Stream;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A network-sourced byte stream (e.g. a fetch response, an incoming request
/// body), with its error flattened to `io::Error` so consumers stay
/// producer-crate-free.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>>>>;

/// Adapts a foreign byte stream into the common `ByteStream`, flattening its error
/// to `io::Error`. The single bridge from a producer crate's stream (reqwest for
/// fetch responses, hyper for incoming request bodies) into our engine-internal
/// body type.
struct MapErrStream<E> {
  inner: Pin<Box<dyn Stream<Item = Result<Bytes, E>>>>,
}

impl<E: Into<Box<dyn std::error::Error + Send + Sync>>> Stream for MapErrStream<E> {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.inner.as_mut().poll_next(cx).map(|chunk| chunk.map(|r| r.map_err(io::Error::other)))
  }
}

pub fn to_byte_stream<S, E>(stream: S) -> ByteStream
where
  S: Stream<Item = Result<Bytes, E>> + 'static,
  E: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
  Box::pin(MapErrStream { inner: Box::pin(stream) })
}
