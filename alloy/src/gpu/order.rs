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
//!
//! Two materializations of one permutation: the gathered records (the
//! instance buffer holds the records themselves in key order) and, for a
//! population whose records live in a data texture the shader fetches by
//! id, the INDEX stream - the instance buffer holds the sorted record
//! indices as u32, K of them per instance record, and a re-sort uploads
//! 4 bytes per record instead of the record.

/// The id an index stream carries past the sorted population (every slot
/// of the last instance's group past the count, and the rest of the
/// buffer): the shader culls it before any fetch.
pub const INDEX_NONE: u32 = u32::MAX;

// Digit width of the LSD radix sort over the exact 32-bit key: 11 bits =
// 3 counting passes (11, 11, 10) with 2048-bucket histograms, one pass
// fewer than the 8-bit split at the price of a wider scatter; the sort
// order is the same at any width.
const RADIX_BITS: usize = 11;
const RADIX_BUCKETS: usize = 1 << RADIX_BITS;
const RADIX_PASSES: usize = (32 + RADIX_BITS - 1) / RADIX_BITS;

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

impl OrderKey {
  /// The key's byte offset in its record.
  pub fn offset(&self) -> usize {
    match *self {
      OrderKey::Field { offset } | OrderKey::Projected { offset, .. } => offset,
    }
  }

  /// The bytes the key reads: one f32 for a field, a vec3 for a position.
  pub fn bytes(&self) -> usize {
    match self {
      OrderKey::Field { .. } => 4,
      OrderKey::Projected { .. } => 12,
    }
  }

  /// The same key at another byte offset.
  fn at(&self, offset: usize) -> OrderKey {
    match *self {
      OrderKey::Field { .. } => OrderKey::Field { offset },
      OrderKey::Projected { direction, .. } => OrderKey::Projected { offset, direction },
    }
  }
}

/// How a handed-off position is packed (`PackedKey`): unorms over the
/// record's chunk box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackedFormat {
  /// 16 bits per axis in two words: x in the low half of the first, y in
  /// its high half, z in the low half of the second (its high half is the
  /// record's to use) - the splat record.
  Unorm16x3,
}

impl PackedFormat {
  /// The API's name for a format.
  pub fn parse(name: &str) -> Result<PackedFormat, String> {
    match name {
      "unorm16x3" => Ok(PackedFormat::Unorm16x3),
      other => Err(format!("packed format '{other}' is not supported (unorm16x3)")),
    }
  }

  /// The bytes the packed position takes in the record.
  pub fn bytes(self) -> usize {
    match self {
      PackedFormat::Unorm16x3 => 8,
    }
  }

  // The three unorms as integers, x first, from the record's words at
  // `at` (native-endian).
  fn unpack(self, bytes: &[u8], at: usize) -> [u32; 3] {
    match self {
      PackedFormat::Unorm16x3 => {
        let xy = read_u32(bytes, at);
        let z = read_u32(bytes, at + 4);
        [xy & 0xffff, xy >> 16, z & 0xffff]
      }
    }
  }

  // The largest unorm per axis, as a float divisor.
  fn steps(self) -> [f32; 3] {
    match self {
      PackedFormat::Unorm16x3 => [65535.0; 3],
    }
  }
}

/// A position key handed off packed: the format's words at the column's
/// offset, decoded against the box of the record's run, whose table the
/// hand-off brings (`RunTable`). The mirror then holds the packed words
/// alone, in storage order (a run is a span of records), and the sort
/// decodes a key from them and the run's box, read sequentially.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedKey {
  pub format: PackedFormat,
}

/// Importance samples per run in a run table's ramp (`RunTable::ramp`):
/// sample i is the run's importance at rank floor(i x len / samples),
/// non-increasing along the run, what the selection cuts a run by without
/// reading a record.
pub const RUN_RAMP_SAMPLES: usize = 16;

/// The data beside a packed hand-off: the records' runs. A run is a span
/// of consecutive records sharing a position box (the packed words are
/// unorms over it), stored by importance descending inside, so a prefix
/// of a run is its most important records. The core culls and thins by
/// the run: a box wholly outside the view skips whole, and a visible
/// run is cut where its ramp falls below the selection's contribution
/// threshold at the run's projected scale (`Selection`). Nothing here
/// names what the records are: a point cloud, a particle cache or a splat
/// cloud hands off the same table.
#[derive(Clone, Debug, PartialEq)]
pub struct RunTable {
  /// Each run's position box: min xyz, max xyz.
  pub bounds: Vec<[f32; 6]>,
  /// Each run's first record; run r spans starts[r]..starts[r + 1], the
  /// last to the record count. Ascending from 0.
  pub starts: Vec<u32>,
  /// `RUN_RAMP_SAMPLES` importance samples per run, in run order.
  pub ramp: Vec<f32>,
}

