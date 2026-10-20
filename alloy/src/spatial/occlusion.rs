// Occlusion culling's geometry: authored occluder shapes rasterized to a
// small CPU depth buffer per target, and the test of a world box against
// it. Nothing here knows what a camera or a node is: a view-projection
// and node-space triangles under world matrices come in, a buffer comes
// out, and a box is either wholly behind what the buffer holds over its
// screen rectangle or it is not. Godot's OccluderInstance3D model (a
// low-resolution software depth buffer, every instance's AABB tested
// against it); Unity's occludee semantics.
//
// Every step errs toward drawing, never toward dropping, down to the
// buffer's texel: a texel takes an occluder's depth where the triangle
// covers its center (so two triangles sharing an edge leave no seam), at
// the farthest depth the triangle reaches inside the texel (its center
// depth plus the slope over half a texel); the nearest surface wins per
// texel and an uncovered texel is infinitely far; a box tests its
// NEAREST depth, grown by its cull margin, against the FARTHEST buffer
// depth over its rectangle, rounded outward to whole texels and grown by
// one more on every side, which reaches past any silhouette texel the
// center test over-covered; a box with a corner on or before the near
// plane is never hidden. What the grid cannot resolve at all is lost: a
// crack narrower than a texel between two occluders occludes as if
// closed. Depth is GL clip z over w, the value a depth buffer
// interpolates, linear across the screen under any projection, so
// interpolating it per texel is exact.

use super::bvh::Box3;
use super::Mat4;

/// The buffer's width in texels; the height follows the target's aspect.
/// Godot's default is the viewport at 1/8 (240 wide at 1080p). The
/// occlusion probe on the tablet (SM-T500) culled the same share of a
/// 1600-solid field at 128 as at 256 while the build fell from 2 to 2.7
/// ms to 0.6 to 0.9 ms per camera-move frame, so 128 it is: a texel is
/// under one percent of the view, and the one-texel test margin is what
/// a silhouette costs.
pub const OCCLUSION_BUFFER_WIDTH: usize = 128;
/// The height's clamp: a degenerate aspect (a strip target) still gets a
/// buffer that can hold a wall, and never a buffer taller than wide ones
/// pay for.
const OCCLUSION_BUFFER_MIN_HEIGHT: usize = 16;
const OCCLUSION_BUFFER_MAX_HEIGHT: usize = 1024;
/// The most occluder triangles one target rasterizes per build; the
/// occluders past it are skipped for that build (less culling, never a
/// wrong cull). A bound on the CPU the flush spends however many meshes
/// an app marks.
pub const OCCLUDER_TRIANGLE_BUDGET: usize = 4096;
/// Depth slack, NDC units: an occludee nearer than the buffer by less
/// than this counts as touching the occluder surface and stays drawn.
/// Covers the float error of a projection and of the raster's
/// interpolation, orders above f64 epsilon over the -1..1 range.
const DEPTH_EPS: f64 = 1e-5;
/// Clip w at or below which a vertex sits on the eye plane: the division
/// is skipped and the polygon dropped (it degenerates at the eye).
const CLIP_W_EPS: f64 = 1e-9;
/// A triangle whose doubled screen area is below this covers no whole
/// texel and is skipped.
const DEGENERATE_AREA: f64 = 1e-6;
/// The icosphere a sphere occluder is inscribed as: one subdivision of
/// the icosahedron, 80 faces, every vertex on the sphere so every face
/// lies inside it.
const SPHERE_SUBDIVISIONS: u32 = 1;

/// An occluder's triangles in node space.
#[derive(Clone, Debug, PartialEq)]
pub struct Occluder {
  positions: Vec<[f64; 3]>,
  indices: Vec<[u32; 3]>,
}

impl Occluder {
  /// A solid box (Godot's BoxOccluder3D): twelve triangles over its
  /// faces. Assumes the shape it stands for is solid.
  pub fn box3(b: Box3) -> Occluder {
    let positions = (0..8)
      .map(|k| {
        [
          if k & 1 == 0 { b[0] } else { b[3] },
          if k & 2 == 0 { b[1] } else { b[4] },
          if k & 4 == 0 { b[2] } else { b[5] },
        ]
      })
      .collect();
    let faces: [[u32; 4]; 6] = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let mut indices = Vec::with_capacity(12);
    for f in faces {
      indices.push([f[0], f[1], f[2]]);
      indices.push([f[0], f[2], f[3]]);
    }
    Occluder { positions, indices }
  }

