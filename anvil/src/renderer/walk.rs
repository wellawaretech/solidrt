//! The pure half of a draw: a list's ops walked into the instances the
//! programs draw (plain rect fills, rounded fills, shapes, glyphs, paint
//! shaders) and the path meshes' draws (a retained mesh by handle with its
//! map, color and clips), with the save stack, the transforms (perspective
//! among them, as the projective row a quad carries and a mesh's 3x3 map)
//! and the clip applied, the draws outside the clip rejected, a large
//! rounded fill split into plain pieces and a rounded rim (`split_rounded`),
//! a gradient's rows and ramp row put on its instances and mesh draws, and
//! the clips the device box and the per-instance rounded clip cannot take
//! turned into stencil clip draws (`ClipDraw`) with the stencil depth every
//! batch tests against. No GL here, so the walk is unit-tested without a context
//! (`tests/anvil.rs`, `tests/gradient.rs`, `tests/mesh.rs`).

use super::mesh::{scale_bucket, Mesh, MeshStyle};
use super::ops::{Op, Premultiplied, Radii, Source};
use super::path::Path;
use super::ramps::{RampTable, RAMP_ROWS};
use super::record::DisplayList;
use super::{PaintShader, ShaderInput, Texture};
use crate::{affine, Affine, ClipOperation, CoveragePolicy, FillType, GlyphQuad, RampGradient, TileMode};
use crate::{ISize, Matrix, Point, Rect, ShaderParam, Size, StrokeCap, TextureSampling};
use euclid::default::{Box2D, Point2D, Transform2D};
use euclid::Angle;
use lyon_path::builder::BorderRadii;
use lyon_path::Winding;
use std::f32::consts::SQRT_2;
use std::sync::{Arc, OnceLock};

/// One quad for any program, as the instance buffer carries it. All
/// f32 and `repr(C)`, so a slice of instances is uploaded as its bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
  /// The quad in local space: x, y, width, height.
  pub rect: [f32; 4],
  /// The linear part of the map, local to device: m11, m12, m21, m22
  /// (euclid's row-vector convention: device = local * m).
  pub linear: [f32; 4],
  /// m41, m42 (the map's translation), the shape kind, the stroke width
  /// (0 = fill).
  pub offset_kind_stroke: [f32; 4],
  /// Premultiplied color. The glyph kind alone carries straight alpha: its
  /// coverage policy reads the color's luminance and alpha apart, and the
  /// fragment stage premultiplies after it.
  pub color: [f32; 4],
  /// The texture rect in normalized texels at the quad's corners: u0, v0,
  /// u1, v1 for a glyph, which the glyph program samples by; or the line's
  /// cap and length in x and y (line kind). A textured shape or texture
  /// quad keeps its corner texels here too, but only because the glyph
  /// program's layout and the tests read the field: the shape program
  /// never reads `tex` for it and samples through the texel rows in
  /// `grad_x` and `grad_y`. For a rounded fill (`rounded_fill`) the whole
  /// shape quad in local space, x, y, w, h (for a rounded rect its padded
  /// quad): the rounded program measures the shape from it, so a piece of a
  /// split fill (`split_rounded`), whose `rect` is the piece, evaluates the
  /// same distance as the whole quad. Unused, zero, for other untextured
  /// rects and ovals.
  pub tex: [f32; 4],
  /// The clip in device pixels: x0, y0, x1, y1. Tested per fragment.
  pub clip: [f32; 4],
  /// The shape's corner radii in local pixels, CSS order; zero for the
  /// kinds without corners.
  pub radii: [f32; 4],
  /// The rounded clip in effect, in device pixels: x0, y0, x1, y1. Read
  /// only when `rclip_radii` has a non-zero entry.
  pub rclip: [f32; 4],
  /// The rounded clip's corner radii in device pixels, CSS order; all zero
  /// when no rounded clip is in effect.
  pub rclip_radii: [f32; 4],
  /// The map's projective row: p1, p2, p3, so w' = x * p1 + y * p2 + p3
  /// (m14, m24, m44), and in the spare fourth float the gradient's tile
  /// mode (`TILE_*`, 0 without a gradient). A 2D map without a gradient
  /// carries [0, 0, 1, 0].
  pub persp: [f32; 4],
  /// A gradient's x row over the quad's local space (3 floats) and its
  /// ramp row; a textured instance's u row over the same space and 0.
  pub grad_x: [f32; 4],
  /// A gradient's y row (3 floats) and its kind (`GradientKind`'s
  /// discriminant); a textured instance's v row and 0. A kind of 0 means
  /// no gradient.
  pub grad_y: [f32; 4],
}

/// The floats of one instance: twelve vec4 attributes (GLES 3.0 guarantees
/// sixteen).
pub const INSTANCE_FLOATS: usize = 48;

// The upload sends a slice of instances as its bytes: no padding, every
// field an f32.
const _: () = assert!(std::mem::size_of::<Instance>() == INSTANCE_FLOATS * std::mem::size_of::<f32>());

/// One vertex of a path mesh, as the mesh's vertex buffer carries it: its
/// position in the path's space (`Mesh::vertices`), uploaded as its bytes.
pub type MeshVertex = [f32; 2];

/// The floats of one mesh vertex: one vec2 attribute.
pub const MESH_VERTEX_FLOATS: usize = 2;

// The upload sends a slice of mesh vertices as its bytes: two f32s, no
// padding.
const _: () = assert!(std::mem::size_of::<MeshVertex>() == MESH_VERTEX_FLOATS * std::mem::size_of::<f32>());

/// The shape kinds the shape program's fragment stage switches on. The
/// `const int KIND_*` declarations in exec.rs's SHAPE_FRAGMENT must match
/// them.
pub const KIND_RECT: f32 = 0.0;
pub const KIND_OVAL: f32 = 1.0;
pub const KIND_LINE: f32 = 2.0;
/// A glyph quad: the atlas's red channel is its coverage, its color
/// straight alpha, turned into color under the batch's coverage policy.
/// It samples without `KIND_TEXTURED`, which means a premultiplied texel.
/// It draws through the glyph program, which never reads the kind; the
/// shape program has no arm for it.
pub const KIND_GLYPH: f32 = 3.0;
pub const KIND_ROUNDED_RECT: f32 = 4.0;
/// A paint shader's quad. It draws through its shader's program, which
/// never reads the kind; the shape program has no arm for it.
pub const KIND_PAINT: f32 = 5.0;
/// Added to a shape kind when the instance samples its texture: coverage
/// times texel. A texture quad is `KIND_RECT + KIND_TEXTURED`.
pub const KIND_TEXTURED: f32 = 8.0;

/// Local pixels a rounded rect's quad extends past its edge (and past a
/// stroke's half width), so the quad's edge never coincides with the
/// analytic edge on a straight run, where MSAA and the ramp would both
/// attenuate. exec.rs's GLSL const of the same name must match.
pub const ROUNDED_QUAD_PAD: f32 = 1.0;

/// How far apart the two axis scales of a transform may be and still count
/// as one scale for a rounded clip's device radii: float noise from
/// composed transforms, not a real non-uniform scale (the value oplog.rs
/// uses for the same question about stroke widths).
const UNIFORM_SCALE_EPSILON: f32 = 1e-5;

/// The line caps, as the instance carries them in `tex.x` (line kind). The
/// `const int CAP_*` declarations in exec.rs's SHAPE_FRAGMENT must match them.
pub const CAP_BUTT: f32 = 0.0;
pub const CAP_SQUARE: f32 = 1.0;
pub const CAP_ROUND: f32 = 2.0;

/// The device area, in device pixels, under which a plain rect fill rides
/// along in an open shape batch instead of opening a fill batch of its own
/// (256 x 256): about where the fragment cost the fill program would save
/// on the TV's Mali meets the CPU cost of one more draw call. A tuning
/// value, to be re-read on the TV.
pub const FILL_FOLD_AREA: f32 = 65_536.0;

/// The device area, in device pixels, from which a rounded fill splits into
/// plain pieces and a rim (`split_rounded`) instead of drawing as one
/// rounded instance (128 x 128): below it the extra instances cost more
/// than the rounded-box fragments they save (a chip, a small button). A
/// tuning value; the TV read sets it.
pub const ROUNDED_SPLIT_AREA: f32 = 16_384.0;

/// Device pixels a plain piece of a split rounded fill lies inside every
/// rounded edge, beyond `ROUNDED_QUAD_PAD`: the width of `coverage()`'s
/// ramp (exec.rs FRAGMENT_MATH), so a plain fragment, and the rim fragment
/// beside it, is at least a full ramp inside the edge and its coverage is
/// exactly 1.
pub const ROUNDED_SPLIT_MARGIN: f32 = 1.0;

/// The tile modes, as `Instance::persp`'s last float and a batch's or a
/// mesh draw's `uGradMeta` carry them. The `const int TILE_*`
/// declarations in exec.rs's GRADIENT_COMMON must match them.
pub const TILE_CLAMP: f32 = 0.0;
pub const TILE_REPEAT: f32 = 1.0;
pub const TILE_MIRROR: f32 = 2.0;
pub const TILE_DECAL: f32 = 3.0;

/// The program a batch draws through, by the class of its instances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BatchProgram {
  /// Plain rect fills only (see `plain_fill`): the fill program, no
  /// texture.
  Fill,
  /// Rounded fills only (see `rounded_fill`): untextured, unstroked rounded
  /// rects and plain rects under a rounded clip, through the rounded
  /// program, no texture.
  Rounded,
  /// Every other shape and texture quad, and the small plain and rounded
  /// fills that ride along with them: the shape program.
  Shape,
  /// Glyph runs: the glyph program.
  Glyph,
  /// One paint shader draw: the program of the batch's `paint` shader.
  Paint,
  /// One path mesh: the `Walked::mesh_draws` entry at the batch's start,
  /// drawn through the mesh program (its gradient or textured variant by
  /// the draw's `gradient` and `texture`).
  Mesh,
  /// One stencil clip draw: the `Walked::clip_draws` entry at the batch's
  /// start, its quad through the fill program or its mesh through the
  /// solid mesh program, the color writes off.
  Clip,
}

