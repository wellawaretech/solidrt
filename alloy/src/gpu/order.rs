//! Instance draw order within one entry (gather-at-publish): the pure half.
//! A draw entry may declare an instance order - a key read from each instance
//! record - and the Context permutes the records into key order while they
//! move through the buffer-write lease, so the GPU consumes key order while
//! writers keep addressing stable slots. This module owns the key vocabulary,
//! the setup validation, and the sort/gather; it knows nothing about buffers,
//! entries, or threads (Context's registry in context/order.rs does), which
//! keeps it testable in isolation.
//!
//! The sort is an LSD radix over order-preserving u32 quantizations of the
//! f32 keys - counting passes, no comparisons - because ordered populations
//! are large (30k sprites, 100k+ splats) and republish per frame, so the
//! sort must be linear and allocation-free in steady state. It is stable:
//! equal keys keep slot order, so the untouched case reproduces today's
//! insertion-order draw exactly.

// Digit width of the LSD radix sort: 8 bits = 4 counting passes over u32
// keys with 256-bucket histograms, the standard cache-friendly split.
const RADIX_BITS: usize = 8;
const RADIX_BUCKETS: usize = 1 << RADIX_BITS;
const RADIX_PASSES: usize = 32 / RADIX_BITS;

/// Where an entry's per-record sort key comes from. Offsets are BYTE offsets
/// into one instance record (the public API speaks float offsets; the flux
/// boundary converts through `InstanceOrder::parse`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderKey {
  /// The f32 at `offset` in each record - a y coordinate, an age, an
  /// explicit sort field the app writes.
  Field { offset: usize },
  /// dot(vec3 at `offset`, `direction`) - view depth of a record position
  /// along a caller-named direction. Core does arithmetic on caller-named
  /// data; no camera concept enters (the SharedSlot projection stance).
  Projected { offset: usize, direction: [f32; 3] },
}

/// One entry's declared instance order: the key source, which instance
/// slot's records hold it, and the direction of the sort. Ascending draws
/// smallest key first; descending is the back-to-front alpha case when
/// larger keys are nearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceOrder {
  pub key: OrderKey,
  pub descending: bool,
  /// The instance slot whose records hold the key (default 0). On a
  /// multi-buffer entry every slot gathers under the key slot's
  /// permutation, whoever writes it - a core-written pose slot keyed by
  /// world y, or an app-written style slot keyed by an explicit sort field.
  pub key_slot: usize,
}

impl InstanceOrder {
  /// Parse the API-boundary shape: `field` or `position` are float offsets
  /// into one instance record (exactly one of the two), `direction` is
  /// required with `position` and rejected with `field`, `slot` names the
  /// instance slot holding the key (default 0). Stored offsets are bytes.
  pub fn parse(
    field: Option<f64>,
    position: Option<f64>,
    direction: Option<[f32; 3]>,
    descending: bool,
    slot: Option<f64>,
  ) -> Result<InstanceOrder, String> {
    let key_slot = match slot {
      None => 0,
      Some(s) => {
        if !(s.is_finite() && s >= 0.0 && s.fract() == 0.0 && (s as usize) < super::vocab::MAX_INSTANCE_SLOTS) {
          return Err(format!(
            "instanceOrder slot must be an integer 0..{}, got {s}",
            super::vocab::MAX_INSTANCE_SLOTS - 1
          ));
        }
        s as usize
      }
    };
    let key = match (field, position) {
      (Some(_), Some(_)) => return Err("instanceOrder takes either field or position, not both".to_string()),
      (None, None) => {
        return Err("instanceOrder needs a key: field (float offset) or position (float offset) with direction".to_string())
      }
      (Some(f), None) => {
        if direction.is_some() {
          return Err("direction applies to a position key; a field key has none".to_string());
        }
        OrderKey::Field { offset: float_offset(f, "field")? }
      }
      (None, Some(p)) => {
        let Some(direction) = direction else {
          return Err("a position key needs a direction (the view direction positions project onto)".to_string());
        };
        check_direction(direction)?;
        OrderKey::Projected { offset: float_offset(p, "position")?, direction }
      }
    };
    Ok(InstanceOrder { key, descending, key_slot })
  }

