use std::future::poll_fn;

use crate::stream::{from_bytes, ByteStream};

async fn drain(mut stream: ByteStream) -> Vec<Vec<u8>> {
  let mut chunks = Vec::new();
  while let Some(item) = poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
    chunks.push(item.expect("a one-shot stream never errors").to_vec());
  }
  chunks
}

#[tokio::test]
async fn from_bytes_is_one_chunk_then_end() {
  assert_eq!(drain(from_bytes(b"abc".to_vec())).await, vec![b"abc".to_vec()]);
}

#[tokio::test]
async fn from_bytes_empty_ends_at_once() {
  // No empty chunk: a consumer sees the end straight away.
  assert!(drain(from_bytes(Vec::new())).await.is_empty());
}
