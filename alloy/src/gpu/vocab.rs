//! The draw-state vocabulary and its parsers: the typed words every GPU
//! extension adds to (formats, topologies, blend modes, depth state, stages,
//! uniform kinds) and the value/descriptor types built from them. Callers
//! parse strings at their own boundary, so an invalid word fails at the call
//! site, not on the raster thread. The same boundary rule drives the
//! validators at the bottom: params, sampler bindings, and draw counts are
//! checked against reflected/mirrored state where the app made the mistake.

use std::collections::{HashMap, HashSet};

/// A shader uniform value as supplied from the app: a scalar or a flat
/// component array. The shader's own declaration decides how components are
/// dispatched (vec2/vec3/vec4/mat4, float or int scalar) - the value only
/// carries numbers, matched against the reflected uniform type at render.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
  Scalar(f32),
  Array(Vec<f32>),
}

impl ParamValue {
  /// The value's components as one flat slice (a scalar is one component).
  pub fn components(&self) -> &[f32] {
    match self {
      ParamValue::Scalar(v) => std::slice::from_ref(v),
      ParamValue::Array(a) => a.as_slice(),
    }
  }
}

/// A float vertex attribute's shape within the interleaved vertex buffer.
#[derive(Clone, Copy, Debug)]
pub enum AttrFormat {
  F32,
  Vec2,
  Vec3,
  Vec4,
}

impl AttrFormat {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "f32" => AttrFormat::F32,
      "vec2" => AttrFormat::Vec2,
      "vec3" => AttrFormat::Vec3,
      "vec4" => AttrFormat::Vec4,
      _ => return Err(format!("unsupported attribute format '{s}' (expected f32|vec2|vec3|vec4)")),
    })
  }

  pub(crate) fn components(self) -> i32 {
    match self {
      AttrFormat::F32 => 1,
      AttrFormat::Vec2 => 2,
      AttrFormat::Vec3 => 3,
      AttrFormat::Vec4 => 4,
    }
  }

  /// The string form `parse` accepts, for reporting the layout back out.
  pub fn name(self) -> &'static str {
    match self {
      AttrFormat::F32 => "f32",
      AttrFormat::Vec2 => "vec2",
      AttrFormat::Vec3 => "vec3",
      AttrFormat::Vec4 => "vec4",
    }
  }
}

/// Blending applied to a pipeline's mesh draw. Absent (the default) the draw
/// overwrites: overlapping geometry resolves by depth or draw order, never by
/// accumulation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlendMode {
  /// glBlendFunc(ONE, ONE): every fragment adds onto what is already in the
  /// target. Order-independent, so geometry needs no sorting - the additive
  /// half of translucency (point splats, glow passes).
  Add,
}

pub fn parse_blend(s: &str) -> Result<Option<BlendMode>, String> {
  Ok(match s {
    "none" => None,
    "add" => Some(BlendMode::Add),
    _ => return Err(format!("unsupported blend mode '{s}' (expected none|add)")),
  })
}

/// The string form `parse_blend` accepts, for reporting back out.
pub fn blend_name(b: Option<BlendMode>) -> &'static str {
  match b {
    None => "none",
    Some(BlendMode::Add) => "add",
  }
}

/// The element type of an index buffer, WebGPU's two formats: uint16 halves
/// index bandwidth and addresses meshes up to 65535 vertices, uint32 covers
/// the rest. (ES 3.0's uint8 indices are deliberately not offered; WebGPU
/// has no such format.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexFormat {
  U16,
  U32,
}

impl IndexFormat {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "uint16" => IndexFormat::U16,
      "uint32" => IndexFormat::U32,
      _ => return Err(format!("unsupported index format '{s}' (expected uint16|uint32)")),
    })
  }

  /// The string form `parse` accepts, for reporting back out.
  pub fn name(self) -> &'static str {
    match self {
      IndexFormat::U16 => "uint16",
      IndexFormat::U32 => "uint32",
    }
  }

  /// Bytes per index, the element stride of the index buffer.
  pub fn size(self) -> i32 {
    match self {
      IndexFormat::U16 => 2,
      IndexFormat::U32 => 4,
    }
  }

  pub(crate) fn gl(self) -> u32 {
    match self {
      IndexFormat::U16 => glow::UNSIGNED_SHORT,
      IndexFormat::U32 => glow::UNSIGNED_INT,
    }
  }
}

