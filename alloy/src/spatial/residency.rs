// The residency gate: which regions of a world should EXIST right now,
// given every viewer's view volume. Culling and LOD gate drawing; this
// gates memory, and is the same primitive for a 2d tile world's chunks and
// a 3d open world's regions. Two region forms: an EXPLICIT region is a
// bounded node (its world box, as the cull test carries it), a GRID is a
// lattice of cells that exist only while approached. A region is wanted
// by a viewer while within `enter` of its view volume and stays wanted
// until beyond `leave` (two-radius hysteresis), and is wanted overall if
// ANY viewer wants it. Changes are reported, never polled: leaves at
// once, enters paced under one scene-wide budget, highest priority then
// nearest eye first. The gate knows wanted or not; what a region holds is
// its handler's.
//
// Memory is bounded by LIMITS over a region's WEIGHT (the handler's cost
// of it, in a unit of its choosing: bytes, a tile layer's chunk texels):
// a grid's own limit over its cells and a scene-wide one over every
// region. A grid's `weight` and an explicit region's are the ESTIMATE a
// limit admits by; a handler that has built the content reports what it
// measured (`set_cell_weight`, `set_region_weight`), so a limit counts
// content, not cells. Past a limit the most needed pending region
// displaces the resident ones least needed, Unreal's streaming pool in
// our costume, where "needed" is PRIORITY (the grid's or region's,
// higher first) then URGENCY: the distance to the nearest wanting eye
// over the region's own `leave`, dimensionless, so grids in different
// units compare. A region held back by a limit is wanted and STALLED: no
// event, and no frame demanded for it until a view moves.

use std::collections::{HashMap, HashSet};

use super::bvh::Box3;
use super::cull::Frustum;
use super::math::invert;
use super::{LodView, Mat4, Node, NodeId};

/// A grid handle (`create_grid`), counted from 1.
pub type GridId = u64;

/// Enters delivered per frame scene-wide when no app set a budget: wide
/// enough to fill a desktop viewport of chunks in one frame on a camera
/// jump, narrow enough that a fast camera streams instead of bursting.
pub const DEFAULT_RESIDENCY_BUDGET: u32 = 16;

/// Where viewer keys handed out by `alloc_view_key` start: above any
/// texture id (a counter from near 0) and still exact in a JS double.
pub const VIEW_KEY_BASE: u64 = 1 << 48;

/// Below this the unprojected far corner's w is taken as 0: an infinite
/// projection, whose far corners are directions, not points.
const INFINITE_FAR_W: f32 = 1e-9;

/// The hysteresis band of a region: wanted within `enter` of the view
/// volume, kept until beyond `leave` (`leave >= enter`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionMargins {
  pub enter: f32,
  pub leave: f32,
}

impl RegionMargins {
  fn validate(&self) -> Result<(), String> {
    if !(self.enter.is_finite() && self.leave.is_finite()) {
      return Err("enter and leave must be finite".to_string());
    }
    if self.enter < 0.0 {
      return Err("enter must not be negative".to_string());
    }
    if self.leave < self.enter {
      return Err("leave must be at least enter".to_string());
    }
    Ok(())
  }

  /// The margin a viewer tests with: `leave` while it already wants the
  /// region, `enter` otherwise.
  fn margin(&self, wanted: bool) -> f32 {
    if wanted {
      self.leave
    } else {
      self.enter
    }
  }

  /// How needed a region at `distance` from its nearest wanting eye is,
  /// as a number comparable across regions of different units: the
  /// distance in `leave`s (a region with no band compares raw distance).
  fn urgency(&self, distance: f32) -> f32 {
    if self.leave > 0.0 {
      distance / self.leave
    } else {
      distance
    }
  }
}

/// An explicit region's declaration (`set_region`): its margins, the
/// estimated weight a limit admits it by (any unit, default 0 = free
/// under every limit until measured) and its priority (higher enters
/// first and is evicted last; default 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionSpec {
  pub margins: RegionMargins,
  pub weight: f64,
  pub priority: i32,
}

impl Default for RegionSpec {
  fn default() -> Self {
    RegionSpec { margins: RegionMargins { enter: 0.0, leave: 0.0 }, weight: 0.0, priority: 0 }
  }
}

impl RegionSpec {
  fn validate(&self) -> Result<(), String> {
    self.margins.validate()?;
    validate_weight(self.weight)
  }
}

/// A grid of cells: `origin` the corner of cell (0, 0, 0), `cell` the
/// size per axis with 0 meaning unbounded along that axis (the index is 0
/// there and the test ignores the axis), `bounds` an optional inclusive
/// cell-index box outside which no cell exists, `wrap` an optional period
/// in cells per axis (0 = none: indices modulo the period, the canonical
/// cell reported once however many images a viewer sees), `weight` the
/// handler's estimated cost of one resident cell (any unit, default 1;
/// 0 = free under every limit; a measured weight per cell overrides it),
/// `limit` the grid's own cap on resident weight, `priority` the order
/// among grids and regions (higher enters first, evicted last), `label`
/// the name the stats group the grid's footprint under.
#[derive(Clone, Debug, PartialEq)]
pub struct GridSpec {
  pub origin: [f32; 3],
  pub cell: [f32; 3],
  pub margins: RegionMargins,
  pub bounds: Option<[i32; 6]>,
  pub wrap: Option<[i32; 3]>,
  pub weight: f64,
  pub limit: Option<f64>,
  pub priority: i32,
  pub label: Option<String>,
}