/// What the executor needs for a paint batch beside its one instance: the
/// shader, its params and inputs, and `resolution`, the quad's size in
/// device pixels (`iResolution`). Equal when it holds the same handle and
/// the same shared slices, and the same resolution.
#[derive(Clone)]
pub struct PaintDraw {
  pub shader: PaintShader,
  pub params: Arc<[ShaderParam]>,
  pub inputs: Arc<[ShaderInput]>,
  pub resolution: [f32; 2],
}

impl PartialEq for PaintDraw {
  fn eq(&self, other: &PaintDraw) -> bool {
    self.shader == other.shader
      && Arc::ptr_eq(&self.params, &other.params)
      && Arc::ptr_eq(&self.inputs, &other.inputs)
      && self.resolution == other.resolution
  }
}

impl std::fmt::Debug for PaintDraw {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("PaintDraw")
      .field("shader", &self.shader)
      .field("params", &self.params)
      .field("inputs", &self.inputs.len())
      .field("resolution", &self.resolution)
      .finish()
  }
}

/// A gradient as its instances and mesh draws draw it: the gradient,
/// shared with the op that carries it, `rows`, which map the space the
/// program evaluates the gradient in (a quad's local space, a path's space)
/// to the gradient's parameter space, and `row`, the ramp atlas row its
/// ramp sits in (`RampTable`). For a shape, a glyph run and a path op that
/// space is the op's space and the rows are the gradient's own; a line's
/// quad lives in its segment's frame, so its rows compose with the frame
/// map. Equal when it holds the same gradient, the same rows and the same
/// row.
#[derive(Clone)]
pub struct GradientInstance {
  pub gradient: Arc<RampGradient>,
  pub rows: Affine,
  pub row: u16,
}

impl PartialEq for GradientInstance {
  fn eq(&self, other: &GradientInstance) -> bool {
    Arc::ptr_eq(&self.gradient, &other.gradient) && self.rows == other.rows && self.row == other.row
  }
}

impl std::fmt::Debug for GradientInstance {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GradientInstance")
      .field("kind", &self.gradient.kind)
      .field("tile", &self.gradient.tile)
      .field("rows", &self.rows)
      .field("row", &self.row)
      .finish()
  }
}

/// What a fill, rounded or shape batch of gradient instances holds
/// throughout, its batch key: the gradient kind and the tile mode, as
/// `GradientInstance::kind()` and `tile()` give them, which the batch sets
/// as its program's `uGradMeta` uniform (exec.rs GRADIENT_BATCH). Each
/// instance still carries its own rows and ramp row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientClass {
  pub kind: f32,
  pub tile: f32,
}

impl GradientInstance {
  /// The batch key this gradient draws under: its kind and its tile.
  pub fn class(&self) -> GradientClass {
    GradientClass { kind: self.kind(), tile: self.tile() }
  }

  /// The kind as `Instance::grad_y`'s last float carries it:
  /// `GradientKind`'s discriminant, never 0.
  pub fn kind(&self) -> f32 {
    self.gradient.kind as i32 as f32
  }

  /// The tile mode as `Instance::persp`'s last float carries it: a
  /// `TILE_*`.
  pub fn tile(&self) -> f32 {
    tile_value(self.gradient.tile)
  }
}

// The tile mode as the instance carries it.
fn tile_value(tile: TileMode) -> f32 {
  match tile {
    TileMode::Clamp => TILE_CLAMP,
    TileMode::Repeat => TILE_REPEAT,
    TileMode::Mirror => TILE_MIRROR,
    TileMode::Decal => TILE_DECAL,
  }
}

/// A run of consecutive instances one draw issues, of one class: plain
/// rect fills (untextured, unstroked, under no rounded clip), rounded fills
/// (untextured, unstroked rounded rects, and plain rects under a rounded
/// clip), shapes (the shape instances before any texture, or every shape
/// that shares one texture and sampling, small solid plain and rounded
/// fills among them) or glyphs (one atlas and one coverage policy, solid
/// and gradient runs alike). The classes draw through different programs,
/// so a run never mixes them, except that the shape program also draws the
/// plain and rounded fills a solid shape run takes. A fill, a rounded or a
/// shape run is gradient or solid throughout: a gradient one draws through its
/// class's gradient variant and holds one gradient kind and tile mode
/// throughout (`gradient`), its instances still carrying their own rows and
/// ramp row. A paint shader draw is a run of its own, one
/// instance. A path mesh is a run of its own too, one
/// draw call per path op: its map, color, clips and source are the mesh
/// program's uniforms (`MeshDraw`).
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
  /// The run's first instance and its instance count for the quad
  /// programs; for `Mesh`, its index into `Walked::mesh_draws` and 1.
  pub start: usize,
  pub count: usize,
  /// The program the run draws through. `Glyph` exactly when `policy` is
  /// set; a `Fill` or a `Rounded` run has no texture.
  pub program: BatchProgram,
  /// The texture bound for the run, with its sampling; None for a run of
  /// fills, or of shapes that sample nothing. A mesh batch carries its
  /// draw's.
  pub texture: Option<(glow::NativeTexture, TextureSampling)>,
  /// Set exactly when the run is glyphs (`program` is `Glyph`): the policy
  /// their coverage is turned into color with.
  pub policy: Option<CoveragePolicy>,
  /// Set on a fill, rounded or shape run of gradient instances, which draws
  /// through its program's gradient variant: the one kind and tile mode
  /// every instance of the run has, set per batch as a uniform; each
  /// instance carries its own rows and ramp row. A mesh batch carries its
  /// draw's class (the draw's uniforms set the program's). None for every
  /// other run (a glyph run's instances carry their gradient, kind and tile
  /// whichever they are).
  pub gradient: Option<GradientClass>,
  /// Set exactly when the run is a paint shader draw (`program` is
  /// `Paint`); `texture` is then None, the inputs bind their own units.
  pub paint: Option<PaintDraw>,
  /// The stencil depth the run's draws test against: they draw only where
  /// the stencil equals it, inside every stencil clip in effect. 0 turns
  /// the test off (no stencil clip is in effect). A run takes a draw only
  /// at its own depth. 0 and unread on a `Clip` batch, whose draw carries
  /// its own test.
  pub stencil: u8,
}

/// One path op's mesh draw: the retained mesh by handle and what the mesh
/// program's uniforms take for it. The mesh stays in the path's space; the
/// GPU maps it.
#[derive(Clone, Debug)]
pub struct MeshDraw {
  pub mesh: Arc<Mesh>,
  /// The path's space to device pixels, homogeneous: the state's map.
  pub map: Projective,
  /// Premultiplied; under a source, the paint's alpha in all four
  /// channels.
  pub color: Premultiplied,
  /// The clip in device pixels: x0, y0, x1, y1. Tested per fragment.
  pub clip: [f32; 4],
  /// The rounded clip in effect, if any.
  pub rounded: Option<RoundedClip>,
  /// A gradient source's draw, its rows over the path's space: the
  /// gradient's own.
  pub gradient: Option<GradientInstance>,
  /// An image source's texture, with its sampling.
  pub texture: Option<(glow::NativeTexture, TextureSampling)>,
  /// An image source's map from the path's space to the normalized texel
  /// (`normalized_texel`).
  pub texel: Option<Affine>,
}

/// What a stencil clip draw covers: a rect under its map as one
/// `KIND_RECT` fill instance (its edges are MSAA's), or a rounded rect or an
/// oval as its tessellated mesh in the clip's own space under the map (a
/// mesh draw of the solid mesh program; its color and clips are unread, the
/// color writes being off). Under MSAA the stencil is per sample, so either
/// edge is antialiased.
#[derive(Clone, Debug)]
pub enum ClipShape {
  Quad(Instance),
  Mesh(MeshDraw),
}

/// One draw into the stencil, with the color writes off. Depth-counting, as
/// Impeller and Skia clip: inside every stencil clip in effect the stencil
/// holds the depth, the count of those clips, and a draw tests for it.
#[derive(Clone, Debug)]
pub enum ClipDraw {
  /// Where the stencil equals `reference`, increment it: an intersect
  /// clip's shape, or a difference clip's box before its `Lower`.
  Raise { reference: u8, shape: ClipShape },
  /// Where the stencil equals `reference`, decrement it: a difference
  /// clip's shape, cut back out of the box its `Raise` raised.
  Lower { reference: u8, shape: ClipShape },
  /// Where the stencil is above `to`, set it to `to`: a restore that drops
  /// the depth, over `area`, a device rect quad under the identity map
  /// covering every raise since the depth was `to` (`State::raised`).
  Restore { to: u8, area: Instance },
}

/// What a walk produces: the instances, the path mesh draws and the
/// stencil clip draws in painter's order and the batches over them, back
/// to back and covering every instance, every mesh draw and every clip
/// draw.
#[derive(Default)]
pub struct Walked {
  pub instances: Vec<Instance>,
  /// The path mesh draws, one per path op drawn, each its own batch.
  pub mesh_draws: Vec<MeshDraw>,
  /// The stencil clip draws, each its own `Clip` batch.
  pub clip_draws: Vec<ClipDraw>,
  pub batches: Vec<Batch>,
  /// The path ops drawn: one mesh each.
  pub meshes: u32,
  /// The meshes tessellated by this walk: the path caches' misses, a clip
  /// shape's among them.
  pub tessellated: u32,
  /// The union of the emitted instances' and meshes' device clip boxes,
  /// rounded outward to whole pixels: nothing draws outside it, so the
  /// executor scissors its clear and its draws to it. None when nothing
  /// was emitted.
  pub scissor: Option<Box2D<i32>>,
  /// The last batch takes no further instance: a paint shader draw ended
  /// it.
  sealed: bool,
}