/// Which triangle faces a pipeline's draws discard. Winding is fixed:
/// counter-clockwise AS DISPLAYED = front, WebGPU's framebuffer-space rule.
/// Because the displayed image is the y flip of GL window space, that pins
/// glFrontFace to CW (see `run_pass`) - the choice that makes standard
/// meshes drawn with the pipeline path's usual y negation cull intuitively.
/// Absent (the default) both faces raster: the two-sided fallback open
/// surfaces need, which a closed mesh pays for in doubled fragment work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CullMode {
  Back,
  Front,
}

pub fn parse_cull(s: &str) -> Result<Option<CullMode>, String> {
  Ok(match s {
    "none" => None,
    "back" => Some(CullMode::Back),
    "front" => Some(CullMode::Front),
    _ => return Err(format!("unsupported cull mode '{s}' (expected none|back|front)")),
  })
}

/// The string form `parse_cull` accepts, for reporting back out.
pub fn cull_name(c: Option<CullMode>) -> &'static str {
  match c {
    None => "none",
    Some(CullMode::Back) => "back",
    Some(CullMode::Front) => "front",
  }
}

impl CullMode {
  pub(crate) fn gl(self) -> u32 {
    match self {
      CullMode::Back => glow::BACK,
      CullMode::Front => glow::FRONT,
    }
  }
}

/// How a pipeline's vertices assemble into primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
  Points,
  Lines,
  LineStrip,
  Triangles,
  TriangleStrip,
}

impl Topology {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "points" => Topology::Points,
      "lines" => Topology::Lines,
      "line-strip" => Topology::LineStrip,
      "triangles" => Topology::Triangles,
      "triangle-strip" => Topology::TriangleStrip,
      _ => return Err(format!("unsupported topology '{s}'")),
    })
  }

  /// The string form `parse` accepts, for reporting back out.
  pub fn name(self) -> &'static str {
    match self {
      Topology::Points => "points",
      Topology::Lines => "lines",
      Topology::LineStrip => "line-strip",
      Topology::Triangles => "triangles",
      Topology::TriangleStrip => "triangle-strip",
    }
  }

  pub(crate) fn gl(self) -> u32 {
    match self {
      Topology::Points => glow::POINTS,
      Topology::Lines => glow::LINES,
      Topology::LineStrip => glow::LINE_STRIP,
      Topology::Triangles => glow::TRIANGLES,
      Topology::TriangleStrip => glow::TRIANGLE_STRIP,
    }
  }
}

/// Depth state for a pipeline's draws. Present means every target gets a
/// private depth buffer and the draw tests against it; `write` is whether the
/// draw also writes it (the clear always does). "Test without write" is the
/// blended-pass half an app opts into explicitly; "write without test" does
/// not exist, which is exactly why this is an Option of a struct and not two
/// booleans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepthState {
  pub write: bool,
}

/// The draw-state half of a render pipeline: everything about HOW a program
/// draws (vertex layout, primitive assembly, blending, depth), as opposed to
/// where it draws (the target's size, buffer, and clear are per-target).
/// Vocabulary is typed here - callers parse strings at their own boundary
/// (`AttrFormat::parse`, `Topology::parse`, `parse_blend`), so an invalid
/// word fails at the call site, not on the raster thread.
#[derive(Clone, Debug)]
pub struct PipelineDesc {
  /// One interleaved float vertex, in buffer order: (attribute name, format).
  /// Empty for attributeless rendering driven by gl_VertexID.
  pub attributes: Vec<(String, AttrFormat)>,
  /// One interleaved per-INSTANCE record, in buffer order (WebGPU's stepMode
  /// "instance"): fetched from the entry's instance buffer with vertex
  /// divisor 1, so every vertex of an instance reads the same record and the
  /// record advances per instance. Names share the vertex-attribute
  /// namespace (each is one `in` of the vertex stage), so a name in both
  /// lists is rejected at pipeline creation. Empty = no per-instance fetch;
  /// instances then differ only through gl_InstanceID.
  pub instance_attributes: Vec<(String, AttrFormat)>,
  pub topology: Topology,
  /// None = overwrite (the default); see `BlendMode`.
  pub blend: Option<BlendMode>,
  pub depth: Option<DepthState>,
  /// None = both faces raster (the default); see `CullMode`.
  pub cull: Option<CullMode>,
}