/// A weight or limit must be finite and not negative; a limit positive.
fn validate_weight(weight: f64) -> Result<(), String> {
  if !(weight.is_finite() && weight >= 0.0) {
    return Err("weight must be finite and not negative".to_string());
  }
  Ok(())
}

fn validate_limit(limit: Option<f64>) -> Result<(), String> {
  if let Some(l) = limit {
    if !(l.is_finite() && l > 0.0) {
      return Err("limit must be finite and positive".to_string());
    }
  }
  Ok(())
}

impl GridSpec {
  fn validate(&self) -> Result<(), String> {
    self.margins.validate()?;
    validate_weight(self.weight)?;
    validate_limit(self.limit)?;
    if self.origin.iter().any(|v| !v.is_finite()) {
      return Err("origin must be finite".to_string());
    }
    if self.cell.iter().any(|v| !v.is_finite() || *v < 0.0) {
      return Err("cell sizes must be finite and not negative (0 = unbounded)".to_string());
    }
    if let Some(b) = self.bounds {
      if (0..3).any(|a| b[a] > b[a + 3]) {
        return Err("bounds min must not exceed max".to_string());
      }
    }
    if let Some(w) = self.wrap {
      if w.iter().any(|p| *p < 0) {
        return Err("wrap periods must not be negative (0 = none)".to_string());
      }
    }
    Ok(())
  }

  /// The canonical index of a raw cell index: wrapped into its period.
  fn canonical(&self, raw: [i64; 3]) -> [i32; 3] {
    let mut out = [0i32; 3];
    for a in 0..3 {
      let period = self.wrap.map(|w| w[a]).unwrap_or(0) as i64;
      let i = if period > 0 { raw[a].rem_euclid(period) } else { raw[a] };
      out[a] = i.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    }
    out
  }

  fn in_bounds(&self, cell: [i32; 3]) -> bool {
    match self.bounds {
      Some(b) => (0..3).all(|a| cell[a] >= b[a] && cell[a] <= b[a + 3]),
      None => true,
    }
  }

  /// The world box of the raw cell `raw`; an unbounded axis takes the
  /// enumeration box's extent `reach` there.
  fn cell_box(&self, raw: [i64; 3], reach: &Box3) -> Box3 {
    let mut b = [0.0f32; 6];
    for a in 0..3 {
      if self.cell[a] > 0.0 {
        b[a] = self.origin[a] + raw[a] as f32 * self.cell[a];
        b[a + 3] = b[a] + self.cell[a];
      } else {
        b[a] = reach[a];
        b[a + 3] = reach[a + 3];
      }
    }
    b
  }

  /// The world center of a canonical cell, with the unbounded axes taken
  /// at `at` (the eye: distance 0 along an axis the cell spans whole).
  fn cell_center(&self, cell: [i32; 3], at: [f32; 3]) -> [f32; 3] {
    let mut c = at;
    for a in 0..3 {
      if self.cell[a] > 0.0 {
        c[a] = self.origin[a] + (cell[a] as f32 + 0.5) * self.cell[a];
      }
    }
    c
  }
}

/// What a region event names: an explicit region's node, or a grid cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegionRef {
  Node(NodeId),
  Cell { grid: GridId, cell: [i32; 3] },
}

/// A residency change, reported once: the region should exist (Enter) or
/// should not any more (Leave). Every Leave follows an Enter of the same
/// region; a region that stopped being wanted before its paced enter went
/// out reports nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionEvent {
  Enter(RegionRef),
  Leave(RegionRef),
}

/// Where a wanted region stands (`region_state`, `cell_state`): its enter
/// not yet delivered (Pending), held back by a limit until a view moves
/// (Stalled), or delivered and not left since (Resident). A region nobody
/// wants has no state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionState {
  Pending,
  Stalled,
  Resident,
}

/// The wanted-or-not state every region form shares: the viewers wanting
/// it, whether its enter went out, whether the last pacing held it back,
/// and the weight the handler measured once it built the content (None
/// until then: the declared estimate stands in).
struct Wanted {
  wanted_by: Vec<u64>,
  resident: bool,
  stalled: bool,
  measured: Option<f64>,
}

impl Wanted {
  fn new() -> Self {
    Wanted { wanted_by: Vec::new(), resident: false, stalled: false, measured: None }
  }

  fn state(&self) -> Option<RegionState> {
    if self.wanted_by.is_empty() {
      None
    } else if self.resident {
      Some(RegionState::Resident)
    } else if self.stalled {
      Some(RegionState::Stalled)
    } else {
      Some(RegionState::Pending)
    }
  }
}

/// An explicit region's state, on its node.
pub(super) struct Region {
  spec: RegionSpec,
  wanted: Wanted,
  /// Moved or declared this flush: re-test against every viewer.
  pub(super) moved: bool,
}

impl Region {
  fn weight(&self) -> f64 {
    self.wanted.measured.unwrap_or(self.spec.weight)
  }
}