impl RunTable {
  /// The run count.
  pub fn len(&self) -> usize {
    self.starts.len()
  }

  pub fn is_empty(&self) -> bool {
    self.starts.is_empty()
  }

  /// Run `r`'s (first record, record count) over a population of `count`
  /// records.
  pub fn span(&self, r: usize, count: usize) -> (usize, usize) {
    let start = self.starts[r] as usize;
    let end = self.starts.get(r + 1).map_or(count, |&s| s as usize);
    (start, end.saturating_sub(start))
  }

  /// Run `r`'s ramp.
  pub fn ramp_of(&self, r: usize) -> &[f32] {
    &self.ramp[r * RUN_RAMP_SAMPLES..(r + 1) * RUN_RAMP_SAMPLES]
  }

  /// The table's contract over `count` records: a box, a start and a
  /// ramp per run, the runs ascending from record 0 with no gap or
  /// overlap and the last ending at the count, every ramp finite,
  /// non-negative and non-increasing.
  pub fn check(&self, count: usize) -> Result<(), String> {
    let runs = self.starts.len();
    if self.bounds.len() != runs {
      return Err(format!("the run table holds {} boxes for {runs} runs", self.bounds.len()));
    }
    if self.ramp.len() != runs * RUN_RAMP_SAMPLES {
      return Err(format!(
        "the run table holds {} ramp samples for {runs} runs ({RUN_RAMP_SAMPLES} per run)",
        self.ramp.len()
      ));
    }
    if count == 0 {
      if runs != 0 {
        return Err(format!("the run table lists {runs} runs for no records"));
      }
      return Ok(());
    }
    if runs == 0 {
      return Err(format!("the run table lists no runs for {count} records"));
    }
    if self.starts[0] != 0 {
      return Err(format!("the first run starts at record {}, not 0", self.starts[0]));
    }
    for r in 0..runs {
      let (start, len) = self.span(r, count);
      if len == 0 || start + len > count {
        return Err(format!(
          "run {r} starts at record {start} with {len} records of {count}; runs ascend and end at the count"
        ));
      }
      let ramp = self.ramp_of(r);
      let mut last = f32::INFINITY;
      for &s in ramp {
        if !(s.is_finite() && s >= 0.0 && s <= last) {
          return Err(format!("run {r}'s ramp is not finite, non-negative and non-increasing"));
        }
        last = s;
      }
    }
    Ok(())
  }
}

/// The view a packed population is selected under: the node's
/// model-to-clip matrix (column-major, the target's view-projection times
/// the node's world transform, exactly what its vertex stage projects
/// with). From it alone the core derives the frustum planes in the
/// model frame (the row sums of Gribb-Hartmann), the depth direction the
/// key sorts by (the w row for a perspective matrix, the z row for an
/// orthographic one) and the pixel scale at a run's box (the matrix's
/// x and y rows over the clip w at the box, the projection's Jacobian on
/// axis). Core does arithmetic on a caller-named matrix; no camera
/// concept enters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderView {
  pub clip: [f32; 16],
}

/// A matrix whose w row's direction is shorter than this is orthographic
/// (its w is a constant): the depth then follows the z row.
const VIEW_W_ROW_EPS: f32 = 1e-12;

impl OrderView {
  // Row r of the column-major matrix as (x, y, z, w).
  fn row(&self, r: usize) -> [f32; 4] {
    [self.clip[r], self.clip[4 + r], self.clip[8 + r], self.clip[12 + r]]
  }

  /// The unit depth direction the view orders positions by: the w row for
  /// a perspective matrix, the z row for an orthographic one; None when
  /// both are degenerate or not finite.
  pub fn direction(&self) -> Option<[f32; 3]> {
    for r in [3, 2] {
      let d = self.row(r);
      let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
      if len2.is_finite() && len2 > VIEW_W_ROW_EPS {
        let len = len2.sqrt();
        return Some([d[0] / len, d[1] / len, d[2] / len]);
      }
    }
    None
  }