impl Default for PipelineDesc {
  fn default() -> Self {
    PipelineDesc {
      attributes: Vec::new(),
      instance_attributes: Vec::new(),
      topology: Topology::Triangles,
      blend: None,
      depth: None,
      cull: None,
    }
  }
}

/// Byte stride of one interleaved record for the given attribute list - a
/// vertex of `attributes`, or an instance record of `instance_attributes`.
pub fn vertex_stride(attributes: &[(String, AttrFormat)]) -> i32 {
  attributes.iter().map(|(_, f)| f.components() * 4).sum()
}

/// The GLSL type of one active uniform, reflected once at link time and
/// mirrored UI-side (see `UniformTable`) so uniform writes validate at the
/// call site. The settable set matches the dispatch in `pass::apply_uniform`;
/// everything else reflects as `Other` and errors when named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniformKind {
  Float,
  Int,
  Bool,
  Vec2,
  Vec3,
  Vec4,
  Mat4,
  /// Bound via texture bindings, never via params.
  Sampler2D,
  /// A reflected type outside the settable set (int vectors, matrices other
  /// than mat4, other sampler dimensions, ...).
  Other,
}

impl UniformKind {
  pub fn from_gl(utype: u32) -> Self {
    match utype {
      glow::FLOAT => UniformKind::Float,
      glow::INT => UniformKind::Int,
      glow::BOOL => UniformKind::Bool,
      glow::FLOAT_VEC2 => UniformKind::Vec2,
      glow::FLOAT_VEC3 => UniformKind::Vec3,
      glow::FLOAT_VEC4 => UniformKind::Vec4,
      glow::FLOAT_MAT4 => UniformKind::Mat4,
      glow::SAMPLER_2D => UniformKind::Sampler2D,
      _ => UniformKind::Other,
    }
  }

  /// Component count a param value must supply; None for kinds params cannot
  /// set (samplers, unsupported types).
  pub fn components(self) -> Option<usize> {
    match self {
      UniformKind::Float | UniformKind::Int | UniformKind::Bool => Some(1),
      UniformKind::Vec2 => Some(2),
      UniformKind::Vec3 => Some(3),
      UniformKind::Vec4 => Some(4),
      UniformKind::Mat4 => Some(16),
      UniformKind::Sampler2D | UniformKind::Other => None,
    }
  }

  /// The GLSL spelling, for error messages.
  pub fn glsl_name(self) -> &'static str {
    match self {
      UniformKind::Float => "float",
      UniformKind::Int => "int",
      UniformKind::Bool => "bool",
      UniformKind::Vec2 => "vec2",
      UniformKind::Vec3 => "vec3",
      UniformKind::Vec4 => "vec4",
      UniformKind::Mat4 => "mat4",
      UniformKind::Sampler2D => "sampler2D",
      UniformKind::Other => "an unsupported type",
    }
  }
}

/// A program's active uniforms by name: the plain-data half of the reflection
/// `ShaderProgram` holds, crossing to the UI thread in create/link replies so
/// `Context` can validate uniform writes without an RPC. Note the caveat this
/// inherits from GL reflection: a uniform that is declared but optimized out
/// (inactive) is absent here, so setting it reports "no active uniform" -
/// remove the write or use the uniform.
pub type UniformTable = HashMap<String, UniformKind>;