struct Grid {
  spec: GridSpec,
  /// Only the wanted cells: a cell nobody wants has no state at all.
  cells: HashMap<[i32; 3], Wanted>,
}

impl Grid {
  fn cell_weight(&self, cell: &Wanted) -> f64 {
    cell.measured.unwrap_or(self.spec.weight)
  }

  /// Cells whose enter went out (and no leave since): what the limits
  /// weigh. Counted off the cells, which are the wanted ones only.
  fn resident(&self) -> usize {
    self.cells.values().filter(|c| c.resident).count()
  }

  fn resident_weight(&self) -> f64 {
    self.cells.values().filter(|c| c.resident).map(|c| self.cell_weight(c)).sum()
  }
}

/// The gate's figures for the stats query: regions wanted by any viewer
/// (resident plus pending), the resident ones, the pending ones (enters
/// not yet delivered, the stalled among them held back by a limit), the
/// enters and leaves delivered so far (cumulative), and the resident
/// weight summed over every grid and explicit region.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResidencyStats {
  pub wanted: usize,
  pub resident: usize,
  pub pending: usize,
  pub stalled: usize,
  pub enters: u64,
  pub leaves: u64,
  pub weight: f64,
}

/// A viewer as the gate sees it (`Spatial::set_view`).
struct Viewer {
  frustum: Frustum,
  view_proj: Mat4,
  eye: [f32; 3],
  forward: [f32; 3],
}

pub(super) struct Residency {
  viewers: HashMap<u64, Viewer>,
  /// Viewers whose view changed or went since the last pass.
  dirty: Vec<u64>,
  grids: HashMap<GridId, Grid>,
  next_grid: GridId,
  next_view_key: u64,
  budget: u32,
  /// Enters delivered since the last `take_events`: the budget's counter,
  /// so every flush between two deliveries spends the same budget.
  enters_since_take: u32,
  /// Regions wanted whose enter has not gone out yet.
  pending: usize,
  /// Of the pending, those the last pacing held back by a limit: wanted,
  /// but nothing can change for them until a view moves, so they demand
  /// no frame and no pass. Always the count of the regions flagged
  /// stalled: a pacing recounts, a leave of a stalled region uncounts.
  stalled: usize,
  /// The scene-wide cap on resident weight over every region.
  limit: Option<f64>,
  /// Enters and leaves delivered so far, for the stats.
  enters_total: u64,
  leaves_total: u64,
  events: Vec<RegionEvent>,
}

impl Default for Residency {
  fn default() -> Self {
    Residency {
      viewers: HashMap::new(),
      dirty: Vec::new(),
      grids: HashMap::new(),
      next_grid: 1,
      next_view_key: VIEW_KEY_BASE,
      budget: DEFAULT_RESIDENCY_BUDGET,
      enters_since_take: 0,
      pending: 0,
      stalled: 0,
      limit: None,
      enters_total: 0,
      leaves_total: 0,
      events: Vec::new(),
    }
  }
}

/// The index of a region the pacer handles: the node's arena index, or
/// the grid and its canonical cell.
#[derive(Clone, Copy)]
enum Which {
  Node(u32),
  Cell(GridId, [i32; 3]),
}

/// How needed a region is, as the pacer orders by: priority first (higher
/// first), then urgency (nearer first).
#[derive(Clone, Copy)]
struct Need {
  priority: i32,
  urgency: f32,
}

impl Need {
  /// Whether `self` is needed less than `other`: a lower priority, or the
  /// same priority and further away.
  fn less_than(&self, other: &Need) -> bool {
    self.priority < other.priority || (self.priority == other.priority && self.urgency > other.urgency)
  }

  /// Most needed first.
  fn cmp_most_first(&self, other: &Need) -> std::cmp::Ordering {
    other.priority.cmp(&self.priority).then(self.urgency.total_cmp(&other.urgency))
  }
}

/// A resident region as the eviction scan sees it: its need, its weight,
/// which it is, and whether the scan already evicted it.
struct Resident {
  need: Need,
  weight: f64,
  which: Which,
  evicted: bool,
}

impl Residency {
  pub(super) fn alloc_view_key(&mut self) -> u64 {
    let key = self.next_view_key;
    self.next_view_key += 1;
    key
  }

  /// A viewer's view changed (Some) or went (None): the next pass re-tests
  /// every region against it.
  pub(super) fn set_view(&mut self, target: u64, view: Option<(Mat4, Frustum, LodView)>) {
    match view {
      Some((view_proj, frustum, lod)) => {
        self.viewers.insert(target, Viewer { frustum, view_proj, eye: lod.eye, forward: lod.forward });
      }
      None => {
        self.viewers.remove(&target);
      }
    }
    if !self.dirty.contains(&target) {
      self.dirty.push(target);
    }
  }

  pub(super) fn set_budget(&mut self, budget: u32) {
    self.budget = budget;
  }

  /// The scene-wide cap on resident weight (None = none). A lower cap
  /// takes effect at the next pass: the viewers are marked dirty so it
  /// runs even for a still world.
  pub(super) fn set_limit(&mut self, limit: Option<f64>) -> Result<(), String> {
    validate_limit(limit)?;
    self.limit = limit;
    self.mark_all_dirty();
    Ok(())
  }

