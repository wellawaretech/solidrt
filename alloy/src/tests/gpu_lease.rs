use crate::gpu::WriteLeases;

// The pool cap in lease.rs: two recycled blocks per id (one leased, one in
// flight is the natural double buffer).
const CAP: usize = 2;

#[test]
fn begin_mints_and_double_begin_errors() {
  let mut leases = WriteLeases::new();
  let (ptr, len) = leases.begin(1, 64).expect("first begin");
  assert!(!ptr.is_null());
  assert_eq!(len, 64);
  let err = leases.begin(1, 64).expect_err("double begin must error");
  assert!(err.contains("open write"), "unexpected message: {err}");
  // A different id is independent.
  leases.begin(2, 16).expect("other id begins fine");
}

#[test]
fn end_without_begin_errors_and_end_closes() {
  let mut leases = WriteLeases::new();
  let err = leases.end(1).expect_err("end without begin must error");
  assert!(err.contains("no open write"), "unexpected message: {err}");
  leases.begin(1, 8).expect("begin");
  let block = leases.end(1).expect("end");
  assert_eq!(block.len(), 8);
  // The lease is closed: a fresh begin succeeds.
  leases.begin(1, 8).expect("begin after end");
}

#[test]
fn recycle_reuses_the_same_allocation() {
  let mut leases = WriteLeases::new();
  let (first_ptr, _) = leases.begin(1, 32).expect("begin");
  let block = leases.end(1).expect("end");
  leases.recycle(1, block, |_| true);
  let (second_ptr, _) = leases.begin(1, 32).expect("begin again");
  // Zero-alloc steady state: the recycled block is the one handed back.
  assert_eq!(first_ptr, second_ptr, "recycled begin should reuse the allocation");
}

#[test]
fn cancel_returns_the_block_to_the_pool() {
  let mut leases = WriteLeases::new();
  let (ptr, _) = leases.begin(1, 32).expect("begin");
  let block = leases.end(1).expect("end");
  leases.cancel(1, block);
  let (again, _) = leases.begin(1, 32).expect("begin after cancel");
  assert_eq!(ptr, again);
}

#[test]
fn recycle_drops_for_retired_ids_and_caps_the_pool() {
  let mut leases = WriteLeases::new();
  // A retired id's block drops on arrival: nothing to observe but no growth;
  // a later begin for it mints fresh without complaint.
  leases.recycle(9, vec![0u8; 16], |_| false);
  leases.begin(9, 16).expect("begin after retired recycle");

  // Cap: recycle four blocks; the pool keeps the first two (p1, p2) and
  // drops the rest. Pops are LIFO, so two begins return p2 then p1 - hold
  // those blocks live so their addresses cannot be reused - and a third
  // begin must mint fresh: its pointer can never be p1 or p2. (Were the cap
  // broken, the third begin would return the still-pooled p2.)
  let mut ptrs = Vec::new();
  for _ in 0..4 {
    let mut block = vec![0u8; 16];
    ptrs.push(block.as_mut_ptr());
    leases.recycle(1, block, |_| true);
  }
  let mut held = Vec::new();
  for _ in 0..CAP {
    let (ptr, _) = leases.begin(1, 16).expect("pooled begin");
    assert!(ptrs.contains(&ptr), "pooled begin should return a recycled block");
    held.push(leases.end(1).expect("end"));
  }
  let (third, _) = leases.begin(1, 16).expect("post-pool begin");
  assert_ne!(third, ptrs[0], "pool held more than the cap");
  assert_ne!(third, ptrs[1], "pool held more than the cap");
  drop(held);
}

#[test]
fn destroy_drops_open_lease_and_pool() {
  let mut leases = WriteLeases::new();
  leases.begin(1, 16).expect("begin");
  leases.destroy(1);
  // Open lease gone: end errors, begin mints fresh.
  assert!(leases.end(1).is_err());
  leases.begin(1, 16).expect("begin after destroy");
}