  // The six frustum planes in the model frame, (a, b, c, d) with
  // ax + by + cz + d >= 0 inside, unnormalized (an in/out test needs no
  // norm).
  fn planes(&self) -> [[f32; 4]; 6] {
    let r0 = self.row(0);
    let r1 = self.row(1);
    let r2 = self.row(2);
    let r3 = self.row(3);
    let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
    let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    [add(r3, r0), sub(r3, r0), add(r3, r1), sub(r3, r1), add(r3, r2), sub(r3, r2)]
  }

  // The pixel scale squared at a box - pixels squared per model unit
  // squared, the factor a model-space area projects by - and whether the
  // box reaches into the view: None for a box wholly outside. The scale
  // is measured at the box's nearest clip depth (the smallest w of its
  // corners), so a run's biggest possible footprint decides its cut; a
  // box at or behind the eye (w at the floor) projects without bound and
  // is kept whole.
  fn scale2(&self, planes: &[[f32; 4]; 6], b: &[f32; 6], viewport: [f32; 2]) -> Option<f32> {
    for p in planes {
      let x = if p[0] >= 0.0 { b[3] } else { b[0] };
      let y = if p[1] >= 0.0 { b[4] } else { b[1] };
      let z = if p[2] >= 0.0 { b[5] } else { b[2] };
      if p[0] * x + p[1] * y + p[2] * z + p[3] < 0.0 {
        return None;
      }
    }
    let r3 = self.row(3);
    let mut w_min = f32::INFINITY;
    for corner in 0..8 {
      let x = if corner & 1 == 0 { b[0] } else { b[3] };
      let y = if corner & 2 == 0 { b[1] } else { b[4] };
      let z = if corner & 4 == 0 { b[2] } else { b[5] };
      w_min = w_min.min(r3[0] * x + r3[1] * y + r3[2] * z + r3[3]);
    }
    if !(w_min > VIEW_W_FLOOR) {
      return Some(f32::INFINITY);
    }
    let r0 = self.row(0);
    let r1 = self.row(1);
    let sx = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt() * 0.5 * viewport[0];
    let sy = (r1[0] * r1[0] + r1[1] * r1[1] + r1[2] * r1[2]).sqrt() * 0.5 * viewport[1];
    Some(sx * sy / (w_min * w_min))
  }
}

/// The clip depth below which a run's box counts as at the eye: its
/// projected scale is unbounded and the run is kept whole.
const VIEW_W_FLOOR: f32 = 1e-6;

/// What a packed population is selected by before it sorts
/// (`order_packed`): the view (None selects every run whole, as a
/// population drawn without a view is), the target's size in pixels (the
/// contribution's unit), the contribution in pixels below which a run's
/// tail is cut (0 cuts nothing), and the budget - the most records the
/// selection may keep, the threshold rising until it fits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
  pub view: Option<OrderView>,
  pub viewport: [f32; 2],
  pub threshold: f32,
  pub budget: usize,
}

/// A run's contribution in pixels, under a threshold search, is bounded
/// below by this: the geometric bisection's floor.
const SELECT_BISECT_FLOOR: f32 = 1e-6;
/// Geometric bisection steps the budget search takes between the floor
/// and the largest contribution in view: each halves the ratio, so 24
/// resolve a threshold to a few percent over the twelve decades a view
/// spans.
const SELECT_BISECTIONS: usize = 24;

// The records a run keeps at contribution floor `t` in the run's own
// importance units: through the last ramp sample at or above `t` (its
// record is kept), extended toward the first sample below it by the
// ramp's slope between the two. `len` is the run's record count.
fn run_cut(ramp: &[f32], len: usize, t: f32) -> usize {
  if ramp[0] < t {
    return 0;
  }
  let samples = ramp.len();
  let rank = |j: usize| j * len / samples;
  for j in 1..samples {
    if ramp[j] < t {
      let lo = rank(j - 1);
      let hi = rank(j);
      let frac = if ramp[j - 1] > ramp[j] { (ramp[j - 1] - t) / (ramp[j - 1] - ramp[j]) } else { 0.0 };
      return lo + 1 + ((hi - lo - 1) as f32 * frac) as usize;
    }
  }
  len
}