  /// A grid's estimated weight per resident cell changed (a chunk re-baked
  /// at a new oversample): the limits re-weigh at the next pass.
  pub(super) fn set_grid_weight(&mut self, id: GridId, weight: f64) -> Result<(), String> {
    validate_weight(weight)?;
    let grid = self.grids.get_mut(&id).ok_or_else(|| format!("grid {id} not found"))?;
    grid.spec.weight = weight;
    self.mark_all_dirty();
    Ok(())
  }

  /// A grid's own cap on resident weight changed (None = none): the
  /// limits re-weigh at the next pass.
  pub(super) fn set_grid_limit(&mut self, id: GridId, limit: Option<f64>) -> Result<(), String> {
    validate_limit(limit)?;
    let grid = self.grids.get_mut(&id).ok_or_else(|| format!("grid {id} not found"))?;
    grid.spec.limit = limit;
    self.mark_all_dirty();
    Ok(())
  }

  /// The weight a handler measured for a wanted cell once it built the
  /// content (bytes, say): what the limits weigh it by from here on, in
  /// place of the grid's estimate. A cell nobody wants has no state to
  /// hold it and is refused.
  pub(super) fn set_cell_weight(&mut self, id: GridId, cell: [i32; 3], weight: f64) -> Result<(), String> {
    validate_weight(weight)?;
    let grid = self.grids.get_mut(&id).ok_or_else(|| format!("grid {id} not found"))?;
    let state = grid.cells.get_mut(&cell).ok_or_else(|| format!("cell {cell:?} of grid {id} is not wanted"))?;
    state.measured = Some(weight);
    self.mark_all_dirty();
    Ok(())
  }

  /// The weight a handler measured for an explicit region (see
  /// `set_cell_weight`); a node that is no region is refused.
  pub(super) fn set_region_weight(&mut self, node: &mut Node, weight: f64) -> Result<(), String> {
    validate_weight(weight)?;
    let region = node.region.as_mut().ok_or_else(|| "the node is not a region".to_string())?;
    region.wanted.measured = Some(weight);
    self.mark_all_dirty();
    Ok(())
  }

  /// Where a grid cell stands (None: not wanted); an unknown grid errs.
  pub(super) fn cell_state(&self, id: GridId, cell: [i32; 3]) -> Result<Option<RegionState>, String> {
    let grid = self.grids.get(&id).ok_or_else(|| format!("grid {id} not found"))?;
    Ok(grid.cells.get(&cell).and_then(Wanted::state))
  }

  /// Where an explicit region stands (None: not wanted, or no region).
  pub(super) fn region_state(node: &Node) -> Option<RegionState> {
    node.region.as_ref().and_then(|r| r.wanted.state())
  }

  /// Every viewer re-tests at the next pass: a limit or weight changed,
  /// or a grid was made, so the resident set may have to change without
  /// a view moving.
  fn mark_all_dirty(&mut self) {
    for &v in self.viewers.keys() {
      if !self.dirty.contains(&v) {
        self.dirty.push(v);
      }
    }
  }

  /// The gate's figures (`ResidencyStats`); the explicit regions are
  /// counted off their nodes.
  pub(super) fn stats(&self, nodes: &[Node], region_nodes: &[u32]) -> ResidencyStats {
    let mut resident = 0;
    let mut weight = 0.0;
    for grid in self.grids.values() {
      resident += grid.resident();
      weight += grid.resident_weight();
    }
    for &i in region_nodes {
      if let Some(r) = nodes[i as usize].region.as_ref() {
        if r.wanted.resident {
          resident += 1;
          weight += r.weight();
        }
      }
    }
    ResidencyStats {
      wanted: resident + self.pending,
      resident,
      pending: self.pending,
      stalled: self.stalled,
      enters: self.enters_total,
      leaves: self.leaves_total,
      weight,
    }
  }

  pub(super) fn create_grid(&mut self, spec: GridSpec) -> Result<GridId, String> {
    spec.validate()?;
    let id = self.next_grid;
    self.next_grid += 1;
    self.grids.insert(id, Grid { spec, cells: HashMap::new() });
    // Every viewer enumerates the new grid at the next pass.
    self.mark_all_dirty();
    Ok(id)
  }

  /// End a grid: every resident cell leaves, every pending one is dropped.
  pub(super) fn destroy_grid(&mut self, id: GridId) -> Result<(), String> {
    let grid = self.grids.remove(&id).ok_or_else(|| format!("grid {id} not found"))?;
    for (cell, state) in grid.cells {
      if state.resident {
        self.leaves_total += 1;
        self.events.push(RegionEvent::Leave(RegionRef::Cell { grid: id, cell }));
      } else {
        self.forget_pending(&state);
      }
    }
    Ok(())
  }

  /// A pending region stops being wanted: off the pending count, and off
  /// the stalled one if the last pacing held it back.
  fn forget_pending(&mut self, state: &Wanted) {
    self.pending -= 1;
    if state.stalled {
      self.stalled -= 1;
    }
  }