impl Walked {
  // Append the shape `instance` to the current batch when it can share its
  // draw, else open a new batch. `gradient`, the instance's gradient kind
  // and tile mode (None for a solid one), is a class key like the program:
  // a gradient instance joins only a batch of its kind and tile, whose
  // program reads them from a uniform, a solid one only a solid batch, so
  // the fill program stays lean for solids. A plain fill (see
  // `plain_fill`) joins an open fill batch of its gradient class, or, when
  // solid, an open solid shape batch while its device area is below
  // FILL_FOLD_AREA (the shape program draws a rect too, and one draw call
  // fewer where rects and other shapes interleave); otherwise it opens a
  // fill batch. A rounded fill (see `rounded_fill`) follows the same rules
  // with its own class: it joins an open rounded batch of its gradient class,
  // or, when solid, rides along in an open solid shape batch below the same
  // FILL_FOLD_AREA (the shape program draws a rounded rect and a rounded
  // clip too, one draw call fewer where rounded rects and other shapes
  // interleave), except a rim piece of a split rounded rect, which the shape
  // program cannot draw (it measures a rounded rect from its quad, the piece,
  // not from the whole quad in `tex`); otherwise it opens a rounded batch.
  // Any other shape shares only an open shape batch of its gradient class
  // (the shader branches on the kind; a bound sampler it never reads costs
  // nothing), so a texture change, a sampling change, a fill or rounded run
  // or a glyph run ends a run. `stencil` is a key too: a batch takes only an instance of its own
  // stencil depth.
  fn push(
    &mut self,
    instance: Instance,
    texture: Option<(glow::NativeTexture, TextureSampling)>,
    gradient: Option<GradientClass>,
    stencil: u8,
  ) {
    let start = self.instances.len();
    let sealed = self.sealed;
    let last = self.batches.last_mut().filter(|last| !sealed && last.stencil == stencil);
    let batch = |program| Batch { start, count: 1, program, texture, policy: None, gradient, paint: None, stencil };
    // The two lean classes; an instance is at most one of them.
    let lean = if plain_fill(&instance) {
      Some(BatchProgram::Fill)
    } else if rounded_fill(&instance) {
      Some(BatchProgram::Rounded)
    } else {
      None
    };
    if let Some(own) = lean {
      match last {
        Some(last) if last.program == own && last.gradient == gradient => last.count += 1,
        Some(last)
          if gradient.is_none()
            && last.gradient.is_none()
            && last.program == BatchProgram::Shape
            && device_area(&instance) < FILL_FOLD_AREA
            && !rounded_piece(&instance) =>
        {
          last.count += 1
        }
        _ => self.open(Batch { texture: None, ..batch(own) }),
      }
      self.instances.push(instance);
      return;
    }
    match last.filter(|last| last.program == BatchProgram::Shape && last.gradient == gradient) {
      None => self.open(batch(BatchProgram::Shape)),
      Some(last) if texture.is_none() || last.texture == texture => last.count += 1,
      // The shapes before it do not care which sampler is bound.
      Some(last) if last.texture.is_none() => {
        last.texture = texture;
        last.count += 1;
      }
      Some(_) => self.open(batch(BatchProgram::Shape)),
    }
    self.instances.push(instance);
  }

  // Append one glyph op's surviving `instances` over the atlas `atlas`
  // (sampled linearly) under `policy`. The run joins the current batch
  // when that batch is a glyph run over the same atlas under the same
  // policy at the same stencil depth, whatever either's gradient: the glyph
  // program reads the gradient per instance.
  fn push_glyphs(&mut self, instances: Vec<Instance>, atlas: glow::NativeTexture, policy: CoveragePolicy, stencil: u8) {
    if instances.is_empty() {
      return;
    }
    let start = self.instances.len();
    let count = instances.len();
    let texture = Some((atlas, TextureSampling::Linear));
    let joins = |last: &Batch| {
      last.program == BatchProgram::Glyph
        && last.texture == texture
        && last.policy == Some(policy)
        && last.stencil == stencil
    };
    let sealed = self.sealed;
    match self.batches.last_mut().filter(|last| !sealed && joins(last)) {
      Some(last) => last.count += count,
      None => self.open(Batch {
        start,
        count,
        program: BatchProgram::Glyph,
        texture,
        policy: Some(policy),
        gradient: None,
        paint: None,
        stencil,
      }),
    }
    self.instances.extend(instances);
  }

  // Append a paint shader's one `instance` as a batch of its own, sealed
  // after it: the program is the shader's, so nothing before or after it
  // shares its draw.
  fn push_paint(&mut self, instance: Instance, paint: PaintDraw, stencil: u8) {
    let start = self.instances.len();
    self.open(Batch {
      start,
      count: 1,
      program: BatchProgram::Paint,
      texture: None,
      policy: None,
      gradient: None,
      paint: Some(paint),
      stencil,
    });
    self.sealed = true;
    self.instances.push(instance);
  }

  // Append one path op's mesh `draw` as a batch of its own: its map,
  // color, clips and source are the mesh program's uniforms, so no other
  // path or quad shares its draw call; it tests the stencil at `stencil`.
  fn push_mesh(&mut self, draw: MeshDraw, stencil: u8) {
    let start = self.mesh_draws.len();
    self.open(Batch {
      start,
      count: 1,
      program: BatchProgram::Mesh,
      texture: draw.texture,
      policy: None,
      gradient: draw.gradient.as_ref().map(GradientInstance::class),
      paint: None,
      stencil,
    });
    self.mesh_draws.push(draw);
  }

  // Append one stencil clip `draw` as a batch of its own. No quad batch
  // takes an instance after it (`push` and `push_glyphs` join only their
  // own programs), so it needs no seal.
  fn push_clip(&mut self, draw: ClipDraw) {
    let start = self.clip_draws.len();
    self.open(Batch {
      start,
      count: 1,
      program: BatchProgram::Clip,
      texture: None,
      policy: None,
      gradient: None,
      paint: None,
      stencil: 0,
    });
    self.clip_draws.push(draw);
  }

  // Open `batch` as the current one, which takes instances again.
  fn open(&mut self, batch: Batch) {
    self.batches.push(batch);
    self.sealed = false;
  }
}

/// The transform and the clips in effect, as `Save` pushes them.
#[derive(Clone, Copy)]
struct State {
  matrix: Matrix,
  /// Device pixels. Empty (max below min) once two clips are disjoint,
  /// which rejects every draw until the enclosing restore. A rounded clip
  /// and an intersect stencil clip narrow it to their bounds too, so
  /// rejection and the box test hold.
  clip: Box2D<f32>,
  /// The one per-instance rounded clip in effect, if any; a further one
  /// clips through the stencil.
  rounded: Option<RoundedClip>,
  /// The stencil clips in effect: the stencil depth every draw tests
  /// against, 0 for none.
  depth: u8,
  /// The device area every stencil raise of the clips in effect may have
  /// touched, within the target; None while none was made. A restore that
  /// drops the depth resets the stencil over it.
  raised: Option<Box2D<f32>>,
  /// The target, device pixels: the area of a raise whose bounds are
  /// unknown (a corner behind the eye).
  target: Box2D<f32>,
}

/// An axis-aligned rounded clip, in device pixels: its rect and its corner
/// radii in CSS order, at least one of them non-zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedClip {
  pub rect: Box2D<f32>,
  pub radii: Radii,
}

/// The instances a list draws into a target of `size` device pixels, in
/// painter's order, with every op's state applied: the transform stack,
/// the device-space clip, and the rejection of draws that fall outside it.
/// Every stage 1 shape draws: rect and oval fills and strokes, lines with
/// their caps, and texture quads; stage 2 adds rounded rects, one rounded
/// clip in device space, shapes filled with an image, and glyph runs;
/// stage 4 adds gradients and paths, a path as its mesh's device-space
/// triangles (`push_path`); stage 5 draws every quad under a perspective
/// map, its projective row on the instance, and every clip (rect, rounded
/// rect and oval, intersect and difference, under any map, nested to any
/// depth), those the box and the per-instance rounded clip cannot take
/// through the stencil (`clip`). A stroke expands its quad by
/// half the width on every side so it covers the band, which the fragment
/// stage keeps from the quad's local coordinates. There is no hairline
/// rule: a width of zero or below draws nothing, and a width under one
/// device pixel draws thin, covering only some of the MSAA samples it
/// crosses. Every gradient op takes a row of `ramps` for its ramp; past the
/// table's rows it draws its paint's color.
pub fn walk(list: &DisplayList, size: ISize, ramps: &mut RampTable) -> Walked {
  // The target rect, not an infinite one: every instance's clip stays
  // finite (a highp float holds 1e7 but not INFINITY reliably), the
  // rejection culls offscreen content, and the rendertree's "infinite"
  // overflow extents (composite.rs, CLIP_INF) intersect down to the target.
  let target = Box2D::from_size(size.to_f32());
  let mut state = State { matrix: Matrix::identity(), clip: target, rounded: None, depth: 0, raised: None, target };
  let mut stack = Vec::new();
  let mut out = Walked::default();
  walk_ops(list.executable(), &mut state, &mut stack, ramps, &mut out);
  out.scissor = scissor(&out.instances, &out.mesh_draws);
  out
}