  /// The key bytes must sit inside one record of `stride` bytes: a field key
  /// reads one f32, a projected key reads a vec3.
  pub fn check_stride(&self, stride: usize) -> Result<(), String> {
    let (offset, len, name) = match self.key {
      OrderKey::Field { offset } => (offset, 4, "field"),
      OrderKey::Projected { offset, .. } => (offset, 12, "position"),
    };
    if offset + len > stride {
      return Err(format!(
        "instanceOrder {name} offset {} does not fit the {stride}-byte instance record",
        offset / 4
      ));
    }
    Ok(())
  }

  /// Replace the projected key's direction (the per-camera-move update).
  /// Takes effect at the entry's next publish - gather retains nothing to
  /// re-sort in place.
  pub fn set_direction(&mut self, direction: [f32; 3]) -> Result<(), String> {
    let OrderKey::Projected { offset, .. } = self.key else {
      return Err("the entry's instance order uses a field key; orderDirection applies to position keys".to_string());
    };
    check_direction(direction)?;
    self.key = OrderKey::Projected { offset, direction };
    Ok(())
  }
}

fn float_offset(value: f64, name: &str) -> Result<usize, String> {
  if !(value.is_finite() && value >= 0.0 && value.fract() == 0.0) {
    return Err(format!("instanceOrder {name} must be a non-negative integer float offset, got {value}"));
  }
  Ok(value as usize * 4)
}

fn check_direction(direction: [f32; 3]) -> Result<(), String> {
  if direction.iter().any(|c| !c.is_finite()) {
    return Err("direction components must be finite".to_string());
  }
  if direction == [0.0, 0.0, 0.0] {
    return Err("direction must not be the zero vector".to_string());
  }
  Ok(())
}

/// The sort's working memory, held by the Context registry and reused across
/// publishes: after the first publish at a given population, a re-sort
/// allocates nothing.
#[derive(Default)]
pub struct OrderScratch {
  // Quantized key per slot (indexed by slot; never reordered).
  keys: Vec<u32>,
  // The permutation under construction: perm[i] = the slot drawn i-th.
  perm: Vec<u32>,
  // The ping-pong partner of `perm` for the radix passes.
  tmp: Vec<u32>,
  // All RADIX_PASSES histograms, built in one sweep over the keys.
  counts: Vec<u32>,
}

impl OrderScratch {
  /// The permutation `order_permutation` left behind (perm[i] = the slot
  /// drawn i-th) - what a multi-buffer entry retains and applies to sibling
  /// publishes.
  pub fn perm(&self) -> &[u32] {
    &self.perm
  }
}

// The f32 at byte offset `at` (native-endian, matching the Float32Array
// writer); goes through a copy because a lease block has no alignment
// guarantee.
fn read_f32(bytes: &[u8], at: usize) -> f32 {
  let mut b = [0u8; 4];
  b.copy_from_slice(&bytes[at..at + 4]);
  f32::from_ne_bytes(b)
}

// Order-preserving f32 -> u32: flip all bits of negatives, only the sign bit
// of non-negatives, so unsigned order equals numeric order (-inf lowest,
// +inf highest, the standard radix float transform). NaN in the usual
// positive quiet pattern maps above +inf and sorts last ascending.
fn quantize(f: f32) -> u32 {
  let b = f.to_bits();
  if b >> 31 == 1 {
    !b
  } else {
    b ^ 0x8000_0000
  }
}

