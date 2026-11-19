//! A path's meshes: its geometry tessellated by lyon into triangles, a fill
//! under the path's rule or a stroke under the paint's metrics, at a
//! tolerance set by the scale it draws at, and cached on the path (up to
//! `MESH_CACHE_ENTRIES`), so a retained list redraws from the cache. Pure,
//! no GL: the walk reads the meshes (`walk::push_path`), the tests read
//! them with no context (`tests/mesh.rs`).

use super::path::Path;
use crate::{FillType, StrokeCap, StrokeJoin};
use euclid::default::{Box2D, Point2D, Transform2D};
use lyon_tessellation::{
  BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
  StrokeTessellator, StrokeVertex, TessellationError, VertexBuffers,
};
use std::sync::Arc;

/// The flattening tolerance of a path mesh in device pixels: how far the
/// flattened curve may stray from the true one. The dash walker's figure;
/// under 4x MSAA a quarter pixel does not show. A tuning value, to be
/// re-read on the TV against the vertex counts it produces.
pub const MESH_TOLERANCE: f32 = 0.25;
/// Meshes a path keeps: a fill and a stroke at one or two scale buckets
/// cover an element that animates its stroke or zooms; beyond it the
/// oldest goes.
pub const MESH_CACHE_ENTRIES: usize = 4;

/// The smallest miter limit a stroke tessellates with, lyon's and SVG's
/// minimum (a miter is never shorter than the stroke is wide); a smaller
/// limit draws as this one.
pub const MINIMUM_MITER: f32 = StrokeOptions::MINIMUM_MITER_LIMIT;

/// What a mesh is tessellated for, and half of its cache key. A stroke's
/// `miter` is the miter limit, the ratio of miter length to stroke width
/// (lyon's, SVG's and the contract's definition).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeshStyle {
  Fill,
  Stroke { width: f32, cap: StrokeCap, join: StrokeJoin, miter: f32 },
}

/// A path tessellated into triangles.
#[derive(Debug)]
pub struct Mesh {
  /// Positions in the path's space.
  pub vertices: Vec<[f32; 2]>,
  pub indices: Vec<u32>,
  /// The vertices' box: the exact extent the mesh covers. A zero box for a
  /// mesh with no triangle.
  pub bounds: Box2D<f32>,
}

/// The cache key: the style and the scale bucket. The style's floats
/// compare with `==`: a NaN width never reaches here, since the record
/// skips a stroke whose width is not above zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MeshKey {
  pub style: MeshStyle,
  pub bucket: i32,
}

/// The scale bucket of a draw under the 2D map `m`: the larger axis
/// stretch, `max(|row 1|, |row 2|)`, rounded up to a power of two and
/// given as its exponent. A bucket covers the scales up to 2^bucket, so a
/// mesh tessellated for it (`bucket_tolerance`) is never coarser than
/// `MESH_TOLERANCE` in device pixels within it, and a smooth zoom
/// retessellates only where it crosses a power of two. None when the
/// stretch is zero or not finite: such a draw covers nothing.
pub fn scale_bucket(m: &Transform2D<f32>) -> Option<i32> {
  let (a, b) = (m.m11.hypot(m.m12), m.m21.hypot(m.m22));
  if !a.is_finite() || !b.is_finite() {
    return None;
  }
  let s = a.max(b);
  if s == 0.0 {
    return None;
  }
  Some(s.log2().ceil() as i32)
}

/// The tolerance a mesh of `bucket` is tessellated at, in the path's
/// space: `MESH_TOLERANCE` device pixels at the bucket's largest scale.
pub fn bucket_tolerance(bucket: i32) -> f32 {
  MESH_TOLERANCE / 2f32.powi(bucket)
}

/// `path` tessellated under `style` at `tolerance` (in the path's space):
/// a fill with lyon's fill tessellator under `fill`, a stroke with its
/// stroke tessellator under the style's width, cap, join and miter limit
/// (a limit below `MINIMUM_MITER` is raised to it). A path that covers
/// nothing is a mesh with empty buffers and a zero box.
pub(crate) fn tessellate(
  path: &lyon_path::Path,
  fill: FillType,
  style: MeshStyle,
  tolerance: f32,
) -> Result<Mesh, TessellationError> {
  let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
  match style {
    MeshStyle::Fill => {
      let rule = match fill {
        FillType::NonZero => FillRule::NonZero,
        FillType::EvenOdd => FillRule::EvenOdd,
      };
      let options = FillOptions::tolerance(tolerance).with_fill_rule(rule);
      let mut builder = BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array());
      FillTessellator::new().tessellate_path(path, &options, &mut builder)?;
    }
    MeshStyle::Stroke { width, cap, join, miter } => {
      let options = StrokeOptions::tolerance(tolerance)
        .with_line_width(width)
        .with_line_cap(line_cap(cap))
        .with_line_join(line_join(join))
        // lyon asserts its minimum; a NaN limit takes it too.
        .with_miter_limit(miter.max(MINIMUM_MITER));
      let mut builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position().to_array());
      StrokeTessellator::new().tessellate_path(path, &options, &mut builder)?;
    }
  }
  let bounds = if buffers.indices.is_empty() {
    Box2D::zero()
  } else {
    Box2D::from_points(buffers.vertices.iter().map(|v| Point2D::new(v[0], v[1])))
  };
  Ok(Mesh { vertices: buffers.vertices, indices: buffers.indices, bounds })
}

fn line_cap(cap: StrokeCap) -> LineCap {
  match cap {
    StrokeCap::Butt => LineCap::Butt,
    StrokeCap::Round => LineCap::Round,
    StrokeCap::Square => LineCap::Square,
  }
}

// A miter join past the limit is beveled, as SVG's is: lyon's `Miter`, not
// its `MiterClip`.
fn line_join(join: StrokeJoin) -> LineJoin {
  match join {
    StrokeJoin::Miter => LineJoin::Miter,
    StrokeJoin::Round => LineJoin::Round,
    StrokeJoin::Bevel => LineJoin::Bevel,
  }
}

impl Path {
  /// The mesh of this path under `style` at the scale `bucket`, and
  /// whether it was tessellated by this call (a cache miss): the cached
  /// one for the key, or a new one, stored with the oldest entry dropped
  /// past `MESH_CACHE_ENTRIES`. The mesh is None for one with no triangle
  /// (cached too, so it is not tessellated again) and for a path lyon's
  /// tessellator rejects (warned once, and cached as such a mesh); the
  /// miss flag still says the tessellator ran.
  pub(crate) fn mesh(&self, style: MeshStyle, bucket: i32) -> (Option<Arc<Mesh>>, bool) {
    let key = MeshKey { style, bucket };
    // A poisoned lock only means a panic elsewhere while it was held; the
    // vector of meshes is still whole, so the lookup goes on.
    let mut meshes = self.meshes().lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, mesh)) = meshes.iter().find(|(k, _)| *k == key) {
      return ((!mesh.indices.is_empty()).then(|| mesh.clone()), false);
    }
    let mesh = match tessellate(self.geometry(), self.fill_type(), style, bucket_tolerance(bucket)) {
      Ok(mesh) => mesh,
      Err(err) => {
        warn_once!("[anvil] Renderer: a path lyon's tessellator rejected is not drawn ({err:?})");
        Mesh { vertices: Vec::new(), indices: Vec::new(), bounds: Box2D::zero() }
      }
    };
    let mesh = Arc::new(mesh);
    if meshes.len() >= MESH_CACHE_ENTRIES {
      meshes.remove(0);
    }
    meshes.push((key, mesh.clone()));
    ((!mesh.indices.is_empty()).then_some(mesh), true)
  }
}
