use std::time::Duration;

use crate::fetch::HostLimits;

async fn acquires_within(limits: &HostLimits, host: &str, ms: u64) -> Option<tokio::sync::OwnedSemaphorePermit> {
  tokio::time::timeout(Duration::from_millis(ms), limits.acquire(host)).await.ok()
}

#[tokio::test]
async fn per_host_limit_blocks_and_releases() {
  let limits = HostLimits::new(2);
  let p1 = limits.acquire("a:80").await;
  let _p2 = limits.acquire("a:80").await;

  // Third slot on the same host waits...
  assert!(acquires_within(&limits, "a:80", 50).await.is_none());
  // ...while another host is unaffected.
  assert!(acquires_within(&limits, "b:80", 50).await.is_some());

  // Dropping a permit frees the slot.
  drop(p1);
  assert!(acquires_within(&limits, "a:80", 1000).await.is_some());
}