  /// A quad of `width` by `height` in the node's XY plane, centered at its
  /// origin (Godot's QuadOccluder3D); two-sided.
  pub fn quad(width: f64, height: f64) -> Occluder {
    let (hw, hh) = (width / 2.0, height / 2.0);
    Occluder {
      positions: vec![[-hw, -hh, 0.0], [hw, -hh, 0.0], [hw, hh, 0.0], [-hw, hh, 0.0]],
      indices: vec![[0, 1, 2], [0, 2, 3]],
    }
  }

  /// A solid sphere (Godot's SphereOccluder3D) as an inscribed icosphere:
  /// its faces lie inside the sphere, so they are never nearer than the
  /// surface they stand for.
  pub fn sphere(center: [f64; 3], radius: f64) -> Occluder {
    let t = (1.0 + 5.0f64.sqrt()) / 2.0;
    let mut positions: Vec<[f64; 3]> = vec![
      [-1.0, t, 0.0],
      [1.0, t, 0.0],
      [-1.0, -t, 0.0],
      [1.0, -t, 0.0],
      [0.0, -1.0, t],
      [0.0, 1.0, t],
      [0.0, -1.0, -t],
      [0.0, 1.0, -t],
      [t, 0.0, -1.0],
      [t, 0.0, 1.0],
      [-t, 0.0, -1.0],
      [-t, 0.0, 1.0],
    ];
    let mut indices: Vec<[u32; 3]> = vec![
      [0, 11, 5],
      [0, 5, 1],
      [0, 1, 7],
      [0, 7, 10],
      [0, 10, 11],
      [1, 5, 9],
      [5, 11, 4],
      [11, 10, 2],
      [10, 7, 6],
      [7, 1, 8],
      [3, 9, 4],
      [3, 4, 2],
      [3, 2, 6],
      [3, 6, 8],
      [3, 8, 9],
      [4, 9, 5],
      [2, 4, 11],
      [6, 2, 10],
      [8, 6, 7],
      [9, 8, 1],
    ];
    for _ in 0..SPHERE_SUBDIVISIONS {
      let mut next = Vec::with_capacity(indices.len() * 4);
      let mut midpoints: Vec<((u32, u32), u32)> = Vec::new();
      let mut mid = |a: u32, b: u32, positions: &mut Vec<[f64; 3]>| -> u32 {
        let key = (a.min(b), a.max(b));
        if let Some((_, m)) = midpoints.iter().find(|(k, _)| *k == key) {
          return *m;
        }
        let (pa, pb) = (positions[a as usize], positions[b as usize]);
        positions.push([(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0, (pa[2] + pb[2]) / 2.0]);
        let m = (positions.len() - 1) as u32;
        midpoints.push((key, m));
        m
      };
      for [a, b, c] in indices {
        let ab = mid(a, b, &mut positions);
        let bc = mid(b, c, &mut positions);
        let ca = mid(c, a, &mut positions);
        next.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
      }
      indices = next;
    }
    for p in positions.iter_mut() {
      let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
      *p = [center[0] + p[0] / l * radius, center[1] + p[1] / l * radius, center[2] + p[2] / l * radius];
    }
    Occluder { positions, indices }
  }

  /// Any triangle list in node space (Godot's ArrayOccluder3D): a mesh's
  /// own geometry or a simpler proxy. A triangle occludes exactly what
  /// lies behind it, so this is the one shape exact for a non-solid.
  pub fn mesh(positions: Vec<[f64; 3]>, indices: Vec<[u32; 3]>) -> Result<Occluder, String> {
    if indices.is_empty() {
      return Err("an occluder mesh needs at least one triangle".to_string());
    }
    let n = positions.len() as u32;
    for (k, tri) in indices.iter().enumerate() {
      if tri.iter().any(|&i| i >= n) {
        return Err(format!("occluder triangle {k} indexes past the {n} positions"));
      }
    }
    Ok(Occluder { positions, indices })
  }

  pub fn triangles(&self) -> usize {
    self.indices.len()
  }
}

/// A target's occluder depth buffer: level 0 at the buffer's resolution,
/// then a max pyramid down to one texel (a quadtree of maxima), so a
/// rectangle's farthest depth is exact over the rectangle and read from
/// the coarsest blocks it contains plus the finer ones along its edge.
#[derive(Clone, Debug, Default)]
pub struct DepthBuffer {
  width: usize,
  height: usize,
  levels: Vec<Vec<f64>>,
  /// Each level's texel counts.
  dims: Vec<(usize, usize)>,
  vp: Mat4,
  /// Triangles the build took (within the budget).
  triangles: usize,
  /// Of those, the triangles that reached the raster: not clipped away
  /// whole by the view volume.
  drawn: usize,
  /// The texel rectangle [x0, y0, x1, y1) the rasterized triangles wrote:
  /// a box whose rectangle misses it cannot be hidden, without a read.
  covered: [usize; 4],
}

impl DepthBuffer {
  /// Start a build for a target seen through `vp`, sized by the
  /// projection's aspect (its y over x scale, the same ratio for a
  /// perspective and an orthographic matrix).
  pub fn begin(&mut self, vp: Mat4, proj: &Mat4) {
    let aspect = if proj[0] != 0.0 { (proj[5] / proj[0]).abs() } else { 1.0 };
    let height = if aspect.is_finite() && aspect > 0.0 {
      (OCCLUSION_BUFFER_WIDTH as f64 * aspect).round() as usize
    } else {
      OCCLUSION_BUFFER_WIDTH
    };
    self.width = OCCLUSION_BUFFER_WIDTH;
    self.height = height.clamp(OCCLUSION_BUFFER_MIN_HEIGHT, OCCLUSION_BUFFER_MAX_HEIGHT);
    self.vp = vp;
    self.triangles = 0;
    self.drawn = 0;
    self.covered = [usize::MAX, usize::MAX, 0, 0];
    self.levels.clear();
    self.levels.push(vec![f64::INFINITY; self.width * self.height]);
    self.dims.clear();
    self.dims.push((self.width, self.height));
  }

