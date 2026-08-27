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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

  /// The format of a linked program's active attribute by its GL type;
  /// None for types no pipeline layout can feed (matrices, integer vectors).
  pub fn from_gl(atype: u32) -> Option<Self> {
    Some(match atype {
      glow::FLOAT => AttrFormat::F32,
      glow::FLOAT_VEC2 => AttrFormat::Vec2,
      glow::FLOAT_VEC3 => AttrFormat::Vec3,
      glow::FLOAT_VEC4 => AttrFormat::Vec4,
      _ => return None,
    })
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
  /// glBlendFunc(DST_COLOR, ZERO): every fragment scales what is already in
  /// the target, all four channels. Order-independent like Add; the darkening
  /// counterpart (a projected shadow, a dust pass). On the premultiplied
  /// target a uniform factor across rgb and alpha is a fade of the existing
  /// pixels; alpha 1 with rgb below 1 darkens color only.
  Multiply,
  /// glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA): the fragment composites OVER what
  /// is in the target, premultiplied like every target pixel. The one
  /// order-DEPENDENT mode: the result follows draw-list order, so translucent
  /// geometry must be drawn back-to-front by whoever orders the list.
  Alpha,
}

pub fn parse_blend(s: &str) -> Result<Option<BlendMode>, String> {
  Ok(match s {
    "none" => None,
    "add" => Some(BlendMode::Add),
    "multiply" => Some(BlendMode::Multiply),
    "alpha" => Some(BlendMode::Alpha),
    _ => return Err(format!("unsupported blend mode '{s}' (expected none|add|multiply|alpha)")),
  })
}

/// The string form `parse_blend` accepts, for reporting back out.
pub fn blend_name(b: Option<BlendMode>) -> &'static str {
  match b {
    None => "none",
    Some(BlendMode::Add) => "add",
    Some(BlendMode::Multiply) => "multiply",
    Some(BlendMode::Alpha) => "alpha",
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
  /// The per-INSTANCE attributes as (name, format, buffer slot): fetched
  /// from the entry's instance buffer for that slot with vertex divisor 1,
  /// so every vertex of an instance reads the same record and the record
  /// advances per instance. Attributes sharing a slot interleave into one
  /// record in declaration order (WebGPU's stepMode "instance"); distinct
  /// slots are distinct buffers with their own strides, which is what lets
  /// two writers own instance data independently (a core-written pose
  /// buffer beside a JS-written style buffer). Slots must be dense from 0
  /// and below `MAX_INSTANCE_SLOTS` (`validate_instance_slots`). Names
  /// share the vertex-attribute namespace (each is one `in` of the vertex
  /// stage), so a name in both lists is rejected at pipeline creation.
  /// Empty = no per-instance fetch; instances then differ only through
  /// gl_InstanceID.
  pub instance_attributes: Vec<(String, AttrFormat, u32)>,
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

/// The most instance buffer slots a pipeline may declare. A hard engine
/// cap so per-slot state stays fixed-size (and `Copy`) everywhere; two is
/// the designed-for case (a core-owned pose buffer beside a JS-owned
/// style buffer), four leaves headroom.
pub const MAX_INSTANCE_SLOTS: usize = 4;

/// Per-slot byte strides of a pipeline's instance attributes (0 = the slot
/// is unused). Callers index it by an entry's instance-buffer slot.
pub fn instance_strides(attributes: &[(String, AttrFormat, u32)]) -> [usize; MAX_INSTANCE_SLOTS] {
  let mut strides = [0usize; MAX_INSTANCE_SLOTS];
  for (_, f, slot) in attributes {
    if let Some(s) = strides.get_mut(*slot as usize) {
      *s += f.components() as usize * 4;
    }
  }
  strides
}

/// The slot contract of `PipelineDesc::instance_attributes`: every slot
/// below `MAX_INSTANCE_SLOTS`, and the used slots dense from 0 (a gap
/// would be an entry buffer nothing reads). Checked at both pipeline
/// creates, so a bad layout throws at its call site.
pub fn validate_instance_slots(attributes: &[(String, AttrFormat, u32)]) -> Result<(), String> {
  let mut used = [false; MAX_INSTANCE_SLOTS];
  for (name, _, slot) in attributes {
    let i = *slot as usize;
    if i >= MAX_INSTANCE_SLOTS {
      return Err(format!("instance attribute '{name}' uses buffer slot {slot}; slots are 0..{MAX_INSTANCE_SLOTS}"));
    }
    used[i] = true;
  }
  let count = used.iter().rposition(|&u| u).map_or(0, |p| p + 1);
  for (i, &u) in used[..count].iter().enumerate() {
    if !u {
      return Err(format!("instance buffer slots must be dense from 0: slot {i} has no attributes"));
    }
  }
  Ok(())
}

/// The GLSL element type of one active uniform, reflected once at link time
/// (see `UniformSlot`, which pairs it with the declared array size). The
/// settable set matches the dispatch in `pass::apply_uniform`; everything
/// else reflects as `Other` and errors when named.
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
  /// Declared in the source but optimized out by the compiler, so GL
  /// reflects nothing for it: writes are accepted and skipped (with a
  /// warning) rather than rejected as unknown names.
  Inactive,
  /// A reflected type outside the settable set (int vectors, matrices other
  /// than mat4, other sampler dimensions, ...), carrying the raw GL type
  /// enum for diagnostics.
  Other(u32),
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
      _ => UniformKind::Other(utype),
    }
  }

  /// Component count of one element of this kind; None for kinds params
  /// cannot set (samplers, unsupported types).
  pub fn components(self) -> Option<usize> {
    match self {
      UniformKind::Float | UniformKind::Int | UniformKind::Bool => Some(1),
      UniformKind::Vec2 => Some(2),
      UniformKind::Vec3 => Some(3),
      UniformKind::Vec4 => Some(4),
      UniformKind::Mat4 => Some(16),
      UniformKind::Sampler2D | UniformKind::Inactive | UniformKind::Other(_) => None,
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
      UniformKind::Inactive => "declared but inactive",
      UniformKind::Other(_) => "an unsupported type",
    }
  }
}