/// Gather `src`'s records into `dst` in key order. `src.len()` must be a
/// whole number of `stride`-byte records and `dst` at least as long - both
/// the caller's checks (the Context registry validates the publish length
/// against the entry's stride before calling). Stable: equal keys keep slot
/// order.
pub fn gather_ordered(order: &InstanceOrder, stride: usize, src: &[u8], dst: &mut [u8], scratch: &mut OrderScratch) {
  order_permutation(order, stride, src, scratch);
  gather_permuted(&scratch.perm, stride, src, dst);
}

/// Sort `src`'s records by key and leave the permutation in `scratch.perm`
/// (perm[i] = the slot drawn i-th) - the compute half of `gather_ordered`,
/// exposed so a multi-buffer entry can gather several buffers under ONE
/// permutation (computed from the key buffer, applied to its siblings).
/// Same contracts and stability as `gather_ordered`.
pub fn order_permutation(order: &InstanceOrder, stride: usize, src: &[u8], scratch: &mut OrderScratch) {
  debug_assert!(stride > 0 && src.len() % stride == 0, "publish length not a whole number of records");
  let count = src.len() / stride;
  let OrderScratch { keys, perm, tmp, counts } = scratch;

  keys.clear();
  keys.reserve(count);
  for slot in 0..count {
    let at = slot * stride;
    let f = match order.key {
      OrderKey::Field { offset } => read_f32(src, at + offset),
      OrderKey::Projected { offset, direction } => {
        read_f32(src, at + offset) * direction[0]
          + read_f32(src, at + offset + 4) * direction[1]
          + read_f32(src, at + offset + 8) * direction[2]
      }
    };
    let k = quantize(f);
    keys.push(if order.descending { !k } else { k });
  }

  perm.clear();
  perm.extend(0..count as u32);
  tmp.clear();
  tmp.resize(count, 0);
  counts.clear();
  counts.resize(RADIX_PASSES * RADIX_BUCKETS, 0);
  for &k in keys.iter() {
    for pass in 0..RADIX_PASSES {
      counts[pass * RADIX_BUCKETS + ((k as usize >> (pass * RADIX_BITS)) & (RADIX_BUCKETS - 1))] += 1;
    }
  }

  for pass in 0..RADIX_PASSES {
    let hist = &mut counts[pass * RADIX_BUCKETS..(pass + 1) * RADIX_BUCKETS];
    // Every key in one bucket = the pass would be the identity; skip it
    // (clustered keys make this the common case for the high digits).
    if hist.iter().any(|&c| c as usize == count) {
      continue;
    }
    let mut sum = 0u32;
    for c in hist.iter_mut() {
      let n = *c;
      *c = sum;
      sum += n;
    }
    let shift = pass * RADIX_BITS;
    for &slot in perm.iter() {
      let digit = (keys[slot as usize] as usize >> shift) & (RADIX_BUCKETS - 1);
      tmp[hist[digit] as usize] = slot;
      hist[digit] += 1;
    }
    std::mem::swap(perm, tmp);
  }
}

/// Gather `src`'s records into `dst` following `perm` (perm[i] = the slot
/// drawn i-th). `dst` must hold at least `src.len()` bytes. The record
/// counts may disagree - a sibling buffer can publish more or fewer records
/// than the key buffer the permutation was computed from: perm entries past
/// `src`'s record count are skipped, records past the permutation's length
/// append in slot order. Every source record lands exactly once either way.
pub fn gather_permuted(perm: &[u32], stride: usize, src: &[u8], dst: &mut [u8]) {
  debug_assert!(stride > 0 && src.len() % stride == 0, "publish length not a whole number of records");
  debug_assert!(dst.len() >= src.len(), "gather destination smaller than the publish");
  let count = src.len() / stride;
  let mut out = 0usize;
  let mut place = |slot: usize| {
    dst[out * stride..(out + 1) * stride].copy_from_slice(&src[slot * stride..(slot + 1) * stride]);
    out += 1;
  };
  for &slot in perm {
    if (slot as usize) < count {
      place(slot as usize);
    }
  }
  for slot in perm.len()..count {
    place(slot);
  }
}