// The union of `instances`' and the mesh draws' device clips, rounded
// outward to whole pixels; None for neither. Every clip is already
// intersected with the target and finite (see `walk`), so the union lies
// inside the target.
fn scissor(instances: &[Instance], mesh_draws: &[MeshDraw]) -> Option<Box2D<i32>> {
  let clips = instances.iter().map(|i| &i.clip).chain(mesh_draws.iter().map(|d| &d.clip));
  let union = clips.map(|c| Box2D::new(Point2D::new(c[0], c[1]), Point2D::new(c[2], c[3])));
  union.reduce(|a, b| a.union(&b)).map(|b| b.round_out().to_i32())
}

// One list's ops. A restore pops only what this list saved (a restore with
// nothing saved does nothing, as `oplog::walk` treats it), so a nested
// list with an extra restore cannot pop its parent's save.
fn walk_ops(ops: &[Op], state: &mut State, stack: &mut Vec<State>, ramps: &mut RampTable, out: &mut Walked) {
  let base = stack.len();
  for op in ops {
    match op {
      Op::Save => stack.push(*state),
      Op::Restore => {
        if stack.len() > base {
          if let Some(saved) = stack.pop() {
            restore_stencil(state, &saved, out);
            *state = saved;
          }
        }
      }
      Op::Translate { x, y } => state.matrix = Matrix::translation(*x, *y, 0.0).then(&state.matrix),
      Op::Scale { x, y } => state.matrix = Matrix::scale(*x, *y, 1.0).then(&state.matrix),
      Op::Transform(m) => state.matrix = m.then(&state.matrix),
      Op::ClipRect { rect, op } => clip(state, rect, ClipGeometry::Rect, *op, out),
      Op::ClipRoundedRect { rect, radii, op, path } => {
        let geometry =
          if radii.iter().all(|r| *r == 0.0) { ClipGeometry::Rect } else { ClipGeometry::Rounded(radii, path) };
        clip(state, rect, geometry, *op, out)
      }
      Op::ClipOval { bounds, op, path } => clip(state, bounds, ClipGeometry::Oval(path), *op, out),
      Op::DrawRect { rect, radii, color, stroke, source } => {
        let kind = if radii.iter().all(|r| *r == 0.0) { KIND_RECT } else { KIND_ROUNDED_RECT };
        push_shape(state, rect, kind, *radii, *color, *stroke, source.as_ref(), ramps, out)
      }
      Op::DrawOval { bounds, color, stroke, source } => {
        push_shape(state, bounds, KIND_OVAL, [0.0; 4], *color, *stroke, source.as_ref(), ramps, out)
      }
      Op::DrawLine { from, to, color, width, cap, gradient } => {
        push_line(state, *from, *to, *color, *width, *cap, gradient.as_ref(), ramps, out)
      }
      Op::DrawTexture { texture, src, dst, sampling, alpha } => {
        push_texture(state, texture, src, dst, *sampling, *alpha, out)
      }
      Op::DrawGlyphs { atlas, quads, bounds, color, policy, gradient } => {
        push_glyphs(state, atlas, quads, bounds, *color, *policy, gradient.as_ref(), ramps, out)
      }
      Op::DrawShader { rect, shader, params, inputs } => push_paint(state, rect, shader, params, inputs, out),
      Op::DrawPath { path, style, color, source } => {
        push_path(state, path, *style, *color, source.as_ref(), ramps, out)
      }
      Op::DrawList { list, opacity } => {
        if *opacity < 1.0 {
          unsupported!("draw_display_list below opacity 1", "the nested list draws at full opacity");
        }
        // Whatever the nested list leaves on the stack or in the state
        // ends with it, the stencil clips it left in effect among it.
        let depth = stack.len();
        let saved = *state;
        walk_ops(list.executable(), state, stack, ramps, out);
        stack.truncate(depth);
        restore_stencil(state, &saved, out);
        *state = saved;
      }
    }
  }
}

/// A map from a draw's local space to device pixels as the instance
/// carries it: the 3x3 of a `Matrix` with z = 0, in euclid's row-vector
/// convention, x' = x * m11 + y * m21 + m41, y' = x * m12 + y * m22 + m42,
/// w' = x * m14 + y * m24 + m44, and the device point (x' / w', y' / w').
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projective {
  /// m11, m12, m21, m22.
  pub linear: [f32; 4],
  /// m41, m42.
  pub offset: [f32; 2],
  /// m14, m24, m44: [0, 0, 1] for a 2D map.
  pub persp: [f32; 3],
}

impl Projective {
  /// The map `m` takes the z = 0 plane through.
  pub fn from_matrix(m: &Matrix) -> Projective {
    Projective { linear: [m.m11, m.m12, m.m21, m.m22], offset: [m.m41, m.m42], persp: [m.m14, m.m24, m.m44] }
  }

  /// Whether the map is 2D: w' is 1 everywhere.
  pub fn is_affine(&self) -> bool {
    self.persp == [0.0, 0.0, 1.0]
  }

  // Whether the map keeps a rect axis-aligned: 2D, no rotation or skew.
  fn is_axis_aligned(&self) -> bool {
    self.is_affine() && self.linear[1] == 0.0 && self.linear[2] == 0.0
  }

  /// The device point `p` maps to; None when it lies at or behind the
  /// eye (w' <= 0).
  pub fn project(&self, p: Point) -> Option<Point> {
    let [m11, m12, m21, m22] = self.linear;
    let x = p.x * m11 + p.y * m21 + self.offset[0];
    let y = p.x * m12 + p.y * m22 + self.offset[1];
    if self.is_affine() {
      return Some(Point::new(x, y));
    }
    let [p1, p2, p3] = self.persp;
    let w = p.x * p1 + p.y * p2 + p3;
    // A NaN w' has no device point either.
    if w <= 0.0 || w.is_nan() {
      return None;
    }
    Some(Point::new(x / w, y / w))
  }

  /// The device box of `rect`'s four projected corners; None when any
  /// corner lies at or behind the eye. w' is affine in (x, y), so all of
  /// the rect is in front of the eye exactly when its corners are.
  pub fn bounds(&self, rect: &Rect) -> Option<Box2D<f32>> {
    let corners = [
      rect.origin,
      Point::new(rect.max_x(), rect.min_y()),
      Point::new(rect.min_x(), rect.max_y()),
      Point::new(rect.max_x(), rect.max_y()),
    ];
    let mut projected = [Point::zero(); 4];
    for (out, corner) in projected.iter_mut().zip(corners) {
      *out = self.project(corner)?;
    }
    Some(Box2D::from_points(projected))
  }

  // The 2D part of the map: exact for a 2D map, and under perspective the
  // linear part a mesh's scale bucket reads.
  fn affine_part(&self) -> Transform2D<f32> {
    let [m11, m12, m21, m22] = self.linear;
    Transform2D::new(m11, m12, m21, m22, self.offset[0], self.offset[1])
  }
}

/// The deepest the stencil counts: an 8-bit stencil buffer, every target's
/// (DEPTH24_STENCIL8).
const MAX_STENCIL_DEPTH: u8 = u8::MAX;

/// A clip op's shape: a plain rect, a rounded rect with its (clamped,
/// local) radii, or an oval in its bounds; the rounded rect and the oval
/// with the op's path slot, which their geometry is built into once, so a
/// retained list keeps the path, its mesh cache and its GPU buffers.
#[derive(Clone, Copy)]
enum ClipGeometry<'a> {
  Rect,
  Rounded(&'a Radii, &'a OnceLock<Path>),
  Oval(&'a OnceLock<Path>),
}

// A clip to `rect` (`geometry` in it) combined by `op` with the clips in
// effect, the first of these that takes it:
//   1. an intersect of a plain rect under an axis-aligned map: the device
//      box;
//   2. an intersect of a rounded rect under an axis-aligned uniform-scale
//      map with no rounded clip in effect: the box and the per-instance
//      rounded clip (`clip_rounded`);
//   3. anything else: a stencil clip (`clip_stencil`).
fn clip(state: &mut State, rect: &Rect, geometry: ClipGeometry, op: ClipOperation, out: &mut Walked) {
  let p = Projective::from_matrix(&state.matrix);
  if op == ClipOperation::Intersect && p.is_axis_aligned() {
    match geometry {
      ClipGeometry::Rect => {
        // A 2D map: every corner is in front of the eye.
        if let Some(device) = p.bounds(rect) {
          state.clip = state.clip.intersection_unchecked(&device);
        }
        return;
      }
      // False when the scale is not uniform: the stencil takes it.
      ClipGeometry::Rounded(radii, _) if state.rounded.is_none() && clip_rounded(state, &p, rect, radii) => return,
      _ => {}
    }
  }
  clip_stencil(state, &p, rect, geometry, op, out);
}

// An intersect clip to `rect` with the corner `radii` (local, CSS order,
// clamped) under the axis-aligned map `p`, as the per-instance rounded clip
// in device space: the device box narrows the clip box, the device radii
// ride on the instances. False, changing nothing, when the map's two axis
// scales differ: the corners would be elliptical in device space, which the
// stencil takes.
fn clip_rounded(state: &mut State, p: &Projective, rect: &Rect, radii: &Radii) -> bool {
  let [m11, _, _, m22] = p.linear;
  let (sx, sy) = (m11.abs(), m22.abs());
  if (sx - sy).abs() > UNIFORM_SCALE_EPSILON {
    return false;
  }
  // A 2D map: every corner is in front of the eye.
  let Some(device) = p.bounds(rect) else { return false };
  state.clip = state.clip.intersection_unchecked(&device);
  let scale = sx.min(sy);
  let [tl, tr, br, bl] = radii.map(|r| r * scale);
  // A mirrored axis swaps the corners: local top-left lands on device
  // top-right under a negative x scale, on bottom-left under a negative y.
  let [tl, tr, br, bl] = if m11 < 0.0 { [tr, tl, bl, br] } else { [tl, tr, br, bl] };
  let device_radii = if m22 < 0.0 { [bl, br, tr, tl] } else { [tl, tr, br, bl] };
  if device_radii.iter().any(|r| *r > 0.0) {
    state.rounded = Some(RoundedClip { rect: device, radii: device_radii });
  }
  true
}

