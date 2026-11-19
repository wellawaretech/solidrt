//! A path as anvil draws it: lyon's geometry, its fill rule, its
//! control-point bounds and the meshes tessellated from it so far
//! (`super::mesh`), shared behind an Arc so the element that keeps a path
//! and every recording that draws it hold the one cache. `PathBuilder`
//! builds one from the contract's pen walk.

use super::mesh::{Mesh, MeshKey};
use crate::contract;
#[cfg(any(test, feature = "oplog"))]
use crate::oplog::{PathLog, PathSegment};
use crate::{FillType, Point};
use euclid::default::Box2D;
use lyon_path::Event;
use std::sync::{Arc, Mutex};

/// A finished path: its geometry (lyon's), its fill rule, and the meshes
/// drawn from it, which a clone shares. `Send + Sync`, as a list that
/// holds it crosses threads: lyon's path is, and a mutex guards the mesh
/// cache.
#[derive(Clone)]
pub struct Path(Arc<PathInner>);

struct PathInner {
  geometry: lyon_path::Path,
  fill: FillType,
  /// The box of every point the path names (its starts, ends and control
  /// points), None for an empty path: a conservative extent for culling
  /// before a mesh exists.
  bounds: Option<Box2D<f32>>,
  /// The meshes tessellated from this path so far (mesh.rs).
  meshes: Mutex<Vec<(MeshKey, Arc<Mesh>)>>,
  // A path is cloned and kept across frames: the log is shared.
  #[cfg(any(test, feature = "oplog"))]
  log: Arc<PathLog>,
}

impl Path {
  /// The geometry the path was built from, for a caller that measures,
  /// hit-tests or walks it (the rendertree does all three). An inherent
  /// method of anvil's path, not a contract one.
  pub fn geometry(&self) -> &lyon_path::Path {
    &self.0.geometry
  }

  /// The fill rule the path was built with.
  pub fn fill_type(&self) -> FillType {
    self.0.fill
  }

  /// The control-point box (see `PathInner::bounds`).
  pub(crate) fn bounds(&self) -> Option<Box2D<f32>> {
    self.0.bounds
  }

  /// The mesh cache, for `mesh.rs`.
  pub(super) fn meshes(&self) -> &Mutex<Vec<(MeshKey, Arc<Mesh>)>> {
    &self.0.meshes
  }

  /// The segments as the pen walk drew them, for the op log.
  #[cfg(any(test, feature = "oplog"))]
  pub(super) fn log(&self) -> &Arc<PathLog> {
    &self.0.log
  }
}

impl contract::Path for Path {}

/// Builds a `Path` over lyon's builder, tolerating the contract's walks:
/// a segment with no subpath open begins one at the current point (lyon's
/// builder would panic), and a `close` with nothing open is nothing.
pub struct PathBuilder {
  builder: lyon_path::path::Builder,
  /// A subpath is open: lyon's `begin` was called and not yet ended.
  open: bool,
  /// Where the next segment starts: the last segment's end, the origin
  /// before any.
  current: Point,
  /// The open subpath's start, where a `close` returns to.
  start: Point,
  #[cfg(any(test, feature = "oplog"))]
  segments: Vec<PathSegment>,
}

impl Default for PathBuilder {
  fn default() -> Self {
    PathBuilder {
      builder: lyon_path::Path::builder(),
      open: false,
      current: Point::origin(),
      start: Point::origin(),
      #[cfg(any(test, feature = "oplog"))]
      segments: Vec::new(),
    }
  }
}

impl PathBuilder {
  // Begin a subpath at the current point unless one is open, so a segment
  // always lands inside lyon's begin and end.
  fn ensure_open(&mut self) {
    if !self.open {
      self.builder.begin(self.current);
      self.open = true;
      self.start = self.current;
    }
  }
}

impl contract::Pen for PathBuilder {
  fn move_to(&mut self, p: Point) -> &mut Self {
    if self.open {
      self.builder.end(false);
    }
    self.builder.begin(p);
    self.open = true;
    self.current = p;
    self.start = p;
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::MoveTo(p));
    self
  }

  fn line_to(&mut self, p: Point) -> &mut Self {
    self.ensure_open();
    self.builder.line_to(p);
    self.current = p;
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::LineTo(p));
    self
  }

  fn quadratic_to(&mut self, ctrl: Point, p: Point) -> &mut Self {
    self.ensure_open();
    self.builder.quadratic_bezier_to(ctrl, p);
    self.current = p;
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::QuadraticTo { ctrl, to: p });
    self
  }

  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point) -> &mut Self {
    self.ensure_open();
    self.builder.cubic_bezier_to(ctrl1, ctrl2, p);
    self.current = p;
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::CubicTo { ctrl1, ctrl2, to: p });
    self
  }
}

impl contract::PathBuilder for PathBuilder {
  fn close(&mut self) -> &mut Self {
    if self.open {
      self.builder.close();
      self.open = false;
      self.current = self.start;
    }
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::Close);
    self
  }

  fn build(mut self, fill: FillType) -> Path {
    if self.open {
      self.builder.end(false);
    }
    let geometry = self.builder.build();
    let bounds = control_bounds(&geometry);
    Path(Arc::new(PathInner {
      geometry,
      fill,
      bounds,
      meshes: Mutex::new(Vec::new()),
      #[cfg(any(test, feature = "oplog"))]
      log: Arc::new(PathLog { segments: self.segments, fill }),
    }))
  }
}

// The box of every point `path`'s events name: the starts, the segment
// ends and the control points. None for an empty path.
fn control_bounds(path: &lyon_path::Path) -> Option<Box2D<f32>> {
  let mut points = Vec::new();
  for event in path.iter() {
    match event {
      Event::Begin { at } => points.push(at),
      Event::Line { from, to } => points.extend([from, to]),
      Event::Quadratic { from, ctrl, to } => points.extend([from, ctrl, to]),
      Event::Cubic { from, ctrl1, ctrl2, to } => points.extend([from, ctrl1, ctrl2, to]),
      // Its points are the subpath's own, already counted.
      Event::End { .. } => {}
    }
  }
  (!points.is_empty()).then(|| Box2D::from_points(points))
}