  /// Declare, re-declare or end (None) the region on node `i` (which has a
  /// box; the caller checked). A re-declaration keeps what viewers want
  /// and what the handler measured; its weight and priority re-weigh the
  /// limits at the next pass.
  pub(super) fn set_region(&mut self, i: u32, node: &mut Node, spec: Option<RegionSpec>) -> Result<bool, String> {
    match spec {
      Some(s) => {
        s.validate()?;
        match node.region.as_mut() {
          Some(r) => r.spec = s,
          None => node.region = Some(Box::new(Region { spec: s, wanted: Wanted::new(), moved: false })),
        }
        self.mark_all_dirty();
        Ok(true)
      }
      None => {
        self.end_region(i, node);
        Ok(false)
      }
    }
  }

  /// The node's region is over (ended or the node freed): a leave if it
  /// had entered, nothing if it was still pending.
  pub(super) fn end_region(&mut self, i: u32, node: &mut Node) {
    let Some(r) = node.region.take() else {
      return;
    };
    if r.wanted.resident {
      self.leaves_total += 1;
      self.events.push(RegionEvent::Leave(RegionRef::Node(node_id(i, node))));
    } else if !r.wanted.wanted_by.is_empty() {
      self.forget_pending(&r.wanted);
    }
  }

  /// The changes since the last take, with every enter whose leave came
  /// before the take COLLAPSED with it: a region wanted by one pass and
  /// unwanted by the next within one delivery window (a scene's default
  /// camera at creation and the app's camera a flush later, a viewer
  /// that swept past) was no change to the handler, which would only
  /// start a load to abort it. The enter's budget is refunded and the
  /// counters forget the pair; the order of the rest is kept.
  pub(super) fn take_events(&mut self) -> Vec<RegionEvent> {
    self.enters_since_take = 0;
    let events = std::mem::take(&mut self.events);
    let mut keep: Vec<Option<RegionEvent>> = Vec::with_capacity(events.len());
    let mut entered: HashMap<RegionRef, usize> = HashMap::new();
    for event in events {
      match event {
        RegionEvent::Enter(r) => {
          entered.insert(r, keep.len());
          keep.push(Some(event));
        }
        RegionEvent::Leave(r) => match entered.remove(&r) {
          Some(i) => {
            keep[i] = None;
            self.enters_total -= 1;
            self.leaves_total -= 1;
          }
          None => keep.push(Some(event)),
        },
      }
    }
    keep.into_iter().flatten().collect()
  }

  /// Something waits on a frame: events to deliver, or enters to pace
  /// (the stalled ones wait on a view, not a frame).
  pub(super) fn demand(&self) -> bool {
    !self.events.is_empty() || self.pending > self.stalled
  }

  /// Whether the pass has anything to do this flush.
  pub(super) fn needs_pass(&self, regions_moved: bool) -> bool {
    regions_moved || !self.dirty.is_empty() || self.pending > self.stalled
  }

  /// The pass, after the flush's walk (world boxes fresh) and the cull
  /// pass: re-test every explicit region against the viewers that moved
  /// (every viewer, for a region that moved), re-enumerate every grid
  /// under the viewers that moved, then pace the pending enters most
  /// needed first. `region_nodes` is the list of nodes carrying a region,
  /// pruned here of the dead.
  pub(super) fn pass(&mut self, nodes: &mut [Node], region_nodes: &mut Vec<u32>) {
    let dirty = std::mem::take(&mut self.dirty);
    region_nodes.retain(|&i| nodes[i as usize].alive && nodes[i as usize].region.is_some());
    self.test_regions(nodes, region_nodes, &dirty);
    if !dirty.is_empty() {
      self.enumerate_grids(&dirty);
    }
    self.pace_enters(nodes, region_nodes);
  }

  fn test_regions(&mut self, nodes: &mut [Node], region_nodes: &[u32], dirty: &[u64]) {
    let all: Vec<u64> = self.viewers.keys().copied().collect();
    for &i in region_nodes {
      let node = &mut nodes[i as usize];
      let id = node_id(i, node);
      let world_box = node.world_box;
      let region = node.region.as_mut().expect("region nodes carry a region");
      let moved = std::mem::replace(&mut region.moved, false);
      let was_wanted = !region.wanted.wanted_by.is_empty();
      let viewers: &[u64] = if moved { &all } else { dirty };
      for &v in viewers {
        let had = region.wanted.wanted_by.contains(&v);
        let want = match (self.viewers.get(&v), world_box) {
          (Some(viewer), Some(b)) => viewer.frustum.intersects(&b, region.spec.margins.margin(had)),
          _ => false,
        };
        if want && !had {
          region.wanted.wanted_by.push(v);
        } else if !want && had {
          region.wanted.wanted_by.retain(|&w| w != v);
        }
      }
      // A viewer that went never tests true above, but one that went
      // without ever being dirty cannot happen: `set_view` marks it.
      let now_wanted = !region.wanted.wanted_by.is_empty();
      if was_wanted && !now_wanted {
        if region.wanted.resident {
          region.wanted.resident = false;
          self.leaves_total += 1;
          self.events.push(RegionEvent::Leave(RegionRef::Node(id)));
        } else {
          self.pending -= 1;
          if region.wanted.stalled {
            self.stalled -= 1;
          }
        }
        region.wanted.stalled = false;
      } else if !was_wanted && now_wanted {
        self.pending += 1;
      }
    }
  }