  /// Rasterize an occluder under its world matrix, spending from `budget`
  /// (triangles); the triangles past it are skipped. Returns how many
  /// were drawn.
  pub fn rasterize(&mut self, world: &Mat4, occluder: &Occluder, budget: &mut usize) -> usize {
    let m = super::math::multiply(self.vp, *world);
    let clip: Vec<[f64; 4]> = occluder.positions.iter().map(|p| to_clip(&m, *p)).collect();
    let mut drawn = 0;
    for tri in &occluder.indices {
      if *budget == 0 {
        break;
      }
      *budget -= 1;
      drawn += 1;
      self.draw_triangle([clip[tri[0] as usize], clip[tri[1] as usize], clip[tri[2] as usize]]);
    }
    self.triangles += drawn;
    drawn
  }

  /// Close the build: the max pyramid over level 0.
  pub fn finish(&mut self) {
    let (mut w, mut h) = (self.width, self.height);
    while w > 1 || h > 1 {
      let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
      let prev = self.levels.last().expect("level 0 exists after begin");
      let mut next = vec![f64::NEG_INFINITY; nw * nh];
      for y in 0..nh {
        for x in 0..nw {
          let mut m = f64::NEG_INFINITY;
          for dy in 0..2 {
            for dx in 0..2 {
              let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
              m = m.max(prev[sy * w + sx]);
            }
          }
          next[y * nw + x] = m;
        }
      }
      self.levels.push(next);
      self.dims.push((nw, nh));
      w = nw;
      h = nh;
    }
  }

  /// No texel was written (nothing built, or every triangle clipped
  /// away, the occluders all behind the camera): every test would say
  /// visible, so the pass skips them.
  pub fn is_empty(&self) -> bool {
    self.covered[2] == 0
  }

  pub fn triangles(&self) -> usize {
    self.triangles
  }

  pub fn drawn(&self) -> usize {
    self.drawn
  }