// A clip to `rect` (`geometry` in it) under `p` through the stencil. An
// intersect raises its shape where the stencil holds the depth, and an
// intersect also narrows the box to the shape's projected bounds when
// they exist (a corner behind the eye leaves the box as it was: the
// shape still draws, homogeneously, and the GPU clips it). A difference
// raises the box, then lowers its shape back where the box raised: outside
// the shape the stencil reads the new depth, inside it the old one, which
// a draw at the new depth rejects; it never narrows the box. Either way the
// depth then counts one more. A shape that covers nothing (a map that
// collapses it, an empty rounded rect or oval) keeps nothing under an
// intersect and cuts nothing under a difference. Nothing is raised once
// the box is empty (every draw is rejected anyway), an intersect whose
// bounds empty the box among it, or past MAX_STENCIL_DEPTH, where an
// intersect degrades to its box and a difference is dropped, warned once.
fn clip_stencil(
  state: &mut State,
  p: &Projective,
  rect: &Rect,
  geometry: ClipGeometry,
  op: ClipOperation,
  out: &mut Walked,
) {
  if state.clip.is_empty() {
    return;
  }
  let bounds = p.bounds(rect);
  if state.depth == MAX_STENCIL_DEPTH {
    match op {
      ClipOperation::Intersect => {
        unsupported!("stencil clips nested past 255 deep", "an intersect is its bounding box");
        if let Some(bounds) = bounds {
          state.clip = state.clip.intersection_unchecked(&bounds);
        }
      }
      ClipOperation::Difference => {
        unsupported!("stencil clips nested past 255 deep", "a difference is ignored");
      }
    }
    return;
  }
  let clip = box_array(&state.clip);
  let shape = match geometry {
    ClipGeometry::Rect => Some(ClipShape::Quad(clip_quad(p, rect, clip))),
    ClipGeometry::Rounded(radii, path) => clip_mesh(p, path.get_or_init(|| rounded_path(rect, radii)), clip, out),
    ClipGeometry::Oval(path) => clip_mesh(p, path.get_or_init(|| oval_path(rect)), clip, out),
  };
  let depth = state.depth;
  match op {
    ClipOperation::Intersect => {
      let Some(shape) = shape else {
        state.clip = Box2D::zero();
        return;
      };
      if let Some(b) = bounds {
        state.clip = state.clip.intersection_unchecked(&b);
      }
      // A shape whose bounds miss the box keeps nothing: every later draw
      // is rejected by the empty box, so the raise would cost a stencil
      // draw for none.
      if state.clip.is_empty() {
        return;
      }
      out.push_clip(ClipDraw::Raise { reference: depth, shape });
      // The raise is not held to the box (neither program discards), so its
      // area is the shape's bounds within the target, or the whole target.
      let area = match bounds {
        Some(b) => b.intersection(&state.target),
        None => Some(state.target),
      };
      raise(state, area);
    }
    ClipOperation::Difference => {
      let Some(shape) = shape else { return };
      let identity = Projective::from_matrix(&Matrix::identity());
      let area = clip_quad(&identity, &state.clip.to_rect(), clip);
      out.push_clip(ClipDraw::Raise { reference: depth, shape: ClipShape::Quad(area) });
      out.push_clip(ClipDraw::Lower { reference: depth + 1, shape });
      // The box raise covers the box exactly; the lower only takes back
      // what it raised.
      let area = Some(state.clip);
      raise(state, area);
    }
  }
}

// Count one stencil clip more in `state`, whose raise touched `area`.
fn raise(state: &mut State, area: Option<Box2D<f32>>) {
  state.raised = match (state.raised, area) {
    (Some(a), Some(b)) => Some(a.union(&b)),
    (a, b) => a.or(b),
  };
  state.depth += 1;
}

// The stencil reset of a restore from `popped` to `saved`: when the depth
// drops, one `Restore` to the saved depth over every area `popped` raised
// (`State::raised`). Its test passes only where the stencil is above the
// saved depth, and only a raise since the depth was there left it so, so
// an area larger than those raises (`raised` holds the saved state's own
// too) changes nothing more. The raised area, not the popping state's clip
// box: an intersect raise is not held to the box it was made in (neither
// program discards), so a raise of a shape reaching past an outer clip's
// box would outlive a restore over the box and reject the parent's later
// draws there.
fn restore_stencil(popped: &State, saved: &State, out: &mut Walked) {
  if popped.depth <= saved.depth {
    return;
  }
  // Rounded outward to whole pixels: a raise's rasterized edge covers the
  // samples inside its shape, which may reach a sub-pixel past its float
  // bounds' edge under MSAA, never past the whole pixel. LESS passes only
  // where a raise left the stencil above `to`, so the larger area changes
  // nothing more. Within the target still: `raised` is, and the target's
  // edges are whole pixels.
  let Some(raised) = popped.raised.map(|r| r.round_out()) else { return };
  let identity = Projective::from_matrix(&Matrix::identity());
  let area = clip_quad(&identity, &raised.to_rect(), box_array(&raised));
  out.push_clip(ClipDraw::Restore { to: saved.depth, area });
}

// `b` as an instance's clip carries it: x0, y0, x1, y1.
fn box_array(b: &Box2D<f32>) -> [f32; 4] {
  [b.min.x, b.min.y, b.max.x, b.max.y]
}

// A clip quad: `rect` under `p` as a plain `KIND_RECT` fill instance with
// the device clip `clip`, no color (the stencil draws write none) and no
// rounded clip.
fn clip_quad(p: &Projective, rect: &Rect, clip: [f32; 4]) -> Instance {
  let [p1, p2, p3] = p.persp;
  Instance {
    rect: [rect.origin.x, rect.origin.y, rect.size.width, rect.size.height],
    linear: p.linear,
    offset_kind_stroke: [p.offset[0], p.offset[1], KIND_RECT, 0.0],
    color: [0.0; 4],
    tex: [0.0; 4],
    clip,
    radii: [0.0; 4],
    rclip: [0.0; 4],
    rclip_radii: [0.0; 4],
    persp: [p1, p2, p3, 0.0],
    grad_x: [0.0; 4],
    grad_y: [0.0; 4],
  }
}

// A clip shape's mesh: `path`'s fill at the scale bucket of `p`'s 2D part
// (under perspective an approximation, as for a path op), tessellated on a
// cache miss, drawn by handle in the clip's own space under `p`. None when
// the map collapses it or the path covers nothing.
fn clip_mesh(p: &Projective, path: &Path, clip: [f32; 4], out: &mut Walked) -> Option<ClipShape> {
  let bucket = scale_bucket(&p.affine_part())?;
  let (mesh, missed) = path.mesh(MeshStyle::Fill, bucket);
  out.tessellated += missed as u32;
  let mesh = mesh?;
  Some(ClipShape::Mesh(MeshDraw {
    mesh,
    map: *p,
    color: [0.0; 4],
    clip,
    rounded: None,
    gradient: None,
    texture: None,
    texel: None,
  }))
}

// The rounded rect `rect` with the corner `radii` (CSS order) as a path.
fn rounded_path(rect: &Rect, radii: &Radii) -> Path {
  let [top_left, top_right, bottom_right, bottom_left] = *radii;
  let radii = BorderRadii { top_left, top_right, bottom_left, bottom_right };
  let mut builder = lyon_path::Path::builder();
  builder.add_rounded_rectangle(&rect.to_box2d(), &radii, Winding::Positive);
  Path::from_geometry(builder.build(), FillType::NonZero)
}

// The oval in `bounds` as a path.
fn oval_path(bounds: &Rect) -> Path {
  let radii = euclid::default::Vector2D::new(bounds.size.width * 0.5, bounds.size.height * 0.5);
  let mut builder = lyon_path::Path::builder();
  builder.add_ellipse(bounds.center(), radii, Angle::zero(), Winding::Positive);
  Path::from_geometry(builder.build(), FillType::NonZero)
}

// `gradient` drawn with `rows` from the quad's local space, at the ramp row
// `ramps` gives it; None, warned once, when every row is taken this frame,
// and the draw then takes its solid color.
fn gradient_instance(ramps: &mut RampTable, gradient: &Arc<RampGradient>, rows: Affine) -> Option<GradientInstance> {
  let Some(row) = ramps.row(gradient) else {
    warn_once!(
      "[anvil] Renderer: more than {RAMP_ROWS} distinct gradients in one frame; the rest draw their paint's color"
    );
    return None;
  };
  Some(GradientInstance { gradient: gradient.clone(), rows, row })
}

// The map and the device clip of a draw covering `rect` under the current
// state; None when the clip rejects it.
fn placed(state: &State, rect: &Rect) -> Option<(Projective, [f32; 4])> {
  let p = Projective::from_matrix(&state.matrix);
  placed_under(state, &p, rect).map(|clip| (p, clip))
}

// The device clip of a draw covering `rect` under the map `p` (the
// state's, or a line's own composed onto it); None when the clip rejects
// it. A rect with a corner behind the eye has no device bounds and is not
// culled.
fn placed_under(state: &State, p: &Projective, rect: &Rect) -> Option<[f32; 4]> {
  if state.clip.is_empty() {
    return None;
  }
  if let Some(bounds) = p.bounds(rect) {
    if !bounds.intersects(&state.clip) {
      return None;
    }
  }
  let clip = state.clip;
  Some([clip.min.x, clip.min.y, clip.max.x, clip.max.y])
}