  fn enumerate_grids(&mut self, dirty: &[u64]) {
    let Residency { viewers, grids, events, pending, stalled, leaves_total, .. } = self;
    for (&gid, grid) in grids.iter_mut() {
      for &v in dirty {
        // Forget the viewer everywhere first, remembering where it was:
        // a cell it still reaches with `leave` keeps it below, the rest
        // fall out in the sweep.
        let mut had: HashSet<[i32; 3]> = HashSet::new();
        for (cell, state) in grid.cells.iter_mut() {
          if state.wanted_by.contains(&v) {
            state.wanted_by.retain(|&w| w != v);
            had.insert(*cell);
          }
        }
        let Some(viewer) = viewers.get(&v) else {
          continue;
        };
        let spec = &grid.spec;
        let Some(reach) = viewer.reach(spec.margins.leave) else {
          continue;
        };
        let mut lo = [0i64; 3];
        let mut hi = [0i64; 3];
        for a in 0..3 {
          if spec.cell[a] > 0.0 {
            lo[a] = ((reach[a] - spec.origin[a]) / spec.cell[a]).floor() as i64;
            hi[a] = ((reach[a + 3] - spec.origin[a]) / spec.cell[a]).floor() as i64;
          }
        }
        for i in lo[0]..=hi[0] {
          for j in lo[1]..=hi[1] {
            for k in lo[2]..=hi[2] {
              let raw = [i, j, k];
              let cell = spec.canonical(raw);
              if !spec.in_bounds(cell) {
                continue;
              }
              let b = spec.cell_box(raw, &reach);
              let margin = spec.margins.margin(had.contains(&cell));
              if !viewer.frustum.reaches_sides(&b, margin) || viewer.depth_of(&b) > margin {
                continue;
              }
              let state = grid.cells.entry(cell).or_insert_with(|| {
                *pending += 1;
                Wanted::new()
              });
              if !state.wanted_by.contains(&v) {
                state.wanted_by.push(v);
              }
            }
          }
        }
      }
      grid.cells.retain(|&cell, state| {
        if !state.wanted_by.is_empty() {
          return true;
        }
        if state.resident {
          *leaves_total += 1;
          events.push(RegionEvent::Leave(RegionRef::Cell { grid: gid, cell }));
        } else {
          *pending -= 1;
          if state.stalled {
            *stalled -= 1;
          }
        }
        false
      });
    }
  }

  /// Deliver the pending enters most needed first (the highest priority,
  /// then the nearest to a wanting viewer's eye in the region's own
  /// `leave`s), as many as the budget still allows since the last
  /// delivery. Under a limit a region enters only while the limits hold;
  /// past one, the resident regions less needed than the candidate
  /// leave, least needed first, until it fits (the leave precedes the
  /// enter, so a handler's pool hands the freed resources straight
  /// over), and a candidate nothing can make room for is STALLED until a
  /// view moves. A grid's own limit weighs its cells; the scene limit
  /// weighs every region.
  fn pace_enters(&mut self, nodes: &mut [Node], region_nodes: &[u32]) {
    if self.pending == 0 || self.enters_since_take >= self.budget {
      // Nothing to pace: what the last pacing stalled stays stalled (the
      // flags and the count agree) until a pacing recounts.
      return;
    }
    self.stalled = 0;
    let limited = self.limit.is_some() || self.grids.values().any(|g| g.spec.limit.is_some());
    let viewers = &self.viewers;
    let mut candidates: Vec<(Need, Which)> = Vec::with_capacity(self.pending);
    // The resident regions: what the eviction scan takes from. Built only
    // under a limit.
    let mut residents: Vec<Resident> = Vec::new();
    let mut scene_weight: f64 = 0.0;
    for &i in region_nodes {
      let node = &mut nodes[i as usize];
      let center = node.world_box.map(center_of).unwrap_or([node.world[12], node.world[13], node.world[14]]);
      let region = node.region.as_mut().expect("region nodes carry a region");
      region.wanted.stalled = false;
      if region.wanted.wanted_by.is_empty() {
        continue;
      }
      let urgency = region.spec.margins.urgency(nearest(viewers, &region.wanted.wanted_by, |_| center));
      let need = Need { priority: region.spec.priority, urgency };
      if region.wanted.resident {
        scene_weight += region.weight();
        if limited {
          residents.push(Resident { need, weight: region.weight(), which: Which::Node(i), evicted: false });
        }
      } else {
        candidates.push((need, Which::Node(i)));
      }
    }
    for (&gid, grid) in self.grids.iter_mut() {
      let spec = &grid.spec;
      for (&cell, state) in grid.cells.iter_mut() {
        state.stalled = false;
        let urgency = spec.margins.urgency(nearest(viewers, &state.wanted_by, |eye| spec.cell_center(cell, eye)));
        let need = Need { priority: spec.priority, urgency };
        if state.resident {
          let weight = state.measured.unwrap_or(spec.weight);
          scene_weight += weight;
          if limited {
            residents.push(Resident { need, weight, which: Which::Cell(gid, cell), evicted: false });
          }
        } else {
          candidates.push((need, Which::Cell(gid, cell)));
        }
      }
    }
    candidates.sort_by(|a, b| a.0.cmp_most_first(&b.0));
    // Least needed first.
    residents.sort_by(|a, b| b.need.cmp_most_first(&a.need));
    for (need, which) in candidates {
      if self.enters_since_take >= self.budget {
        break;
      }
      if limited && !self.make_room(which, need, nodes, &mut residents, &mut scene_weight) {
        self.stalled += 1;
        match which {
          Which::Node(i) => {
            nodes[i as usize].region.as_mut().expect("a candidate's region exists").wanted.stalled = true
          }
          Which::Cell(gid, cell) => {
            self
              .grids
              .get_mut(&gid)
              .expect("a candidate's grid exists")
              .cells
              .get_mut(&cell)
              .expect("a candidate cell exists")
              .stalled = true
          }
        }
        continue;
      }
      let region = match which {
        Which::Node(i) => {
          let node = &mut nodes[i as usize];
          let id = node_id(i, node);
          let region = node.region.as_mut().expect("a candidate's region exists");
          region.wanted.resident = true;
          scene_weight += region.weight();
          RegionRef::Node(id)
        }
        Which::Cell(gid, cell) => {
          let grid = self.grids.get_mut(&gid).expect("a candidate's grid exists");
          let state = grid.cells.get_mut(&cell).expect("a candidate cell exists");
          state.resident = true;
          scene_weight += state.measured.unwrap_or(grid.spec.weight);
          RegionRef::Cell { grid: gid, cell }
        }
      };
      self.pending -= 1;
      self.enters_since_take += 1;
      self.enters_total += 1;
      self.events.push(RegionEvent::Enter(region));
    }
  }