  /// Whether the world box, grown by `margin` on every side, lies wholly
  /// behind the buffer's contents over its screen rectangle.
  pub fn hidden(&self, b: &Box3, margin: f64) -> bool {
    if self.levels.is_empty() || self.is_empty() {
      return false;
    }
    let g = [b[0] - margin, b[1] - margin, b[2] - margin, b[3] + margin, b[4] + margin, b[5] + margin];
    let (w, h) = (self.width as f64, self.height as f64);
    let mut rect = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut near = f64::INFINITY;
    for k in 0..8 {
      let p = [
        if k & 1 == 0 { g[0] } else { g[3] },
        if k & 2 == 0 { g[1] } else { g[4] },
        if k & 4 == 0 { g[2] } else { g[5] },
      ];
      let c = to_clip(&self.vp, p);
      if c[3] <= CLIP_W_EPS {
        return false;
      }
      let iw = 1.0 / c[3];
      let z = c[2] * iw;
      if z < -1.0 {
        // A corner on or before the near plane: the box straddles it.
        return false;
      }
      let x = (c[0] * iw * 0.5 + 0.5) * w;
      let y = (c[1] * iw * 0.5 + 0.5) * h;
      rect[0] = rect[0].min(x);
      rect[1] = rect[1].min(y);
      rect[2] = rect[2].max(x);
      rect[3] = rect[3].max(y);
      near = near.min(z);
    }
    // Outward to whole texels, then one texel further: the silhouette
    // texel a center test over-covered has a neighbor that is not.
    let x0 = (rect[0].floor() - 1.0).max(0.0) as usize;
    let y0 = (rect[1].floor() - 1.0).max(0.0) as usize;
    let x1 = ((rect[2].ceil() + 1.0).max(0.0) as usize).min(self.width);
    let y1 = ((rect[3].ceil() + 1.0).max(0.0) as usize).min(self.height);
    if x0 >= x1 || y0 >= y1 {
      return false;
    }
    let c = self.covered;
    if x1 <= c[0] || x0 >= c[2] || y1 <= c[1] || y0 >= c[3] {
      return false;
    }
    near > self.max_depth(x0, y0, x1, y1) + DEPTH_EPS
  }

  /// The farthest depth over the texel rectangle [x0, x1) x [y0, y1),
  /// exactly: the quadtree walked from the top, a block wholly inside
  /// the rectangle read at its level, one crossing its edge split.
  fn max_depth(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
    self.block_max(self.levels.len() - 1, 0, 0, [x0, y0, x1, y1])
  }

  fn block_max(&self, level: usize, bx: usize, by: usize, rect: [usize; 4]) -> f64 {
    let size = 1usize << level;
    let (px0, py0) = (bx * size, by * size);
    let (px1, py1) = (((bx + 1) * size).min(self.width), ((by + 1) * size).min(self.height));
    if px0 >= rect[2] || px1 <= rect[0] || py0 >= rect[3] || py1 <= rect[1] {
      return f64::NEG_INFINITY;
    }
    let (lw, _) = self.dims[level];
    if level == 0 || (px0 >= rect[0] && px1 <= rect[2] && py0 >= rect[1] && py1 <= rect[3]) {
      return self.levels[level][by * lw + bx];
    }
    let (cw, ch) = self.dims[level - 1];
    let mut m = f64::NEG_INFINITY;
    for dy in 0..2 {
      for dx in 0..2 {
        let (cx, cy) = (bx * 2 + dx, by * 2 + dy);
        if cx < cw && cy < ch {
          m = m.max(self.block_max(level - 1, cx, cy, rect));
        }
      }
    }
    m
  }

  /// One clip-space triangle: clipped to the view volume, divided, fanned
  /// and filled.
  fn draw_triangle(&mut self, clip: [[f64; 4]; 3]) {
    let poly = clip_polygon(&clip);
    if poly.len() < 3 {
      return;
    }
    let (w, h) = (self.width as f64, self.height as f64);
    let mut pts: Vec<[f64; 3]> = Vec::with_capacity(poly.len());
    for c in &poly {
      if c[3] <= CLIP_W_EPS {
        return;
      }
      let iw = 1.0 / c[3];
      pts.push([(c[0] * iw * 0.5 + 0.5) * w, (c[1] * iw * 0.5 + 0.5) * h, c[2] * iw]);
    }
    self.drawn += 1;
    for k in 1..pts.len() - 1 {
      self.fill(pts[0], pts[k], pts[k + 1]);
    }
  }

