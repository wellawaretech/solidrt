// The residency gate: which regions of a world should EXIST right now,
// given every viewer's view volume. Culling and LOD gate drawing; this
// gates memory, and is the same primitive for a 2d tile world's chunks and
// a 3d open world's regions. Two region forms: an EXPLICIT region is a
// bounded node (its world box, as the cull test carries it), a GRID is a
// lattice of cells that exist only while approached. A region is wanted
// by a viewer while within `enter` of its view volume and stays wanted
// until beyond `leave` (two-radius hysteresis), and is wanted overall if
// ANY viewer wants it. Changes are reported, never polled: leaves at
// once, enters paced nearest-eye-first under one scene-wide budget. The
// gate knows wanted or not; what a region holds is its handler's.

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
}

/// A grid of cells: `origin` the corner of cell (0, 0, 0), `cell` the
/// size per axis with 0 meaning unbounded along that axis (the index is 0
/// there and the test ignores the axis), `bounds` an optional inclusive
/// cell-index box outside which no cell exists, `wrap` an optional period
/// in cells per axis (0 = none: indices modulo the period, the canonical
/// cell reported once however many images a viewer sees), `label` the name
/// the stats group the grid's footprint under.
#[derive(Clone, Debug, PartialEq)]
pub struct GridSpec {
  pub origin: [f32; 3],
  pub cell: [f32; 3],
  pub margins: RegionMargins,
  pub bounds: Option<[i32; 6]>,
  pub wrap: Option<[i32; 3]>,
  pub label: Option<String>,
}

impl GridSpec {
  fn validate(&self) -> Result<(), String> {
    self.margins.validate()?;
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

/// An explicit region's state, on its node.
pub(super) struct Region {
  margins: RegionMargins,
  wanted_by: Vec<u64>,
  /// Its enter went out (and no leave since).
  resident: bool,
  /// Moved or declared this flush: re-test against every viewer.
  pub(super) moved: bool,
}

struct Cell {
  wanted_by: Vec<u64>,
  resident: bool,
}

struct Grid {
  spec: GridSpec,
  /// Only the wanted cells: a cell nobody wants has no state at all.
  cells: HashMap<[i32; 3], Cell>,
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
      events: Vec::new(),
    }
  }
}

/// The index of a candidate for a paced enter: the node's arena index, or
/// the grid and its canonical cell.
enum Candidate {
  Node(u32),
  Cell(GridId, [i32; 3]),
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

  pub(super) fn create_grid(&mut self, spec: GridSpec) -> Result<GridId, String> {
    spec.validate()?;
    let id = self.next_grid;
    self.next_grid += 1;
    self.grids.insert(id, Grid { spec, cells: HashMap::new() });
    // Every viewer enumerates the new grid at the next pass.
    for &v in self.viewers.keys() {
      if !self.dirty.contains(&v) {
        self.dirty.push(v);
      }
    }
    Ok(id)
  }

  /// End a grid: every resident cell leaves, every pending one is dropped.
  pub(super) fn destroy_grid(&mut self, id: GridId) -> Result<(), String> {
    let grid = self.grids.remove(&id).ok_or_else(|| format!("grid {id} not found"))?;
    for (cell, state) in grid.cells {
      if state.resident {
        self.events.push(RegionEvent::Leave(RegionRef::Cell { grid: id, cell }));
      } else {
        self.pending -= 1;
      }
    }
    Ok(())
  }