/// One active uniform as reflected at link time: its element kind and its
/// declared array size (1 for a non-array declaration; GL reports a declared
/// `vec3 u[4]` as element type vec3 with size 4 under the name `u[0]`, and
/// reflection strips the suffix). The single currency both call-site
/// validation and the raster-side dispatch compute from, so the two cannot
/// disagree on what a value must look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformSlot {
  pub kind: UniformKind,
  pub count: usize,
}

impl UniformSlot {
  /// Total component count a param value must supply: element components
  /// times array size, as one flat array. None for kinds params cannot set.
  pub fn components(self) -> Option<usize> {
    self.kind.components().map(|per| per * self.count)
  }

  /// The GLSL spelling with the array suffix when declared as one, for
  /// error messages: `vec3` or `vec3[4]`.
  pub fn glsl_name(self) -> String {
    if self.count > 1 {
      format!("{}[{}]", self.kind.glsl_name(), self.count)
    } else {
      self.kind.glsl_name().to_string()
    }
  }
}

/// A program's active uniforms by name: the plain-data half of the reflection
/// `ShaderProgram` holds, crossing to the UI thread in create/link replies so
/// `Context` can validate uniform writes without an RPC. Array uniforms
/// appear under their bare name (the reflected `[0]` suffix is stripped).
/// GL reflection only sees active uniforms, so a uniform that is declared
/// but optimized out is listed here from a source scan instead, as an
/// `Inactive` slot: writing it warns and is skipped, where a name that was
/// never declared throws.
pub type UniformTable = HashMap<String, UniformSlot>;

/// A linked program's active vertex attributes (name, format), reflected at
/// link time - what a pipeline's attribute lists must cover. Names appear
/// in GL's reported order.
pub type AttributeTable = Vec<(String, AttrFormat)>;

fn unknown_uniform(uniforms: &UniformTable, name: &str) -> String {
  let mut names: Vec<&str> =
    uniforms.iter().filter(|(_, slot)| slot.kind != UniformKind::Inactive).map(|(s, _)| s.as_str()).collect();
  names.sort_unstable();
  if names.is_empty() {
    format!("no active uniform named '{name}' (the program has none)")
  } else {
    format!("no active uniform named '{name}' (active: {})", names.join(", "))
  }
}