  /// A screen-space triangle into level 0: the texels whose center it
  /// covers, the farthest depth inside each, the nearer of the texel's
  /// value and it.
  fn fill(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area.abs() < DEGENERATE_AREA {
      return;
    }
    // Counter-clockwise, so every edge function is positive inside.
    let (b, c, area) = if area < 0.0 { (c, b, -area) } else { (b, c, area) };
    // Edge p -> q as E(x, y) = A x + B y + C, positive inside.
    let edge = |p: [f64; 3], q: [f64; 3]| {
      let aa = -(q[1] - p[1]);
      let bb = q[0] - p[0];
      [aa, bb, -(aa * p[0] + bb * p[1])]
    };
    let edges = [edge(a, b), edge(b, c), edge(c, a)];
    let dzdx = ((c[1] - a[1]) * (b[2] - a[2]) - (b[1] - a[1]) * (c[2] - a[2])) / area;
    let dzdy = ((a[0] - c[0]) * (b[2] - a[2]) + (b[0] - a[0]) * (c[2] - a[2])) / area;
    let slope = 0.5 * (dzdx.abs() + dzdy.abs());
    let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
    let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
    let x1 = (a[0].max(b[0]).max(c[0]).ceil().max(0.0) as usize).min(self.width);
    let y1 = (a[1].max(b[1]).max(c[1]).ceil().max(0.0) as usize).min(self.height);
    let width = self.width;
    let texels = &mut self.levels[0];
    for py in y0..y1 {
      let cy = py as f64 + 0.5;
      for px in x0..x1 {
        let cx = px as f64 + 0.5;
        let inside = edges.iter().all(|e| e[0] * cx + e[1] * cy + e[2] >= 0.0);
        if !inside {
          continue;
        }
        let z = a[2] + dzdx * (cx - a[0]) + dzdy * (cy - a[1]) + slope;
        let d = &mut texels[py * width + px];
        if z < *d {
          *d = z;
        }
        self.covered[0] = self.covered[0].min(px);
        self.covered[1] = self.covered[1].min(py);
        self.covered[2] = self.covered[2].max(px + 1);
        self.covered[3] = self.covered[3].max(py + 1);
      }
    }
  }
}

fn to_clip(m: &Mat4, p: [f64; 3]) -> [f64; 4] {
  [
    m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
    m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
    m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15],
  ]
}

/// Sutherland-Hodgman against the six clip planes (|x|, |y|, |z| <= w):
/// a triangle in, a convex polygon of up to nine vertices out, every
/// vertex with w > 0 so the division is safe and the screen coordinates
/// stay inside the buffer's range (no guard band needed).
fn clip_polygon(tri: &[[f64; 4]; 3]) -> Vec<[f64; 4]> {
  let mut poly: Vec<[f64; 4]> = tri.to_vec();
  let mut next: Vec<[f64; 4]> = Vec::with_capacity(9);
  for axis in 0..3 {
    for sign in [1.0f64, -1.0] {
      if poly.is_empty() {
        return poly;
      }
      next.clear();
      let dist = |v: &[f64; 4]| v[3] + sign * v[axis];
      for k in 0..poly.len() {
        let cur = poly[k];
        let prev = poly[(k + poly.len() - 1) % poly.len()];
        let (dc, dp) = (dist(&cur), dist(&prev));
        if dp >= 0.0 && dc >= 0.0 {
          next.push(cur);
        } else if dp >= 0.0 || dc >= 0.0 {
          let t = dp / (dp - dc);
          let at = [
            prev[0] + (cur[0] - prev[0]) * t,
            prev[1] + (cur[1] - prev[1]) * t,
            prev[2] + (cur[2] - prev[2]) * t,
            prev[3] + (cur[3] - prev[3]) * t,
          ];
          next.push(at);
          if dc >= 0.0 {
            next.push(cur);
          }
        }
      }
      std::mem::swap(&mut poly, &mut next);
    }
  }
  poly
}