  /// Declare, re-declare or end (None) the region on node `i` (which has a
  /// box; the caller checked). A re-declaration keeps what viewers want.
  pub(super) fn set_region(&mut self, i: u32, node: &mut Node, margins: Option<RegionMargins>) -> Result<bool, String> {
    match margins {
      Some(m) => {
        m.validate()?;
        match node.region.as_mut() {
          Some(r) => r.margins = m,
          None => {
            node.region = Some(Box::new(Region { margins: m, wanted_by: Vec::new(), resident: false, moved: false }))
          }
        }
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
    if r.resident {
      self.events.push(RegionEvent::Leave(RegionRef::Node(node_id(i, node))));
    } else if !r.wanted_by.is_empty() {
      self.pending -= 1;
    }
  }

  pub(super) fn take_events(&mut self) -> Vec<RegionEvent> {
    self.enters_since_take = 0;
    std::mem::take(&mut self.events)
  }

  /// Something waits on a frame: events to deliver, or enters to pace.
  pub(super) fn demand(&self) -> bool {
    !self.events.is_empty() || self.pending > 0
  }

  /// Whether the pass has anything to do this flush.
  pub(super) fn needs_pass(&self, regions_moved: bool) -> bool {
    regions_moved || !self.dirty.is_empty() || self.pending > 0
  }

  /// The pass, after the flush's walk (world boxes fresh) and the cull
  /// pass: re-test every explicit region against the viewers that moved
  /// (every viewer, for a region that moved), re-enumerate every grid
  /// under the viewers that moved, then pace the pending enters nearest
  /// first. `region_nodes` is the list of nodes carrying a region, pruned
  /// here of the dead.
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
      let was_wanted = !region.wanted_by.is_empty();
      let viewers: &[u64] = if moved { &all } else { dirty };
      for &v in viewers {
        let had = region.wanted_by.contains(&v);
        let want = match (self.viewers.get(&v), world_box) {
          (Some(viewer), Some(b)) => viewer.frustum.intersects(&b, region.margins.margin(had)),
          _ => false,
        };
        if want && !had {
          region.wanted_by.push(v);
        } else if !want && had {
          region.wanted_by.retain(|&w| w != v);
        }
      }
      // A viewer that went never tests true above, but one that went
      // without ever being dirty cannot happen: `set_view` marks it.
      let now_wanted = !region.wanted_by.is_empty();
      if was_wanted && !now_wanted {
        if region.resident {
          region.resident = false;
          self.events.push(RegionEvent::Leave(RegionRef::Node(id)));
        } else {
          self.pending -= 1;
        }
      } else if !was_wanted && now_wanted {
        self.pending += 1;
      }
    }
  }

  fn enumerate_grids(&mut self, dirty: &[u64]) {
    let Residency { viewers, grids, events, pending, .. } = self;
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
                Cell { wanted_by: Vec::new(), resident: false }
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
          events.push(RegionEvent::Leave(RegionRef::Cell { grid: gid, cell }));
        } else {
          *pending -= 1;
        }
        false
      });
    }
  }

  /// Deliver the pending enters nearest to a wanting viewer's eye first,
  /// as many as the budget still allows since the last delivery.
  fn pace_enters(&mut self, nodes: &mut [Node], region_nodes: &[u32]) {
    if self.pending == 0 || self.enters_since_take >= self.budget {
      return;
    }
    let mut candidates: Vec<(f32, Candidate)> = Vec::with_capacity(self.pending);
    for &i in region_nodes {
      let node = &nodes[i as usize];
      let region = node.region.as_ref().expect("region nodes carry a region");
      if region.resident || region.wanted_by.is_empty() {
        continue;
      }
      let center = node.world_box.map(center_of).unwrap_or([node.world[12], node.world[13], node.world[14]]);
      candidates.push((self.nearest(&region.wanted_by, |_| center), Candidate::Node(i)));
    }
    for (&gid, grid) in &self.grids {
      for (&cell, state) in &grid.cells {
        if state.resident {
          continue;
        }
        let spec = &grid.spec;
        candidates
          .push((self.nearest(&state.wanted_by, |eye| spec.cell_center(cell, eye)), Candidate::Cell(gid, cell)));
      }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, candidate) in candidates {
      if self.enters_since_take >= self.budget {
        break;
      }
      let region = match candidate {
        Candidate::Node(i) => {
          let node = &mut nodes[i as usize];
          let id = node_id(i, node);
          node.region.as_mut().expect("region nodes carry a region").resident = true;
          RegionRef::Node(id)
        }
        Candidate::Cell(gid, cell) => {
          let state = self.grids.get_mut(&gid).and_then(|g| g.cells.get_mut(&cell)).expect("a candidate cell exists");
          state.resident = true;
          RegionRef::Cell { grid: gid, cell }
        }
      };
      self.pending -= 1;
      self.enters_since_take += 1;
      self.events.push(RegionEvent::Enter(region));
    }
  }

  /// The distance from the nearest of these viewers' eyes to the point
  /// `at` gives for that eye.
  fn nearest(&self, wanted_by: &[u64], at: impl Fn([f32; 3]) -> [f32; 3]) -> f32 {
    let mut best = f32::INFINITY;
    for v in wanted_by {
      if let Some(viewer) = self.viewers.get(v) {
        let p = at(viewer.eye);
        let d = [p[0] - viewer.eye[0], p[1] - viewer.eye[1], p[2] - viewer.eye[2]];
        best = best.min((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
      }
    }
    best
  }
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