/// Check one param against a program's active uniforms, tolerating absence:
/// Ok(false) when the name is not active (the shared-params partial-coverage
/// rule - the render-side apply skips undeclared names), Ok(true) when it is
/// active, settable (not a sampler - those bind via textures - and not an
/// unsupported type), and carries exactly the component count its declared
/// type dispatches on; Err when it is active but fails either check.
pub fn validate_param_if_declared(uniforms: &UniformTable, name: &str, value: &ParamValue) -> Result<bool, String> {
  let Some(slot) = uniforms.get(name) else { return Ok(false) };
  if slot.kind == UniformKind::Inactive {
    return Ok(false);
  }
  match slot.components() {
    Some(expected) => {
      let got = value.components().len();
      if got != expected {
        return Err(format!(
          "param '{name}' has {got} component(s), but uniform is {} (expects {expected})",
          slot.glsl_name()
        ));
      }
    }
    None => {
      return Err(match slot.kind {
        UniformKind::Sampler2D => format!("param '{name}' is a sampler2D; bind it via textures"),
        _ => format!("param '{name}' has an unsupported uniform type (settable: float, int, bool, vec2/3/4, mat4, and arrays of these)"),
      })
    }
  }
  Ok(true)
}

/// Check a params list against a program's uniforms: every name must pass
/// `validate_param_if_declared`, a declared-but-inactive name warns (the
/// write is skipped at apply time), and absence is an error. Run at the
/// call-site boundary (create RPCs raster-side, updates UI-side from the
/// mirror), so a typo'd name or a wrong arity throws on the line that wrote
/// it instead of warning on the raster thread.
pub fn validate_params(uniforms: &UniformTable, params: &[(String, ParamValue)]) -> Result<(), String> {
  for (name, value) in params {
    if !validate_param_if_declared(uniforms, name, value)? {
      if is_inactive(uniforms, name) {
        warn_inactive(name);
        continue;
      }
      return Err(unknown_uniform(uniforms, name));
    }
  }
  Ok(())
}

fn is_inactive(uniforms: &UniformTable, name: &str) -> bool {
  uniforms.get(name).is_some_and(|slot| slot.kind == UniformKind::Inactive)
}

fn warn_inactive(name: &str) {
  log::warn!("[shader] uniform '{name}' is declared but inactive (optimized out); the write is ignored");
}

/// One sampler2D input of a pass: the uniform name, the source texture id,
/// and an optional per-binding sampling override (see `SamplerOverride`).
/// The binding-list merge rule is by name (new names append, existing names
/// are replaced whole, override included).
#[derive(Clone, Debug, PartialEq)]
pub struct TextureBinding {
  pub name: String,
  pub id: u64,
  pub sampler: crate::gpu::SamplerOverride,
}

impl TextureBinding {
  pub fn new(name: impl Into<String>, id: u64) -> Self {
    TextureBinding { name: name.into(), id, sampler: crate::gpu::SamplerOverride::default() }
  }
}

/// Fold a binding update into a record by name (new names append, existing
/// names are replaced whole).
pub fn merge_bindings(record: &mut Vec<TextureBinding>, updates: &[TextureBinding]) {
  for b in updates {
    match record.iter_mut().find(|r| r.name == b.name) {
      Some(existing) => *existing = b.clone(),
      None => record.push(b.clone()),
    }
  }
}