  /// The weight a region would enter with, and its grid for a cell.
  fn weight_of(&self, which: Which, nodes: &[Node]) -> (f64, Option<GridId>) {
    match which {
      Which::Node(i) => (nodes[i as usize].region.as_ref().expect("a candidate's region exists").weight(), None),
      Which::Cell(gid, cell) => {
        let grid = &self.grids[&gid];
        (grid.cell_weight(&grid.cells[&cell]), Some(gid))
      }
    }
  }

  /// Make the limits hold for one more region (`which`, needed as `need`),
  /// evicting resident regions less needed than it: the grid's own cells
  /// for its limit, any weighing region's for the scene's. Returns
  /// whether the region fits. The victims are the least needed that
  /// together free what is needed, minus any that turn out unnecessary
  /// once the set is complete (dropped most needed first, so a heavy far
  /// region goes and a light nearer one stays), and nothing is evicted
  /// when the evictable weight cannot cover the need: an eviction is a
  /// leave of a region still wanted, which goes back to pending, so an
  /// eviction that does not admit the candidate would only churn. An
  /// evicted region lighter than the slack its eviction left re-enters
  /// at the next pass and then stays, since it fits without an eviction.
  fn make_room(
    &mut self,
    which: Which,
    need: Need,
    nodes: &mut [Node],
    residents: &mut [Resident],
    scene_weight: &mut f64,
  ) -> bool {
    let (weight, grid_id) = self.weight_of(which, nodes);
    if weight == 0.0 {
      return true;
    }
    let grid_need = grid_id
      .and_then(|g| self.grids[&g].spec.limit.map(|l| self.grids[&g].resident_weight() + weight - l))
      .unwrap_or(0.0);
    let scene_need = self.limit.map_or(0.0, |l| *scene_weight + weight - l);
    if grid_need <= 0.0 && scene_need <= 0.0 {
      return true;
    }
    let same_grid = |r: &Resident| matches!((r.which, grid_id), (Which::Cell(g, _), Some(gid)) if g == gid);
    // Least needed first (the list is sorted so), taking only victims that
    // serve a need still open.
    let mut picked: Vec<usize> = Vec::new();
    let mut grid_freed = 0.0;
    let mut scene_freed = 0.0;
    for (i, r) in residents.iter().enumerate() {
      if grid_freed >= grid_need && scene_freed >= scene_need {
        break;
      }
      if r.evicted || !r.need.less_than(&need) {
        continue;
      }
      let serves_grid = same_grid(r) && grid_freed < grid_need;
      let serves_scene = scene_freed < scene_need;
      if r.weight == 0.0 || !(serves_grid || serves_scene) {
        continue;
      }
      picked.push(i);
      scene_freed += r.weight;
      if same_grid(r) {
        grid_freed += r.weight;
      }
    }
    if grid_freed < grid_need || scene_freed < scene_need {
      return false;
    }
    // Drop what the set does not need, most needed (last picked) first.
    for k in (0..picked.len()).rev() {
      let r = &residents[picked[k]];
      let same = same_grid(r);
      if scene_freed - r.weight >= scene_need && (!same || grid_freed - r.weight >= grid_need) {
        scene_freed -= r.weight;
        if same {
          grid_freed -= r.weight;
        }
        picked[k] = usize::MAX;
      }
    }
    for &i in &picked {
      if i == usize::MAX {
        continue;
      }
      let victim = &mut residents[i];
      victim.evicted = true;
      let region = match victim.which {
        Which::Node(n) => {
          let node = &mut nodes[n as usize];
          let id = node_id(n, node);
          node.region.as_mut().expect("a resident's region exists").wanted.resident = false;
          RegionRef::Node(id)
        }
        Which::Cell(g, c) => {
          let grid = self.grids.get_mut(&g).expect("a resident's grid exists");
          grid.cells.get_mut(&c).expect("a resident cell exists").resident = false;
          RegionRef::Cell { grid: g, cell: c }
        }
      };
      *scene_weight -= victim.weight;
      self.pending += 1;
      self.leaves_total += 1;
      self.events.push(RegionEvent::Leave(region));
    }
    true
  }
}