fn unknown_uniform(uniforms: &UniformTable, name: &str) -> String {
  let mut names: Vec<&str> = uniforms.keys().map(|s| s.as_str()).collect();
  names.sort_unstable();
  if names.is_empty() {
    format!("no active uniform named '{name}' (the program has none)")
  } else {
    format!("no active uniform named '{name}' (active: {})", names.join(", "))
  }
}

/// Check a params list against a program's active uniforms: every name must
/// be active, settable (not a sampler - those bind via textures - and not an
/// unsupported type), and carry exactly the component count its declared type
/// dispatches on. Run at the call-site boundary (create RPCs raster-side,
/// updates UI-side from the mirror), so a typo'd name or a wrong arity throws
/// on the line that wrote it instead of warning on the raster thread.
pub fn validate_params(uniforms: &UniformTable, params: &[(String, ParamValue)]) -> Result<(), String> {
  for (name, value) in params {
    let kind = uniforms.get(name).ok_or_else(|| unknown_uniform(uniforms, name))?;
    match kind.components() {
      Some(expected) => {
        let got = value.components().len();
        if got != expected {
          return Err(format!(
            "param '{name}' has {got} component(s), but uniform is {} (expects {expected})",
            kind.glsl_name()
          ));
        }
      }
      None => {
        return Err(match kind {
          UniformKind::Sampler2D => format!("param '{name}' is a sampler2D; bind it via textures"),
          _ => format!("param '{name}' has an unsupported uniform type (settable: float, int, bool, vec2/3/4, mat4)"),
        })
      }
    }
  }
  Ok(())
}

/// Check a sampler-binding list against a program's active uniforms: every
/// name must be an active `sampler2D`. Same boundary rule as
/// `validate_params`.
pub fn validate_texture_bindings(uniforms: &UniformTable, textures: &[(String, u64)]) -> Result<(), String> {
  for (name, _) in textures {
    let kind = uniforms.get(name).ok_or_else(|| unknown_uniform(uniforms, name))?;
    if *kind != UniformKind::Sampler2D {
      return Err(format!("uniform '{name}' is {}, not a sampler2D", kind.glsl_name()));
    }
  }
  Ok(())
}

/// The draw parameters of one pipeline target: which vertices are drawn
/// (`[first_vertex, first_vertex + vertex_count)`, WebGPU's `firstVertex` /
/// `vertexCount`) and how many instances the range is drawn as. On an INDEXED
/// entry the same two fields count indices instead (WebGPU's `firstIndex` /
/// `indexCount`, the JS surface's spelling there too); the entry's index
/// binding decides the unit, never the range itself. One value because the
/// three numbers describe one draw call; targets mutate it as a unit via
/// `DrawUpdate`. `instance_count` 1 is the plain non-instanced draw; 0 draws
/// nothing (a cheap off switch, as in WebGPU). Note `gl_VertexID` includes
/// `first_vertex` (GL and WebGPU agree; on an indexed draw it reads the index
/// value) and `gl_InstanceID` always starts at 0 - ES 3.0 has no base
/// instance, and no base vertex either (glDrawElementsBaseVertex is ES 3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawRange {
  pub first_vertex: i32,
  /// Negative at the API boundary means "the rest of the buffer from
  /// `first_vertex` on"; `resolve_draw_range` replaces it with the concrete
  /// count before the range crosses to the raster thread.
  pub vertex_count: i32,
  /// Negative at the API boundary means "one instance per record of the
  /// entry's instance buffer" (1 when the entry has none - the plain draw);
  /// `resolve_draw_range` replaces it, like `vertex_count`.
  pub instance_count: i32,
}

impl Default for DrawRange {
  /// The create-time default: the whole buffer (see `vertex_count`), from
  /// vertex 0, at the derived instance count (see `instance_count`).
  fn default() -> Self {
    DrawRange { first_vertex: 0, vertex_count: -1, instance_count: -1 }
  }
}

