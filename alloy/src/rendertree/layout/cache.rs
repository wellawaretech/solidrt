use taffy::geometry::Size;
use taffy::style::AvailableSpace;
use taffy::tree::{LayoutInput, LayoutOutput, RunMode};

// Replacement for taffy::Cache. Taffy's cache keeps ONE entry per input shape
// (which dimensions are known / min- vs max-content), but its hit key also
// includes the parent width, and a single flex pass probes the same child with
// the same shape under several parent sizes (unknown during intrinsic sizing,
// resolved for the final pass). Same slot + different key = each store clobbers
// the previous entry, so nothing survives even within one frame and a
// one-node change re-measures the whole tree. This cache keeps the exact key
// semantics taffy uses for a hit, but stores entries in a ring with enough
// capacity for all parent-size variants a pass generates, so clean subtrees
// answer from cache across frames.

/// Measure-entry ring capacity. A pass probes a node with roughly a dozen
/// distinct (shape, parent width) combinations; 16 holds them all with slack.
/// Bounded, so per-node memory is a fixed ~0.5 KB regardless of app lifetime.
const MEASURE_ENTRIES: usize = 16;

// Key encoding mirrors taffy's: a known dimension stores its f32 bits, a
// missing one stores the available-space bits instead, with Definite encoded
// negated so "known 300" and "available 300" never collide (f32 layout sizes
// are non-negative).

fn option_bits(v: Option<f32>) -> u32 {
  v.map(f32::to_bits).unwrap_or(f32::INFINITY.to_bits())
}

fn available_bits(a: AvailableSpace) -> u32 {
  match a {
    AvailableSpace::Definite(v) => (-v).to_bits(),
    AvailableSpace::MinContent => f32::NEG_INFINITY.to_bits(),
    AvailableSpace::MaxContent => f32::INFINITY.to_bits(),
  }
}

fn mixed_bits(kd: Option<f32>, avail: AvailableSpace) -> u32 {
  kd.map(f32::to_bits).unwrap_or_else(|| available_bits(avail))
}

fn kd_avail_key(input: &LayoutInput) -> u64 {
  (mixed_bits(input.known_dimensions.width, input.available_space.width) as u64) << 32
    | mixed_bits(input.known_dimensions.height, input.available_space.height) as u64
}

/// ComputeSize hit key, matching taffy's: known dimensions/available space in
/// both axes plus the parent WIDTH only (taffy ignores parent height and the
/// requested axis when comparing measure entries).
#[derive(Clone, Copy, PartialEq, Eq)]
struct MeasureKey {
  kd_avail: u64,
  parent_width: u32,
}

impl MeasureKey {
  fn from(input: &LayoutInput) -> Self {
    Self { kd_avail: kd_avail_key(input), parent_width: option_bits(input.parent_size.width) }
  }
}

/// PerformLayout hit key: full input equality (both parent axes and the
/// requested axis), like taffy's final-layout entry.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FinalKey {
  kd_avail: u64,
  parent: u64,
  axis: u8,
}

impl FinalKey {
  fn from(input: &LayoutInput) -> Self {
    Self {
      kd_avail: kd_avail_key(input),
      parent: (option_bits(input.parent_size.width) as u64) << 32 | option_bits(input.parent_size.height) as u64,
      axis: input.axis as u8,
    }
  }
}

pub struct LayoutCache {
  final_layout: Option<(FinalKey, LayoutOutput)>,
  measures: [Option<(MeasureKey, Size<f32>)>; MEASURE_ENTRIES],
  /// Ring write position for the next new measure key.
  next: usize,
}

impl LayoutCache {
  pub fn new() -> Self {
    Self { final_layout: None, measures: [None; MEASURE_ENTRIES], next: 0 }
  }

  pub fn get(&self, input: &LayoutInput) -> Option<LayoutOutput> {
    match input.run_mode {
      RunMode::PerformLayout => {
        let key = FinalKey::from(input);
        self.final_layout.filter(|(k, _)| *k == key).map(|(_, out)| out)
      }
      RunMode::ComputeSize => {
        let key = MeasureKey::from(input);
        for (k, size) in self.measures.iter().flatten() {
          if *k == key {
            return Some(LayoutOutput::from_outer_size(*size));
          }
        }
        None
      }
      RunMode::PerformHiddenLayout => None,
    }
  }

  pub fn store(&mut self, input: &LayoutInput, output: LayoutOutput) {
    match input.run_mode {
      RunMode::PerformLayout => self.final_layout = Some((FinalKey::from(input), output)),
      RunMode::ComputeSize => {
        let key = MeasureKey::from(input);
        for entry in self.measures.iter_mut().flatten() {
          if entry.0 == key {
            entry.1 = output.size;
            return;
          }
        }
        self.measures[self.next] = Some((key, output.size));
        self.next = (self.next + 1) % MEASURE_ENTRIES;
      }
      RunMode::PerformHiddenLayout => {}
    }
  }

  pub fn clear(&mut self) {
    self.final_layout = None;
    self.measures = [None; MEASURE_ENTRIES];
    self.next = 0;
  }

  pub fn is_empty(&self) -> bool {
    self.final_layout.is_none() && self.measures.iter().all(|e| e.is_none())
  }
}

impl Default for LayoutCache {
  fn default() -> Self {
    Self::new()
  }
}