// The records every run keeps at contribution threshold `t` in pixels,
// over the per-run pixel scales (`scales`, None = culled): the run's
// floor in its own units is t / scale, an unbounded scale keeps the run
// whole, and the sum is what the budget is measured against.
fn selection_total(table: &RunTable, count: usize, scales: &[Option<f32>], t: f32) -> usize {
  let mut total = 0;
  for (r, scale) in scales.iter().enumerate() {
    let Some(s) = scale else { continue };
    let (_, len) = table.span(r, count);
    total += if *s == f32::INFINITY || t <= 0.0 { len } else { run_cut(table.ramp_of(r), len, t / s) };
  }
  total
}

/// Select the runs of a packed population to draw under `sel`, into
/// `scratch.runs` as (first record, records) in run order: the view
/// culls every run whose box lies outside it and cuts the visible ones
/// where their ramp falls under the threshold at the run's projected
/// scale; without a view every run is whole at unit scale, where the
/// threshold does not apply and the budget alone thins, by importance
/// across the whole population. Over budget, the threshold rises by
/// geometric bisection until the kept records fit, and the records a
/// ramp step leaves between the last threshold over budget and the first
/// under it fill the remainder in proportion, so the budget is met to
/// within a record per run of rounding whatever the ramps' shape (a flat
/// ramp is all or nothing to a threshold); a budget the runs at the eye
/// alone exceed (their scale is
/// unbounded, no threshold cuts them) is honored by cutting those runs in
/// proportion. Returns the records selected.
pub fn select_runs(table: &RunTable, count: usize, sel: &Selection, scratch: &mut OrderScratch) -> usize {
  let OrderScratch { runs, scales, .. } = scratch;
  runs.clear();
  scales.clear();
  let runs_len = table.len();
  scales.reserve(runs_len);
  let mut t = 0.0f32;
  match sel.view {
    Some(view) => {
      let planes = view.planes();
      for b in &table.bounds {
        scales.push(view.scale2(&planes, b, sel.viewport));
      }
      t = sel.threshold.max(0.0);
    }
    None => scales.resize(runs_len, Some(1.0)),
  }
  if sel.budget == 0 {
    return 0;
  }
  // The cut of run `r` at threshold `t`: whole at the eye or under no
  // threshold, nothing at an infinite one, the ramp's otherwise.
  let cut = |r: usize, s: f32, t: f32| -> usize {
    let (_, len) = table.span(r, count);
    if s == f32::INFINITY || t <= 0.0 {
      len
    } else if t == f32::INFINITY {
      0
    } else {
      run_cut(table.ramp_of(r), len, t / s)
    }
  };
  // The step the budget fills in proportion: (the threshold over budget,
  // its total, the records the budget has left past the fitting
  // threshold's total); None when the fitting threshold is exact.
  let mut step: Option<(f32, usize, usize)> = None;
  let mut proportion = 1.0f32;
  let total = selection_total(table, count, scales, t);
  if total > sel.budget {
    let mut unbounded = 0usize;
    let mut hi = SELECT_BISECT_FLOOR;
    for (r, scale) in scales.iter().enumerate() {
      let Some(s) = scale else { continue };
      let (_, len) = table.span(r, count);
      if *s == f32::INFINITY {
        unbounded += len;
      } else {
        hi = hi.max(table.ramp_of(r)[0] * s);
      }
    }
    if unbounded >= sel.budget {
      // Nothing beyond the runs at the eye fits: those alone, in
      // proportion.
      t = f32::INFINITY;
      proportion = sel.budget as f32 / unbounded.max(1) as f32;
    } else {
      // The kept count is non-increasing in t: bisect between the
      // current t (too many) and the contribution above which only the
      // unbounded runs remain (fits).
      let mut lo = t.max(SELECT_BISECT_FLOOR);
      let mut lo_total = total;
      hi *= 2.0;
      let mut hi_total = unbounded;
      for _ in 0..SELECT_BISECTIONS {
        let mid = (lo * hi).sqrt();
        let mid_total = selection_total(table, count, scales, mid);
        if mid_total > sel.budget {
          lo = mid;
          lo_total = mid_total;
        } else {
          hi = mid;
          hi_total = mid_total;
        }
      }
      t = hi;
      if hi_total < sel.budget && lo_total > hi_total {
        step = Some((lo, lo_total - hi_total, sel.budget - hi_total));
      }
    }
  }
  let mut selected = 0usize;
  for (r, scale) in scales.iter().enumerate() {
    let Some(s) = scale else { continue };
    let (start, len) = table.span(r, count);
    let mut kept =
      if *s == f32::INFINITY && proportion < 1.0 { (len as f32 * proportion) as usize } else { cut(r, *s, t) };
    if let Some((over, step_records, remaining)) = step {
      let more = cut(r, *s, over).saturating_sub(kept);
      kept += (more as u64 * remaining as u64 / step_records as u64) as usize;
    }
    if kept > 0 {
      runs.push((start as u32, kept as u32));
      selected += kept;
    }
  }
  selected
}