/// Check a sampler-binding list against a program's active uniforms: every
/// name must be an active non-array `sampler2D` (a binding names one texture
/// unit; sampler arrays are outside the settable set). Same boundary rule as
/// `validate_params`.
pub fn validate_texture_bindings(uniforms: &UniformTable, textures: &[TextureBinding]) -> Result<(), String> {
  for TextureBinding { name, .. } in textures {
    let slot = uniforms.get(name).ok_or_else(|| unknown_uniform(uniforms, name))?;
    if slot.kind == UniformKind::Inactive {
      warn_inactive(name);
      continue;
    }
    if slot.kind != UniformKind::Sampler2D || slot.count > 1 {
      return Err(format!("uniform '{name}' is {}, not a sampler2D", slot.glsl_name()));
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

/// A partial update to a draw entry (the setDraw / setDrawRange payload);
/// `None` fields keep their current value. The range half carries both
/// spellings - the vertex-named pair for plain entries, the index-named pair
/// for indexed ones - and `DrawRange::merged` rejects the pair that does not
/// match the entry, so the marshalling layer stays mode-blind. The buffer
/// half (`buffers`) swaps the entry's buffers; `Context::update_draw`
/// applies both halves as one validated transaction.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawUpdate {
  pub first_vertex: Option<i32>,
  pub vertex_count: Option<i32>,
  pub first_index: Option<i32>,
  pub index_count: Option<i32>,
  pub instance_count: Option<i32>,
  pub buffers: BufferUpdate,
}

/// The registry ids of one draw entry's buffers by role (vertex, index with
/// its element format, per-instance); 0 / None = the entry fills no such
/// role. What `BufferUpdate` merges into, and what the swap carries to the
/// raster thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferIds {
  pub buffer: u64,
  pub index: Option<(u64, IndexFormat)>,
  /// One buffer per instance slot of the pipeline (0 = the slot is unused;
  /// used slots are dense from 0, mirroring the layout contract).
  pub instance_buffers: [u64; MAX_INSTANCE_SLOTS],
}

impl BufferIds {
  /// These ids with the update's present fields replaced: the setDrawBuffers
  /// merge. Replace-only - which roles an entry fills is pipeline layout
  /// state (attributes, instanceAttributes) and the index binding is the
  /// entry's draw vocabulary, so a present field must name a role the entry
  /// already fills and a buffer must be nonzero.
  pub fn merged(self, update: BufferUpdate) -> Result<BufferIds, String> {
    let mut next = self;
    if let Some(id) = update.buffer {
      if self.buffer == 0 {
        return Err("the entry has no vertex buffer (the pipeline declares no attributes)".to_string());
      }
      if id == 0 {
        return Err("buffer must be a buffer id".to_string());
      }
      next.buffer = id;
    }
    if let Some((id, format)) = update.index {
      if self.index.is_none() {
        return Err("the entry is not indexed; an index buffer cannot be added after creation".to_string());
      }
      if id == 0 {
        return Err("indexBuffer must be a buffer id".to_string());
      }
      next.index = Some((id, format));
    }
    if let Some(id) = update.instance_buffer {
      if self.instance_buffers[0] == 0 {
        return Err("the entry has no instance buffer (the pipeline declares no instanceAttributes)".to_string());
      }
      if id == 0 {
        return Err("instanceBuffer must be a buffer id".to_string());
      }
      next.instance_buffers[0] = id;
    }
    if let Some(ids) = update.instance_buffers {
      for (slot, (&cur, &id)) in self.instance_buffers.iter().zip(ids.iter()).enumerate() {
        if (cur == 0) != (id == 0) {
          return Err(format!(
            "instanceBuffers must cover exactly the pipeline's instance slots; slot {slot} {}",
            if cur == 0 { "is not declared" } else { "cannot be dropped" }
          ));
        }
      }
      next.instance_buffers = ids;
    }
    Ok(next)
  }

  /// The nonzero ids, for "which targets read this buffer" bookkeeping.
  pub fn reads(&self, id: u64) -> bool {
    id != 0 && (self.buffer == id || self.instance_buffers.contains(&id) || self.index.is_some_and(|(i, _)| i == id))
  }
}

/// A partial update to an entry's buffers (the setDrawBuffers payload);
/// `None` fields keep their current buffer. See `BufferIds::merged` for the
/// replace-only rule.
#[derive(Clone, Copy, Debug, Default)]
pub struct BufferUpdate {
  pub buffer: Option<u64>,
  pub index: Option<(u64, IndexFormat)>,
  /// Swap the slot-0 instance buffer (the single-slot common case).
  pub instance_buffer: Option<u64>,
  /// Swap every instance slot at once; must fill exactly the slots the
  /// entry fills (see `BufferIds::merged`). Applied after
  /// `instance_buffer` when both are present, so pass one or the other.
  pub instance_buffers: Option<[u64; MAX_INSTANCE_SLOTS]>,
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
  /// The per-instance fetch as (record stride, buffer byte size) per
  /// instance slot (stride 0 = the slot is unused). Instances `[0,
  /// instance_count)` fetch one record from EVERY slot - there is no base
  /// instance - so `instance_count` bounds against the tightest slot
  /// (`instance_limit`).
  pub instances: [(usize, usize); MAX_INSTANCE_SLOTS],
}

impl DrawBounds {
  /// The binding slot with the fewest whole records - what the instance
  /// count derives from and validates against - as (stride, byte size).
  /// None when the entry fetches nothing per instance.
  pub fn instance_limit(&self) -> Option<(usize, usize)> {
    self.instances.iter().filter(|(stride, _)| *stride > 0).min_by_key(|(stride, size)| size / stride).copied()
  }
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
  if let Some((stride, size)) = bounds.instance_limit() {
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
    range.instance_count = match bounds.instance_limit() {
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