/// The distance from the nearest of these viewers' eyes to the point
/// `at` gives for that eye.
fn nearest(viewers: &HashMap<u64, Viewer>, wanted_by: &[u64], at: impl Fn([f32; 3]) -> [f32; 3]) -> f32 {
  let mut best = f32::INFINITY;
  for v in wanted_by {
    if let Some(viewer) = viewers.get(v) {
      let p = at(viewer.eye);
      let d = [p[0] - viewer.eye[0], p[1] - viewer.eye[1], p[2] - viewer.eye[2]];
      best = best.min((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
    }
  }
  best
}

impl Viewer {
  /// The forward depth, from the eye, of the box's nearest point.
  fn depth_of(&self, b: &Box3) -> f32 {
    let f = self.forward;
    let c = center_of(*b);
    let e = [(b[3] - b[0]) / 2.0, (b[4] - b[1]) / 2.0, (b[5] - b[2]) / 2.0];
    let center_depth = (c[0] - self.eye[0]) * f[0] + (c[1] - self.eye[1]) * f[1] + (c[2] - self.eye[2]) * f[2];
    center_depth - (f[0].abs() * e[0] + f[1].abs() * e[1] + f[2].abs() * e[2])
  }

  /// The world box enclosing the view volume cut off at `depth` along the
  /// view direction and grown by `depth` on every side: what a grid
  /// enumerates cells under (every cell within `depth` of the volume lies
  /// inside it). None for a view-projection with no inverse.
  fn reach(&self, depth: f32) -> Option<Box3> {
    let inv = invert(&self.view_proj)?;
    let f = self.forward;
    let depth_of =
      |p: [f32; 3]| (p[0] - self.eye[0]) * f[0] + (p[1] - self.eye[1]) * f[1] + (p[2] - self.eye[2]) * f[2];
    let mut lo = self.eye;
    let mut hi = self.eye;
    let mut grow = |p: [f32; 3]| {
      for a in 0..3 {
        lo[a] = lo[a].min(p[a]);
        hi[a] = hi[a].max(p[a]);
      }
    };
    for &x in &[-1.0f32, 1.0] {
      for &y in &[-1.0f32, 1.0] {
        let near = unproject(&inv, [x, y, -1.0]);
        let Unprojected::Point(n) = near else {
          return None;
        };
        grow(n);
        let dn = depth_of(n);
        // The far corner, pulled in to `depth` when beyond it: the cut is
        // linear along the near-to-far edge (and along the ray of an
        // infinite projection's direction).
        let far = match unproject(&inv, [x, y, 1.0]) {
          Unprojected::Point(p) => {
            let df = depth_of(p);
            if df <= depth || df <= dn {
              p
            } else if depth <= dn {
              n
            } else {
              let t = (depth - dn) / (df - dn);
              [n[0] + (p[0] - n[0]) * t, n[1] + (p[1] - n[1]) * t, n[2] + (p[2] - n[2]) * t]
            }
          }
          Unprojected::Direction(d) => {
            let mut d = d;
            let mut along = d[0] * f[0] + d[1] * f[1] + d[2] * f[2];
            if along < 0.0 {
              d = [-d[0], -d[1], -d[2]];
              along = -along;
            }
            if along <= 0.0 || depth <= dn {
              n
            } else {
              let s = (depth - dn) / along;
              [n[0] + d[0] * s, n[1] + d[1] * s, n[2] + d[2] * s]
            }
          }
        };
        grow(far);
      }
    }
    let b = [lo[0] - depth, lo[1] - depth, lo[2] - depth, hi[0] + depth, hi[1] + depth, hi[2] + depth];
    b.iter().all(|v| v.is_finite()).then_some(b)
  }
}

enum Unprojected {
  Point([f32; 3]),
  /// A clip corner at infinity (an infinite far plane): the ray direction.
  Direction([f32; 3]),
}

/// A clip-space point back through the inverse view-projection.
fn unproject(inv: &Mat4, c: [f32; 3]) -> Unprojected {
  let x = inv[0] * c[0] + inv[4] * c[1] + inv[8] * c[2] + inv[12];
  let y = inv[1] * c[0] + inv[5] * c[1] + inv[9] * c[2] + inv[13];
  let z = inv[2] * c[0] + inv[6] * c[1] + inv[10] * c[2] + inv[14];
  let w = inv[3] * c[0] + inv[7] * c[1] + inv[11] * c[2] + inv[15];
  if w.abs() < INFINITE_FAR_W {
    Unprojected::Direction([x, y, z])
  } else {
    Unprojected::Point([x / w, y / w, z / w])
  }
}

fn center_of(b: Box3) -> [f32; 3] {
  [(b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0]
}

fn node_id(i: u32, node: &Node) -> NodeId {
  ((node.generation as u64) << 32) | i as u64
}