/// The key column of a handed-off record: what the index materialization's
/// mirror keeps of each record. An indexed order never republishes its
/// records (a re-sort writes ids), so the mirror holds the bytes the sort
/// reads and nothing else - 12 per record for a position key, 4 for a
/// field key, 12 for a packed position (the id and its two words) - and a
/// 1M-record population costs the engine 12 MB, not the record block.
/// Declared with the hand-off (`transferRecords`' key argument: a byte
/// stride and a float offset at the API, plus the packing) and matched
/// against the entry's order when it attaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyColumn {
  /// The handed-off record's byte stride.
  pub stride: usize,
  /// The key's byte offset inside one record.
  pub offset: usize,
  /// The key's byte length in the record: 4 for a field, 12 for a float
  /// position, the format's for a packed one (8 for unorm16x3).
  pub len: usize,
  /// Some when the position is packed (`PackedKey`).
  pub packed: Option<PackedKey>,
}

impl KeyColumn {
  /// Parse the API-boundary shape: `stride` in bytes, exactly one of
  /// `field` or `position` as a float offset into the record, and for a
  /// position optionally its packing.
  pub fn parse(
    stride: f64,
    field: Option<f64>,
    position: Option<f64>,
    packed: Option<PackedKey>,
  ) -> Result<KeyColumn, String> {
    let stride = record_stride(stride, "record key stride")?;
    let key = match (field, position) {
      (Some(_), Some(_)) => return Err("a record key is either field or position, not both".to_string()),
      (None, None) => return Err("a record key needs field or position (a float offset into the record)".to_string()),
      (Some(f), None) => OrderKey::Field { offset: float_offset(f, "record key field")? },
      (None, Some(p)) => OrderKey::Projected { offset: float_offset(p, "record key position")?, direction: [0.0; 3] },
    };
    if packed.is_some() && matches!(key, OrderKey::Field { .. }) {
      return Err("a packed record key is a position; a field key is one f32".to_string());
    }
    let len = match packed {
      Some(packed) => packed.format.bytes(),
      None => key.bytes(),
    };
    let column = KeyColumn { stride, offset: key.offset(), len, packed };
    if column.offset + column.len > column.stride {
      return Err(format!("record key at float {} does not fit the {}-byte record", column.offset / 4, column.stride));
    }
    Ok(column)
  }

  /// Bytes of one mirror record: the key's (the packed words alone for a
  /// packed position).
  pub fn mirror_len(&self) -> usize {
    self.len
  }

  /// Whether an entry's declared column (`InstanceOrder::key_column`, a
  /// float key) reads what this hand-off holds: the same column exactly,
  /// or a position key over a packed hand-off's stride and offset.
  pub fn accepts(&self, declared: KeyColumn) -> bool {
    match self.packed {
      None => *self == declared,
      Some(_) => {
        declared.packed.is_none()
          && declared.len == 12
          && declared.stride == self.stride
          && declared.offset == self.offset
      }
    }
  }

  /// Whether `len` bytes are a whole number of this column's records.
  pub fn check_records(&self, len: usize) -> Result<(), String> {
    if len % self.stride != 0 {
      return Err(format!("{len} bytes is not a whole number of {}-byte records", self.stride));
    }
    Ok(())
  }

  /// Copy the key column out of `records` (a whole number of records, see
  /// `check_records`) into `out`, `len` bytes per record.
  pub fn extract(&self, records: &[u8], out: &mut Vec<u8>) {
    debug_assert!(records.len() % self.stride == 0, "hand-off not a whole number of records");
    out.clear();
    out.reserve(records.len() / self.stride * self.len);
    for record in records.chunks_exact(self.stride) {
      out.extend_from_slice(&record[self.offset..self.offset + self.len]);
    }
  }