/// What one draw puts on its instance beside the state's map and clips.
struct Quad {
  /// The quad in local space.
  rect: Rect,
  /// A `KIND_*`, with `KIND_TEXTURED` added when it samples.
  kind: f32,
  /// The stroke width; 0 fills.
  stroke: f32,
  color: Premultiplied,
  tex: [f32; 4],
  /// The shape's corner radii; zero for the kinds without corners.
  radii: Radii,
  /// `grad_x`, `grad_y` and the tile in `persp`'s last float: a
  /// gradient's (`gradient_rows`), a textured instance's texel rows
  /// (`texel_rows`), zeros otherwise.
  rows: Rows,
}

/// What an instance's gradient slots carry: `grad_x`, `grad_y` and the
/// tile mode that goes into `persp`'s spare last float.
#[derive(Clone, Copy)]
struct Rows {
  x: [f32; 4],
  y: [f32; 4],
  tile: f32,
}

/// The slots of an instance without a gradient or texel rows: zeros.
const NO_ROWS: Rows = Rows { x: [0.0; 4], y: [0.0; 4], tile: 0.0 };

// A gradient's rows, ramp row, kind and tile, as `grad_x`, `grad_y` and
// `persp`'s last float carry them, the kind and the tile apart; zeros for
// none.
fn gradient_rows(gradient: Option<&GradientInstance>) -> Rows {
  match gradient {
    Some(g) => {
      let [[x0, x1, x2], [y0, y1, y2]] = g.rows;
      Rows { x: [x0, x1, x2, g.row as f32], y: [y0, y1, y2, g.kind()], tile: g.tile() }
    }
    None => NO_ROWS,
  }
}

// A textured instance's map from its local space to the normalized texel,
// as `grad_x` and `grad_y` carry it: the kind 0, no gradient.
fn texel_rows(rows: Affine) -> Rows {
  let [[u0, u1, u2], [v0, v1, v2]] = rows;
  Rows { x: [u0, u1, u2, 0.0], y: [v0, v1, v2, 0.0], tile: 0.0 }
}

// The one place an instance's fields are set: `quad` under the map `p`
// with the device clip `clip` (from `placed`), and the state's rounded
// clip, zeros when none is in effect.
fn instance(state: &State, p: &Projective, clip: [f32; 4], quad: Quad) -> Instance {
  instance_under(state.rounded, p, clip, quad)
}

// `instance` under the rounded clip `rounded` in place of the state's: None
// for a plain piece of a split rounded fill, whose coverage under the
// state's clip is 1 by construction (`split_rounded`).
fn instance_under(rounded: Option<RoundedClip>, p: &Projective, clip: [f32; 4], quad: Quad) -> Instance {
  let (rclip, rclip_radii) = match rounded {
    Some(r) => ([r.rect.min.x, r.rect.min.y, r.rect.max.x, r.rect.max.y], r.radii),
    None => ([0.0; 4], [0.0; 4]),
  };
  let rect = quad.rect;
  let [p1, p2, p3] = p.persp;
  Instance {
    rect: [rect.origin.x, rect.origin.y, rect.size.width, rect.size.height],
    linear: p.linear,
    offset_kind_stroke: [p.offset[0], p.offset[1], quad.kind, quad.stroke],
    color: quad.color,
    tex: quad.tex,
    clip,
    radii: quad.radii,
    rclip,
    rclip_radii,
    persp: [p1, p2, p3, quad.rows.tile],
    grad_x: quad.rows.x,
    grad_y: quad.rows.y,
  }
}

// A rect, a rounded rect or an oval (`kind`) under the current state,
// filled or stroked `stroke` wide centered on its edge, unless the clip
// rejects it or it is fully transparent. A stroke's quad is the shape's box
// grown by half the width on every side, so it covers the outer half of
// the band; a rounded rect's grows by `ROUNDED_QUAD_PAD` more, so its
// analytic edge never lies on the quad's. With an image `source` the
// instance samples it: coverage times texel, faded by the color's alpha
// (the paint's RGB is not used for an image, as Impeller did not use it),
// the uv through the image's inverse map from the quad's local space. With
// a gradient `source` it keeps its kind and takes the gradient's color,
// faded the same way, in a batch of its gradient's kind and tile
// (`Walked::push`); past the ramp table's rows it draws its solid color. A
// rounded fill under a 2D axis-aligned map whose device area reaches
// ROUNDED_SPLIT_AREA splits (`split_rounded`, against the shape's rounded
// rect and the rounded clip in local space): its rim pieces keep the kind,
// the radii, the rounded clip and the whole quad in `tex`, and draw through
// the rounded program; its plain pieces are plain rect fills with no
// rounded clip, the fill program's. Color, rows and the device clip are the
// op's on every piece.
#[allow(clippy::too_many_arguments)]
fn push_shape(
  state: &State,
  rect: &Rect,
  kind: f32,
  radii: Radii,
  color: Premultiplied,
  stroke: Option<f32>,
  source: Option<&Source>,
  ramps: &mut RampTable,
  out: &mut Walked,
) {
  let s = stroke.unwrap_or(0.0);
  let pad = if kind == KIND_ROUNDED_RECT { ROUNDED_QUAD_PAD } else { 0.0 };
  let grow = s * 0.5 + pad;
  let quad = rect.inflate(grow, grow);
  let Some((p, clip)) = placed(state, &quad) else { return };
  if color[3] == 0.0 {
    return;
  }
  // The local space is the op's: the gradient's own rows.
  let gradient = match source {
    Some(Source::Gradient(g)) => gradient_instance(ramps, g, [g.x_row, g.y_row]),
    _ => None,
  };
  let image = match source {
    Some(Source::Image(image)) => Some(image),
    _ => None,
  };
  let (kind, color, tex, texture, rows) = match image {
    // The ramp sample is premultiplied in the program: the alpha in all
    // four channels fades it, as for an image.
    None if gradient.is_some() => (kind, [color[3]; 4], [0.0; 4], None, gradient_rows(gradient.as_ref())),
    None => (kind, color, [0.0; 4], None, NO_ROWS),
    Some(image) => {
      // A texture's size is never zero (see `push_texture`).
      let size = image.texture.size();
      let (w, h) = (size.width as f32, size.height as f32);
      let t0 = image.to_texel.transform_point(quad.min());
      let t1 = image.to_texel.transform_point(quad.max());
      // The texel is premultiplied: the alpha in all four channels fades it.
      let faded = [color[3]; 4];
      (
        kind + KIND_TEXTURED,
        faded,
        [t0.x / w, t0.y / h, t1.x / w, t1.y / h],
        Some((image.texture.name(), image.sampling)),
        texel_rows(normalized_texel(image)),
      )
    }
  };
  let class = gradient.as_ref().map(GradientInstance::class);
  // A rounded fill (`rounded_fill`): untextured (the kind has no
  // KIND_TEXTURED added), unstroked, a rounded rect or a plain rect under
  // the rounded clip. It carries its whole quad in `tex`, which the rounded
  // program measures the shape from.
  let fill = s == 0.0 && (kind == KIND_ROUNDED_RECT || (kind == KIND_RECT && state.rounded.is_some()));
  let tex = if fill { rect_array(&quad) } else { tex };
  let shape = instance(state, &p, clip, Quad { rect: quad, kind, stroke: s, color, tex, radii, rows });
  debug_assert_eq!(fill, rounded_fill(&shape));
  if fill && p.is_axis_aligned() && device_area(&shape) >= ROUNDED_SPLIT_AREA {
    let [m11, _, _, m22] = p.linear;
    // Non-zero: the device area is.
    let scale = m11.abs().min(m22.abs());
    let margin = ROUNDED_QUAD_PAD + ROUNDED_SPLIT_MARGIN / scale;
    let mut regions = Vec::with_capacity(2);
    if kind == KIND_ROUNDED_RECT {
      regions.push((*rect, radii));
    }
    if let Some(r) = state.rounded {
      regions.push(local_rounded(&p, &r));
    }
    let pieces = split_rounded(&quad, &regions, margin);
    // Without a plain piece (a pill, a near-circle: every cell is rim) the
    // split saves no fragment and only adds instances: the fill stays one
    // rounded instance, whose coverage on the dropped cells is 0 anyway.
    if !pieces.iter().any(|(_, plain)| *plain) {
      out.push(shape, texture, class, state.depth);
      return;
    }
    // The rim first, then the plain pieces: a grid of panels alternates a
    // rounded and a fill batch. The pieces do not overlap, so their order
    // within the op changes no pixel.
    for (piece, _) in pieces.iter().filter(|(_, plain)| !plain) {
      let rim = Quad { rect: *piece, kind, stroke: 0.0, color, tex, radii, rows };
      out.push(instance(state, &p, clip, rim), None, class, state.depth);
    }
    for (piece, _) in pieces.iter().filter(|(_, plain)| *plain) {
      let plain = Quad { rect: *piece, kind: KIND_RECT, stroke: 0.0, color, tex: [0.0; 4], radii: [0.0; 4], rows };
      out.push(instance_under(None, &p, clip, plain), None, class, state.depth);
    }
    return;
  }
  out.push(shape, texture, class, state.depth);
}

// `r` as `Instance::tex` carries a quad: x, y, w, h.
fn rect_array(r: &Rect) -> [f32; 4] {
  [r.origin.x, r.origin.y, r.size.width, r.size.height]
}