impl DrawRange {
  /// This range with the update's present fields overwritten: the setDraw
  /// merge - absent fields keep their current value, like params. The update
  /// must speak the entry's vocabulary - firstVertex/vertexCount on plain
  /// entries, firstIndex/indexCount on indexed ones - so a range written in
  /// the wrong unit errors instead of silently counting the other thing.
  /// One copy of the rule for the single-draw setDraw and the per-entry
  /// setDrawRange.
  pub fn merged(self, update: DrawUpdate, indexed: bool) -> Result<DrawRange, String> {
    if indexed && (update.first_vertex.is_some() || update.vertex_count.is_some()) {
      return Err("the draw is indexed; use firstIndex/indexCount (the range counts indices)".to_string());
    }
    if !indexed && (update.first_index.is_some() || update.index_count.is_some()) {
      return Err("the draw has no index buffer; use firstVertex/vertexCount".to_string());
    }
    let first = if indexed { update.first_index } else { update.first_vertex };
    let count = if indexed { update.index_count } else { update.vertex_count };
    Ok(DrawRange {
      first_vertex: first.unwrap_or(self.first_vertex),
      vertex_count: count.unwrap_or(self.vertex_count),
      instance_count: update.instance_count.unwrap_or(self.instance_count),
    })
  }
}

/// A partial update to a target's `DrawRange` (the setDraw payload); `None`
/// fields keep their current value. Carries both spellings of the range -
/// the vertex-named pair for plain entries, the index-named pair for indexed
/// ones - and `DrawRange::merged` rejects the pair that does not match the
/// entry, so the marshalling layer stays mode-blind.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawUpdate {
  pub first_vertex: Option<i32>,
  pub vertex_count: Option<i32>,
  pub first_index: Option<i32>,
  pub index_count: Option<i32>,
  pub instance_count: Option<i32>,
}

/// The unit nouns of a fetch bound: what the range counts. Vertices through
/// the pipeline's stride on plain entries, indices through the index format's
/// element size on indexed ones - the bound math is identical either way.
fn fetch_nouns(indexed: bool) -> (&'static str, &'static str) {
  if indexed {
    ("index", "indices")
  } else {
    ("vertex", "vertices")
  }
}

/// The fetch bounds one entry's draw range is checked against, captured at
/// create from the mirrored buffer sizes. Sizes are fixed at creation and
/// the entry holds its buffers alive, so a captured bound stays correct for
/// the entry's lifetime even after a buffer id itself is destroyed. One
/// value because the update paths (set_draw/set_draw_range) revalidate the
/// merged range against all of it, synchronously, without an RPC.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawBounds {
  /// The range's fetch as (element stride, buffer byte size): the VERTEX
  /// buffer at the pipeline's stride on a plain entry, the INDEX buffer at
  /// the format's element size on an indexed one. None when the entry
  /// fetches nothing per vertex (attributeless and unindexed) - gl_VertexID
  /// fetches nothing, so any range is safe.
  pub fetch: Option<(usize, usize)>,
  /// Whether the range counts indices: picks the vocabulary the update
  /// paths accept (firstIndex/indexCount vs firstVertex/vertexCount) and
  /// the fetch bound's error nouns.
  pub indexed: bool,
  /// The per-instance fetch as (record stride, buffer byte size), present
  /// when the entry binds an instance buffer. Instances `[0,
  /// instance_count)` fetch one record each - there is no base instance -
  /// so `instance_count` bounds against this.
  pub instance: Option<(usize, usize)>,
}