  /// What a packed hand-off must bring for `records`: a run table over
  /// exactly those records (`RunTable::check`).
  pub fn check_packed(&self, records_len: usize, table: &RunTable) -> Result<(), String> {
    if self.packed.is_none() {
      return Err("the record key is not packed; a run table goes with a packed key".to_string());
    }
    table.check(records_len / self.stride)
  }
}

/// One entry's declared instance order: the key source, which buffer's
/// records hold it, and the direction of the sort. Ascending draws
/// smallest key first; descending is the back-to-front alpha case when
/// larger keys are nearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceOrder {
  pub key: OrderKey,
  pub descending: bool,
  /// The pipeline buffer index whose records hold the key; None = the
  /// first instance-step buffer. On an entry with several instance-step
  /// buffers every one gathers under the key buffer's permutation,
  /// whoever writes it - a core-written pose buffer keyed by world y, or
  /// an app-written style buffer keyed by an explicit sort field.
  pub key_buffer: Option<usize>,
  /// The retained-copy strategy, for write-once populations (splat
  /// clouds): the registry keeps a CPU copy of each slot's published
  /// records, and a direction update re-sorts and republishes them
  /// core-side - no publish from the app. Position keys only (a field
  /// key re-orders when its records republish, so retaining buys nothing).
  pub retain: bool,
  /// The index materialization: Some(record stride in bytes) when the
  /// key buffer holds sorted record INDICES rather than records. The
  /// records are handed off (`instance_order_records`) with a `KeyColumn`
  /// declaring this stride and the key's offset, and the mirror keeps the
  /// key column only (`mirror_stride`, `mirror_order`); the buffer's
  /// instance record is `arrayStride / 4` u32 ids (K records per drawn
  /// instance), written whole on every re-sort with `INDEX_NONE` past the
  /// population. Retains by nature (the mirror is the only key copy).
  pub indices: Option<usize>,
  /// The level-of-detail threshold of a packed population (`Selection`):
  /// the contribution in pixels below which a run's tail is not drawn.
  /// 0 (the default) thins nothing; the budget alone decides.
  pub lod_threshold: f32,
}

impl InstanceOrder {
  /// Parse the API-boundary shape: `field` or `position` are float offsets
  /// into one instance record (exactly one of the two), `direction` is
  /// required with `position` and rejected with `field`, `buffer` names the
  /// pipeline buffer index holding the key (default: the first
  /// instance-step buffer), `retain` opts into the retained-copy strategy
  /// (position keys only), `indices` (a byte stride) selects the index
  /// materialization over handed-off records of that stride. Stored
  /// offsets are bytes.
  pub fn parse(
    field: Option<f64>,
    position: Option<f64>,
    direction: Option<[f32; 3]>,
    descending: bool,
    buffer: Option<f64>,
    retain: bool,
    indices: Option<f64>,
  ) -> Result<InstanceOrder, String> {
    let key_buffer = match buffer {
      None => None,
      Some(b) => {
        if !(b.is_finite() && b >= 0.0 && b.fract() == 0.0 && (b as usize) < super::vocab::MAX_BUFFERS) {
          return Err(format!("instanceOrder buffer must be an integer 0..{}, got {b}", super::vocab::MAX_BUFFERS - 1));
        }
        Some(b as usize)
      }
    };
    let key = match (field, position) {
      (Some(_), Some(_)) => return Err("instanceOrder takes either field or position, not both".to_string()),
      (None, None) => {
        return Err(
          "instanceOrder needs a key: field (float offset) or position (float offset) with direction".to_string(),
        )
      }
      (Some(f), None) => {
        if direction.is_some() {
          return Err("direction applies to a position key; a field key has none".to_string());
        }
        if retain {
          return Err("retain applies to a position key; a field key re-orders when its records republish".to_string());
        }
        OrderKey::Field { offset: float_offset(f, "instanceOrder field")? }
      }
      (None, Some(p)) => {
        let Some(direction) = direction else {
          return Err("a position key needs a direction (the view direction positions project onto)".to_string());
        };
        check_direction(direction)?;
        OrderKey::Projected { offset: float_offset(p, "instanceOrder position")?, direction }
      }
    };
    let indices = match indices {
      None => None,
      Some(s) => Some(record_stride(s, "instanceOrder indices.stride")?),
    };
    let order = InstanceOrder { key, descending, key_buffer, retain, indices, lod_threshold: 0.0 };
    if let Some(stride) = indices {
      order.check_stride(stride)?;
    }
    Ok(order)
  }