// The device rounded clip `r` in the local space of the axis-aligned 2D map
// `p`, whose axis scales are non-zero: its rect through the inverse map and
// its radii over the smaller scale (the larger local radius under a
// non-uniform scale, which only grows the rim), the corners swapped back
// under a mirrored axis, the mirror of `clip_rounded`.
fn local_rounded(p: &Projective, r: &RoundedClip) -> (Rect, Radii) {
  let [m11, _, _, m22] = p.linear;
  let [tx, ty] = p.offset;
  let local = |d: Point2D<f32>| Point::new((d.x - tx) / m11, (d.y - ty) / m22);
  let rect = Box2D::from_points([local(r.rect.min), local(r.rect.max)]).to_rect();
  let scale = m11.abs().min(m22.abs());
  let [tl, tr, br, bl] = r.radii.map(|radius| radius / scale);
  // Each swap is its own inverse, and the two commute.
  let [tl, tr, br, bl] = if m11 < 0.0 { [tr, tl, bl, br] } else { [tl, tr, br, bl] };
  let radii = if m22 < 0.0 { [bl, br, tr, tl] } else { [tl, tr, br, bl] };
  (rect, radii)
}

/// What a cell of a split rounded fill is, against one rounded region or
/// all of them.
#[derive(Clone, Copy, PartialEq)]
enum Cell {
  /// Coverage 0: dropped.
  Outside,
  /// Coverage exactly 1: a plain rect fill.
  Plain,
  /// Anything between: the rounded program.
  Rim,
}

// What the point `c` is against the rounded region `q` with the corner
// `radii` (CSS order) and the margin `m`: outside beyond `q` grown by `m` on
// any side; plain inside the region's plain cross, `q` inset by `m` less the
// four corner squares (the top-left one [q.left, q.left + tl + m] x [q.top,
// q.top + tl + m], and so on); the rim otherwise.
fn cell_of(c: Point, q: &Rect, radii: &Radii, m: f32) -> Cell {
  let (l, t, r, b) = (q.min_x(), q.min_y(), q.max_x(), q.max_y());
  if c.x < l - m || c.x > r + m || c.y < t - m || c.y > b + m {
    return Cell::Outside;
  }
  let [tl, tr, br, bl] = *radii;
  let inset = c.x > l + m && c.x < r - m && c.y > t + m && c.y < b - m;
  let corner = (c.x < l + tl + m && c.y < t + tl + m)
    || (c.x > r - tr - m && c.y < t + tr + m)
    || (c.x > r - br - m && c.y > b - br - m)
    || (c.x < l + bl + m && c.y > b - bl - m);
  if inset && !corner {
    Cell::Plain
  } else {
    Cell::Rim
  }
}

// The cuts one region adds per axis: its two edges less and plus the
// margin, and its two corners' extents at each end.
const CUTS_PER_REGION: usize = 8;

/// The rounded fill whose quad is `fill` (local space) split into pieces
/// against the rounded `regions` (each a rect and its corner radii in CSS
/// order, local space: the shape's own and the rounded clip's), with the
/// margin `m` in local units: each piece a rect and whether it is plain.
/// `fill` is cut at every region's x = left - m, left + m, left + tl + m,
/// left + bl + m, right - tr - m, right - br - m, right - m and right + m
/// that falls strictly inside it, and the same in y, into a grid whose
/// every line is an edge of some region's classes (`cell_of`), so a cell's
/// centre classifies all of it: plain when plain for every region, dropped
/// when outside any, the rim otherwise. Runs of one class along a row merge
/// into one piece; the dropped cells give none. The pieces tile `fill` less
/// the dropped cells, without overlap, in row order.
pub fn split_rounded(fill: &Rect, regions: &[(Rect, Radii)], m: f32) -> Vec<(Rect, bool)> {
  let mut xs = Vec::with_capacity(2 + CUTS_PER_REGION * regions.len());
  let mut ys = Vec::with_capacity(2 + CUTS_PER_REGION * regions.len());
  for (q, [tl, tr, br, bl]) in regions {
    let (l, t, r, b) = (q.min_x(), q.min_y(), q.max_x(), q.max_y());
    xs.extend([l - m, l + m, l + tl + m, l + bl + m, r - tr - m, r - br - m, r - m, r + m]);
    ys.extend([t - m, t + m, t + tl + m, t + tr + m, b - bl - m, b - br - m, b - m, b + m]);
  }
  let xs = grid_lines(xs, fill.min_x(), fill.max_x());
  let ys = grid_lines(ys, fill.min_y(), fill.max_y());
  let mut pieces = Vec::new();
  for row in ys.windows(2) {
    let (y0, y1) = (row[0], row[1]);
    // The current run: its left edge and its class.
    let mut run: Option<(f32, Cell)> = None;
    for column in xs.windows(2) {
      let (x0, x1) = (column[0], column[1]);
      let centre = Point::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
      let classes = regions.iter().map(|(q, radii)| cell_of(centre, q, radii, m));
      let cell = classes.fold(Cell::Plain, |all, one| match (all, one) {
        (Cell::Outside, _) | (_, Cell::Outside) => Cell::Outside,
        (Cell::Rim, _) | (_, Cell::Rim) => Cell::Rim,
        _ => Cell::Plain,
      });
      match run {
        Some((_, class)) if class == cell => {}
        _ => {
          if let Some((start, class)) = run {
            push_piece(&mut pieces, start, x0, y0, y1, class);
          }
          run = Some((x0, cell));
        }
      }
    }
    if let Some((start, class)) = run {
      push_piece(&mut pieces, start, fill.max_x(), y0, y1, class);
    }
  }
  pieces
}

// The sorted, distinct `cuts` strictly between `lo` and `hi`, with `lo` and
// `hi` at the ends.
fn grid_lines(mut cuts: Vec<f32>, lo: f32, hi: f32) -> Vec<f32> {
  cuts.retain(|c| *c > lo && *c < hi);
  cuts.push(lo);
  cuts.push(hi);
  cuts.sort_by(f32::total_cmp);
  cuts.dedup();
  cuts
}

// One merged run [x0, x1] x [y0, y1] of `class` into `pieces`, none for a
// dropped one.
fn push_piece(pieces: &mut Vec<(Rect, bool)>, x0: f32, x1: f32, y0: f32, y1: f32, class: Cell) {
  if class != Cell::Outside {
    pieces.push((Rect::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0)), class == Cell::Plain));
  }
}

// `image`'s map from the op's space to the normalized texel: its own map
// over the texture's size.
fn normalized_texel(image: &super::ops::Image) -> Affine {
  // A texture's size is never zero (see `push_texture`).
  let size = image.texture.size();
  let normalize = [[1.0 / size.width as f32, 0.0, 0.0], [0.0, 1.0 / size.height as f32, 0.0]];
  affine::compose(normalize, affine::of_2d(&image.to_texel))
}

// Whether `instance` is a plain rect fill, which the fill program draws:
// a rect, not textured, not stroked, under no rounded clip. Its coverage
// is 1 inside the quad, whose edges are MSAA's.
fn plain_fill(instance: &Instance) -> bool {
  let [_, _, kind, stroke] = instance.offset_kind_stroke;
  kind == KIND_RECT && stroke == 0.0 && instance.rclip_radii == [0.0; 4]
}

// Whether `instance` is a rounded fill, which the rounded program draws:
// not textured, not stroked, and either a rounded rect or a plain rect
// under a rounded clip (which `plain_fill` refuses). Its coverage is the
// rounded-box distance's, of the shape, of the clip, or both.
fn rounded_fill(instance: &Instance) -> bool {
  let [_, _, kind, stroke] = instance.offset_kind_stroke;
  let rect_under_clip = kind == KIND_RECT && instance.rclip_radii != [0.0; 4];
  stroke == 0.0 && (kind == KIND_ROUNDED_RECT || rect_under_clip)
}

// Whether `instance` is a rim piece of a split rounded rect: a rounded fill
// of the rounded rect kind whose quad is not its whole shape quad (`tex`).
// Only the rounded program measures the shape from `tex`.
fn rounded_piece(instance: &Instance) -> bool {
  instance.offset_kind_stroke[2] == KIND_ROUNDED_RECT && rounded_fill(instance) && instance.rect != instance.tex
}

// The device pixels `instance`'s quad covers, unclipped: under a 2D map its
// local area times the absolute determinant of the map, under perspective
// the area of its projected bounds, 0 when a corner is behind the eye.
fn device_area(instance: &Instance) -> f32 {
  let [x, y, w, h] = instance.rect;
  let [m11, m12, m21, m22] = instance.linear;
  let [p1, p2, p3, _] = instance.persp;
  let p = Projective {
    linear: instance.linear,
    offset: [instance.offset_kind_stroke[0], instance.offset_kind_stroke[1]],
    persp: [p1, p2, p3],
  };
  if p.is_affine() {
    return (m11 * m22 - m12 * m21).abs() * w * h;
  }
  p.bounds(&Rect::new(Point::new(x, y), Size::new(w, h))).map_or(0.0, |b| b.area())
}