/// Check a resolved draw range against the buffers it fetches from: every
/// field must be >= 0 and each fetch must stay within its buffer, or the
/// draw is undefined-behaviour fetch (raw GLES 3.0 has no draw-time bounds
/// check; WebGL made the same case INVALID_OPERATION). `bounds.fetch` bounds
/// `[first, first + count) * stride` - on an indexed entry that is the INDEX
/// buffer; the index VALUES are not checked against the vertex buffer, which
/// would mean reading them back. `bounds.instance` bounds `instance_count`
/// records. Runs UI-side at the call-site boundary: the create paths via
/// `resolve_draw_range` and the range updates against the mirrored bounds.
pub fn validate_draw_range(range: DrawRange, bounds: DrawBounds) -> Result<(), String> {
  let (noun, nouns) = fetch_nouns(bounds.indexed);
  if range.first_vertex < 0 {
    return Err(format!("first {noun} must be >= 0, got {}", range.first_vertex));
  }
  if range.vertex_count < 0 {
    return Err(format!("{noun} count must be >= 0, got {}", range.vertex_count));
  }
  if range.instance_count < 0 {
    return Err(format!("instance count must be >= 0, got {}", range.instance_count));
  }
  if let Some((stride, size)) = bounds.fetch {
    let end = range.first_vertex as usize + range.vertex_count as usize;
    let need = end * stride;
    if need > size {
      let capacity = size / stride;
      return Err(format!(
        "{noun} range {}..{end} needs {need} bytes at {stride} bytes/{noun}, but the buffer holds {size} bytes ({capacity} {nouns})",
        range.first_vertex
      ));
    }
  }
  if let Some((stride, size)) = bounds.instance {
    let need = range.instance_count as usize * stride;
    if need > size {
      let capacity = size / stride;
      return Err(format!(
        "{} instances need {need} bytes at {stride} bytes/instance, but the instance buffer holds {size} bytes ({capacity} instances)",
        range.instance_count
      ));
    }
  }
  Ok(())
}

/// Resolve a create-time draw range against its bounds: a negative
/// vertex/index count becomes "the rest of the buffer from `first` on" (0
/// when nothing is fetched), a negative instance count becomes "one instance
/// per instance-buffer record" (1 without an instance buffer - the plain
/// draw), and the result is validated like any explicit range. Runs UI-side
/// (Context owns the size/stride mirrors); the raster thread only ever sees
/// resolved ranges.
pub fn resolve_draw_range(mut range: DrawRange, bounds: DrawBounds) -> Result<DrawRange, String> {
  if range.vertex_count < 0 {
    range.vertex_count = match bounds.fetch {
      Some((stride, size)) => {
        let capacity = (size / stride) as i32;
        if range.first_vertex > capacity {
          let (noun, nouns) = fetch_nouns(bounds.indexed);
          return Err(format!(
            "first {noun} {} is past the end of the buffer ({capacity} {nouns})",
            range.first_vertex
          ));
        }
        capacity - range.first_vertex.max(0)
      }
      None => 0,
    };
  }
  if range.instance_count < 0 {
    range.instance_count = match bounds.instance {
      Some((stride, size)) => (size / stride) as i32,
      None => 1,
    };
  }
  validate_draw_range(range, bounds)?;
  Ok(range)
}

/// Check that `order` names every id in `current` exactly once - a full
/// permutation of a draw target's entry list, the set_draw_order contract.
/// One copy of the rule for the call-site check (against the UI mirror) and
/// the raster-side backstop.
pub fn validate_order(order: &[u64], current: impl ExactSizeIterator<Item = u64>) -> Result<(), String> {
  let count = current.len();
  if order.len() != count {
    return Err(format!("order lists {} draw(s) but the target has {count}", order.len()));
  }
  let set: HashSet<u64> = order.iter().copied().collect();
  if set.len() != order.len() {
    return Err("order names a draw more than once".to_string());
  }
  for id in current {
    if !set.contains(&id) {
      return Err(format!("order is missing draw {id}"));
    }
  }
  Ok(())
}

/// A stage of the programmable pipeline, for the raw compile path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShaderStage {
  Vertex,
  Fragment,
}

impl ShaderStage {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "vertex" => ShaderStage::Vertex,
      "fragment" => ShaderStage::Fragment,
      _ => return Err(format!("unsupported shader stage '{s}' (expected vertex|fragment)")),
    })
  }

  pub fn name(self) -> &'static str {
    match self {
      ShaderStage::Vertex => "vertex",
      ShaderStage::Fragment => "fragment",
    }
  }

  pub(crate) fn gl_kind(self) -> u32 {
    match self {
      ShaderStage::Vertex => glow::VERTEX_SHADER,
      ShaderStage::Fragment => glow::FRAGMENT_SHADER,
    }
  }
}