  /// This order with its LOD threshold (`lod_threshold`) set: a finite,
  /// non-negative contribution in pixels, on an indexed order only (the
  /// packed hand-off is where runs exist).
  pub fn with_lod(self, threshold: f64) -> Result<InstanceOrder, String> {
    Ok(InstanceOrder { lod_threshold: check_lod_threshold(threshold, self.indices.is_some())?, ..self })
  }

  /// Replace the LOD threshold (the `orderLod` update).
  pub fn set_lod(&mut self, threshold: f64) -> Result<(), String> {
    self.lod_threshold = check_lod_threshold(threshold, self.indices.is_some())?;
    Ok(())
  }
}

fn check_lod_threshold(threshold: f64, indexed: bool) -> Result<f32, String> {
  if !indexed {
    return Err("lod applies to an indexed instance order (the runs of a packed hand-off)".to_string());
  }
  if !(threshold.is_finite() && threshold >= 0.0) {
    return Err(format!("lod threshold must be a finite non-negative number of pixels, got {threshold}"));
  }
  Ok(threshold as f32)
}

impl InstanceOrder {
  /// The byte stride of the records the sort reads off the mirror: the key
  /// column's under the index materialization (the mirror holds nothing
  /// else), the instance buffer's otherwise.
  pub fn mirror_stride(&self, instance_stride: usize) -> usize {
    if self.indices.is_some() {
      self.key.bytes()
    } else {
      instance_stride
    }
  }

  /// The order as the sort reads it off the mirror: under the index
  /// materialization the key rebased to offset 0 of the key column, else
  /// itself.
  pub fn mirror_order(&self) -> InstanceOrder {
    if self.indices.is_none() {
      return *self;
    }
    InstanceOrder { key: self.key.at(0), ..*self }
  }

  /// The key column an index-materialized order expects its records handed
  /// off as; None for the gathered forms.
  pub fn key_column(&self) -> Option<KeyColumn> {
    self.indices.map(|stride| KeyColumn { stride, offset: self.key.offset(), len: self.key.bytes(), packed: None })
  }

  /// The key bytes must sit inside one record of `stride` bytes: a field key
  /// reads one f32, a projected key reads a vec3.
  pub fn check_stride(&self, stride: usize) -> Result<(), String> {
    let name = match self.key {
      OrderKey::Field { .. } => "field",
      OrderKey::Projected { .. } => "position",
    };
    if self.key.offset() + self.key.bytes() > stride {
      return Err(format!(
        "instanceOrder {name} offset {} does not fit the {stride}-byte instance record",
        self.key.offset() / 4
      ));
    }
    Ok(())
  }

  /// Replace the projected key's direction (the per-camera-move update).
  /// On a gather entry it takes effect at the next publish (gather retains
  /// nothing to re-sort in place); on a retained entry the registry
  /// re-materializes from its copy right after (`rematerialize_retained_order`).
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
    return Err(format!("{name} must be a non-negative integer float offset, got {value}"));
  }
  Ok(value as usize * 4)
}