// A segment stroked `width` wide under the current state, unless it has no
// width, the clip rejects it or it is fully transparent. The quad lives in
// the segment's own frame: x from `from` along the segment, y across it,
// extended by half the width at both ends for square and round caps. A
// zero-length segment is a width-square dot for those caps and nothing
// for butt caps, as Skia and Impeller draw it. A `gradient` is in the op's
// space, not the segment's frame, so its rows compose with the frame map
// (quad frame to op space); the line then takes the gradient's color in a
// batch of its gradient's kind and tile, its solid color past the ramp
// table's rows.
#[allow(clippy::too_many_arguments)]
fn push_line(
  state: &State,
  from: Point,
  to: Point,
  color: Premultiplied,
  width: f32,
  cap: StrokeCap,
  gradient: Option<&Arc<RampGradient>>,
  ramps: &mut RampTable,
  out: &mut Walked,
) {
  if width <= 0.0 || color[3] == 0.0 {
    return;
  }
  let (dx, dy) = (to.x - from.x, to.y - from.y);
  let len = (dx * dx + dy * dy).sqrt();
  let ext = match cap {
    StrokeCap::Butt => 0.0,
    StrokeCap::Square | StrokeCap::Round => width * 0.5,
  };
  let local = Rect::new(Point::new(-ext, -width * 0.5), Size::new(len + 2.0 * ext, width));
  // Row vectors: local (len, 0) rotates onto (dx, dy), then moves to `from`,
  // then takes the state's map.
  let line_map = Transform2D::rotation(Angle::radians(dy.atan2(dx))).then(&Transform2D::translation(from.x, from.y));
  let p = Projective::from_matrix(&line_map.to_3d().then(&state.matrix));
  let Some(clip) = placed_under(state, &p, &local) else { return };
  let tex = [cap_value(cap), len, 0.0, 0.0];
  let gradient =
    gradient.and_then(|g| gradient_instance(ramps, g, affine::compose([g.x_row, g.y_row], affine::of_2d(&line_map))));
  // The ramp sample is premultiplied in the program: the alpha fades it.
  let color = if gradient.is_some() { [color[3]; 4] } else { color };
  let rows = gradient_rows(gradient.as_ref());
  let quad = Quad { rect: local, kind: KIND_LINE, stroke: width, color, tex, radii: [0.0; 4], rows };
  let class = gradient.as_ref().map(GradientInstance::class);
  out.push(instance(state, &p, clip, quad), None, class, state.depth);
}

// The cap as the instance carries it.
fn cap_value(cap: StrokeCap) -> f32 {
  match cap {
    StrokeCap::Butt => CAP_BUTT,
    StrokeCap::Square => CAP_SQUARE,
    StrokeCap::Round => CAP_ROUND,
  }
}

// The texel rect `src` of `texture` mapped onto `dst` under the current
// state, faded by `alpha`, unless the clip rejects it or it is fully
// transparent. Texel row 0 is the top of `src` (offscreen lists are
// pre-flipped for it, a text layer's among them), so v is not flipped:
// the top edge of `dst` samples `src.min_y()`.
fn push_texture(
  state: &State,
  texture: &Texture,
  src: &Rect,
  dst: &Rect,
  sampling: TextureSampling,
  alpha: f32,
  out: &mut Walked,
) {
  let Some((p, clip)) = placed(state, dst) else { return };
  if alpha <= 0.0 {
    return;
  }
  // A texture's size is never zero: the registry refuses one, and a layer
  // is at least one texel.
  let size = texture.size();
  let (w, h) = (size.width as f32, size.height as f32);
  let tex = [src.min_x() / w, src.min_y() / h, src.max_x() / w, src.max_y() / h];
  // dst onto src, over the size: u = u0 + (x - dst.x) * (u1 - u0) / dst.w.
  let (su, sv) = ((tex[2] - tex[0]) / dst.size.width, (tex[3] - tex[1]) / dst.size.height);
  let rows = [[su, 0.0, tex[0] - dst.min_x() * su], [0.0, sv, tex[1] - dst.min_y() * sv]];
  let quad = Quad {
    rect: *dst,
    // A rect fill's coverage is 1 inside the quad: the texel as it is.
    kind: KIND_RECT + KIND_TEXTURED,
    stroke: 0.0,
    // The texel is premultiplied, so scaling all four channels by the
    // alpha fades it as a whole (the fragment stage multiplies).
    color: [alpha; 4],
    tex,
    radii: [0.0; 4],
    rows: texel_rows(rows),
  };
  out.push(instance(state, &p, clip, quad), Some((texture.name(), sampling)), None, state.depth);
}

// One glyph run under the current state: rejected whole when its alpha is
// zero or its `bounds` fall outside the clip, else one `KIND_GLYPH`
// instance per quad the clip keeps. A quad's rect is its `dst`, its uv its
// `src` over the atlas's size (v grows downward with the texel rows, as for
// a texture quad), its color the run's `color`, straight alpha. A
// `gradient` takes one ramp row per run and its own rows on every quad
// (the quads are in the op's space); past the ramp table's rows the run
// draws its solid color.
#[allow(clippy::too_many_arguments)]
fn push_glyphs(
  state: &State,
  atlas: &Texture,
  quads: &[GlyphQuad],
  bounds: &Rect,
  color: [f32; 4],
  policy: CoveragePolicy,
  gradient: Option<&Arc<RampGradient>>,
  ramps: &mut RampTable,
  out: &mut Walked,
) {
  if quads.is_empty() || color[3] == 0.0 || placed(state, bounds).is_none() {
    return;
  }
  let gradient = gradient.and_then(|g| gradient_instance(ramps, g, [g.x_row, g.y_row]));
  let rows = gradient_rows(gradient.as_ref());
  // An atlas's size is never zero, as for any texture (see `push_texture`).
  let size = atlas.size();
  let (w, h) = (size.width as f32, size.height as f32);
  let mut instances = Vec::with_capacity(quads.len());
  for q in quads {
    let dst = Rect::new(Point::new(q.dst[0], q.dst[1]), Size::new(q.dst[2], q.dst[3]));
    let Some((p, clip)) = placed(state, &dst) else { continue };
    let [sx, sy, sw, sh] = q.src;
    let quad = Quad {
      rect: dst,
      kind: KIND_GLYPH,
      stroke: 0.0,
      color,
      tex: [sx / w, sy / h, (sx + sw) / w, (sy + sh) / h],
      radii: [0.0; 4],
      rows,
    };
    instances.push(instance(state, &p, clip, quad));
  }
  out.push_glyphs(instances, atlas.name(), policy, state.depth);
}

// A paint shader's quad over `rect` under the current state, unless the
// clip rejects it: one `KIND_PAINT` instance whose `tex` is the unit square
// (the program's `vUV` is the quad's corner, not read from it), in a batch
// of its own that carries the shader, its params and inputs, and the quad's
// device size: the device lengths of the rect's x and y edges under the
// map's linear part (under perspective an approximation).
fn push_paint(
  state: &State,
  rect: &Rect,
  shader: &PaintShader,
  params: &Arc<[ShaderParam]>,
  inputs: &Arc<[ShaderInput]>,
  out: &mut Walked,
) {
  let Some((p, clip)) = placed(state, rect) else { return };
  let [m11, m12, m21, m22] = p.linear;
  let resolution = [rect.size.width * m11.hypot(m12), rect.size.height * m21.hypot(m22)];
  let quad = Quad {
    rect: *rect,
    kind: KIND_PAINT,
    stroke: 0.0,
    // Unread: the program writes its own color.
    color: [1.0; 4],
    tex: [0.0, 0.0, 1.0, 1.0],
    radii: [0.0; 4],
    rows: NO_ROWS,
  };
  let paint = PaintDraw { shader: shader.clone(), params: params.clone(), inputs: inputs.clone(), resolution };
  out.push_paint(instance(state, &p, clip, quad), paint, state.depth);
}

// A path's mesh under `style` and the current state, unless the map
// collapses it, the clip rejects it or it is fully transparent. Rejected
// twice: before the mesh exists by the path's control-point box outset by
// the stroke's reach (half the width times the larger of the miter limit
// and sqrt 2: a miter's tip, or a square cap's corner), after by the
// mesh's own box; a box with a corner behind the eye is not culled (the
// GPU clips the homogeneous vertices) and takes the state's clip box. The
// mesh comes from the path's cache at the scale bucket of the map's 2D
// part (mesh.rs; under perspective an approximation), tessellated on a
// miss, and is drawn by handle in the path's space under the state's map,
// one draw of its own. An image `source` carries its texture and the
// path-space-to-texel rows, a gradient `source` its own rows over the
// path's space; either fades by the color's alpha (in all four channels),
// a gradient past the ramp table's rows draws the solid color.
fn push_path(
  state: &State,
  path: &Path,
  style: MeshStyle,
  color: Premultiplied,
  source: Option<&Source>,
  ramps: &mut RampTable,
  out: &mut Walked,
) {
  let p = Projective::from_matrix(&state.matrix);
  let Some(bucket) = scale_bucket(&p.affine_part()) else { return };
  let Some(bounds) = path.bounds() else { return };
  let reach = match style {
    MeshStyle::Fill => 0.0,
    MeshStyle::Stroke { width, miter, .. } => width * 0.5 * miter.max(SQRT_2),
  };
  if placed_under(state, &p, &bounds.inflate(reach, reach).to_rect()).is_none() {
    return;
  }
  if color[3] == 0.0 {
    return;
  }
  let (mesh, missed) = path.mesh(style, bucket);
  out.tessellated += missed as u32;
  let Some(mesh) = mesh else { return };
  let Some(clip) = placed_under(state, &p, &mesh.bounds.to_rect()) else { return };
  let mut texture = None;
  let mut texel = None;
  let mut gradient = None;
  let color = match source {
    Some(Source::Image(image)) => {
      texture = Some((image.texture.name(), image.sampling));
      texel = Some(normalized_texel(image));
      // The texel is premultiplied: the alpha in all four channels fades it.
      [color[3]; 4]
    }
    // The path's space is the op's: the gradient's own rows.
    Some(Source::Gradient(g)) => match gradient_instance(ramps, g, [g.x_row, g.y_row]) {
      Some(draw) => {
        gradient = Some(draw);
        // The ramp sample is premultiplied in the program: the alpha fades it.
        [color[3]; 4]
      }
      None => color,
    },
    None => color,
  };
  out.push_mesh(MeshDraw { mesh, map: p, color, clip, rounded: state.rounded, gradient, texture, texel }, state.depth);
  out.meshes += 1;
}