// A record's byte stride at the API boundary: a positive multiple of 4
// (whole float records).
fn record_stride(value: f64, name: &str) -> Result<usize, String> {
  if !(value.is_finite() && value > 0.0 && value.fract() == 0.0 && (value as usize) % 4 == 0) {
    return Err(format!("{name} must be a positive multiple of 4 bytes (whole float records), got {value}"));
  }
  Ok(value as usize)
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
  // A packed population's selection (`select_runs`): the kept (first
  // record, records) per visible run, and each run's pixel scale squared
  // under the view (None = culled) while it is decided.
  runs: Vec<(u32, u32)>,
  scales: Vec<Option<f32>>,
  // The record ids of a packed population's selected records, in the
  // compacted slot order the keys and the permutation index: what the
  // permutation maps through to materialize.
  ids: Vec<u32>,
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

  /// The record ids `order_packed` selected, in the slot order its
  /// permutation indexes.
  pub fn ids(&self) -> &[u32] {
    &self.ids
  }

  /// The runs `order_packed` selected, as (first record, records): for
  /// the tests and a diagnostic.
  #[allow(dead_code)]
  pub fn runs(&self) -> &[(u32, u32)] {
    &self.runs
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

// The u32 at byte offset `at`, native-endian (the mirror's own writes).
fn read_u32(bytes: &[u8], at: usize) -> u32 {
  let mut b = [0u8; 4];
  b.copy_from_slice(&bytes[at..at + 4]);
  u32::from_ne_bytes(b)
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
  let keys = &mut scratch.keys;
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
  radix_sort(scratch);
}

/// `order_permutation` over a packed mirror: `words` holds the packed
/// position words per record in storage order (`KeyColumn::extract`),
/// `table` the runs. The selection comes first (`select_runs` under
/// `sel`), then the key pass over the selected runs only: the key is the
/// decoded position's depth along the order's direction - dot(box min, d)
/// plus each unorm times its axis step along d, three multiply-adds per
/// record on the integers, read sequentially - and the permutation is
/// over the compacted selected slots, whose record ids `scratch.ids()`
/// lists (`materialize_indices_through` turns it into ids). Returns the
/// records selected.
pub fn order_packed(
  order: &InstanceOrder,
  key: PackedKey,
  words: &[u8],
  table: &RunTable,
  sel: &Selection,
  scratch: &mut OrderScratch,
) -> usize {
  let word_bytes = key.format.bytes();
  debug_assert!(words.len() % word_bytes == 0, "packed mirror not whole records");
  let count = words.len() / word_bytes;
  let selected = select_runs(table, count, sel, scratch);
  let OrderKey::Projected { direction, .. } = order.key else {
    debug_assert!(false, "a packed key is a position key");
    return 0;
  };
  let steps = key.format.steps();
  let OrderScratch { keys, ids, runs, .. } = scratch;
  keys.clear();
  keys.reserve(selected);
  ids.clear();
  ids.reserve(selected);
  let mut r = 0usize;
  for &(start, len) in runs.iter() {
    // The runs come in table order; find this one's box.
    while table.starts[r] != start {
      r += 1;
    }
    let b = &table.bounds[r];
    let c = [
      b[0] * direction[0] + b[1] * direction[1] + b[2] * direction[2],
      (b[3] - b[0]) / steps[0] * direction[0],
      (b[4] - b[1]) / steps[1] * direction[1],
      (b[5] - b[2]) / steps[2] * direction[2],
    ];
    for slot in start as usize..(start + len) as usize {
      let q = key.format.unpack(words, slot * word_bytes);
      let f = c[0] + q[0] as f32 * c[1] + q[1] as f32 * c[2] + q[2] as f32 * c[3];
      let k = quantize(f);
      keys.push(if order.descending { !k } else { k });
      ids.push(slot as u32);
    }
  }
  radix_sort(scratch);
  selected
}

// The LSD radix passes over `scratch.keys`, leaving the permutation in
// `scratch.perm` (perm[i] = the slot drawn i-th), stable.
fn radix_sort(scratch: &mut OrderScratch) {
  let OrderScratch { keys, perm, tmp, counts, .. } = scratch;
  let count = keys.len();
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

/// Write `perm` (perm[i] = the slot drawn i-th) into `dst` as native-endian
/// u32 ids - the index materialization - and fill the rest of `dst` with
/// `INDEX_NONE`, so every id slot of the buffer is either a sorted record
/// or the sentinel and no stale id from a larger population survives. A
/// trailing partial u32 (a `dst` length not a multiple of 4) is left alone.
pub fn materialize_indices(perm: &[u32], dst: &mut [u8]) {
  let slots = dst.len() / 4;
  for (i, chunk) in dst.chunks_exact_mut(4).enumerate() {
    let id = if i < perm.len() { perm[i] } else { INDEX_NONE };
    chunk.copy_from_slice(&id.to_ne_bytes());
  }
  debug_assert!(perm.len() <= slots, "permutation longer than the index buffer");
}

/// `materialize_indices` for a permutation over a packed population's
/// selected slots: the id written is the slot's record id, read from
/// `ids` (`OrderScratch::ids`).
pub fn materialize_indices_through(perm: &[u32], ids: &[u32], dst: &mut [u8]) {
  let slots = dst.len() / 4;
  for (i, chunk) in dst.chunks_exact_mut(4).enumerate() {
    let id = if i < perm.len() { ids[perm[i] as usize] } else { INDEX_NONE };
    chunk.copy_from_slice(&id.to_ne_bytes());
  }
  debug_assert!(perm.len() <= slots, "permutation longer than the index buffer");
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
