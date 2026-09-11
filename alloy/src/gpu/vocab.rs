//! The draw-state vocabulary and its parsers: the typed words every GPU
//! extension adds to (formats, topologies, blend modes, depth state, stages,
//! uniform kinds) and the value/descriptor types built from them. Callers
//! parse strings at their own boundary, so an invalid word fails at the call
//! site, not on the raster thread. The same boundary rule drives the
//! validators at the bottom: params, sampler bindings, and draw counts are
//! checked against reflected/mirrored state where the app made the mistake.

use std::collections::{HashMap, HashSet};
use crate::gpu::texture::{TextureFormat, TextureShape};

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

/// The byte format of one vertex attribute within an interleaved record:
/// WebGPU's `GPUVertexFormat` spelling of the (component type, count,
/// normalized) triple the GL attribute pointer takes. Every format is a
/// multiple of 4 bytes, so offsets and strides are 4-aligned by
/// construction and no padding rule exists; that is why the 8-bit formats
/// come in x4 only and the 16-bit ones in x2 and x4. Every format feeds a
/// FLOAT-typed shader `in` (the pointer converts on fetch, normalized or
/// not), so the shader side of the vocabulary stays `float`/`vec2`/`vec3`/
/// `vec4` and a format matches an `in` by component count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrFormat {
  Float32,
  Float32x2,
  Float32x3,
  Float32x4,
  Float16x2,
  Float16x4,
  Unorm8x4,
  Snorm8x4,
  Unorm16x2,
  Unorm16x4,
  Snorm16x2,
  Snorm16x4,
  Uint8x4,
  Uint16x2,
  Uint16x4,
}

/// One row of the format table: the spelling, the component count, the
/// GL component type and whether the pointer normalizes integers.
struct AttrSpec {
  format: AttrFormat,
  name: &'static str,
  components: i32,
  gl_type: u32,
  normalized: bool,
}

const ATTR_FORMATS: [AttrSpec; 15] = [
  AttrSpec { format: AttrFormat::Float32, name: "float32", components: 1, gl_type: glow::FLOAT, normalized: false },
  AttrSpec { format: AttrFormat::Float32x2, name: "float32x2", components: 2, gl_type: glow::FLOAT, normalized: false },
  AttrSpec { format: AttrFormat::Float32x3, name: "float32x3", components: 3, gl_type: glow::FLOAT, normalized: false },
  AttrSpec { format: AttrFormat::Float32x4, name: "float32x4", components: 4, gl_type: glow::FLOAT, normalized: false },
  AttrSpec { format: AttrFormat::Float16x2, name: "float16x2", components: 2, gl_type: glow::HALF_FLOAT, normalized: false },
  AttrSpec { format: AttrFormat::Float16x4, name: "float16x4", components: 4, gl_type: glow::HALF_FLOAT, normalized: false },
  AttrSpec { format: AttrFormat::Unorm8x4, name: "unorm8x4", components: 4, gl_type: glow::UNSIGNED_BYTE, normalized: true },
  AttrSpec { format: AttrFormat::Snorm8x4, name: "snorm8x4", components: 4, gl_type: glow::BYTE, normalized: true },
  AttrSpec { format: AttrFormat::Unorm16x2, name: "unorm16x2", components: 2, gl_type: glow::UNSIGNED_SHORT, normalized: true },
  AttrSpec { format: AttrFormat::Unorm16x4, name: "unorm16x4", components: 4, gl_type: glow::UNSIGNED_SHORT, normalized: true },
  AttrSpec { format: AttrFormat::Snorm16x2, name: "snorm16x2", components: 2, gl_type: glow::SHORT, normalized: true },
  AttrSpec { format: AttrFormat::Snorm16x4, name: "snorm16x4", components: 4, gl_type: glow::SHORT, normalized: true },
  AttrSpec { format: AttrFormat::Uint8x4, name: "uint8x4", components: 4, gl_type: glow::UNSIGNED_BYTE, normalized: false },
  AttrSpec { format: AttrFormat::Uint16x2, name: "uint16x2", components: 2, gl_type: glow::UNSIGNED_SHORT, normalized: false },
  AttrSpec { format: AttrFormat::Uint16x4, name: "uint16x4", components: 4, gl_type: glow::UNSIGNED_SHORT, normalized: false },
];

impl AttrFormat {
  // The table is indexed by discriminant, so it must list the variants in
  // declaration order; the assertion catches a reordered row in tests.
  fn spec(self) -> &'static AttrSpec {
    let spec = &ATTR_FORMATS[self as usize];
    debug_assert_eq!(spec.format, self, "ATTR_FORMATS row order");
    spec
  }

  pub fn parse(s: &str) -> Result<Self, String> {
    ATTR_FORMATS.iter().find(|spec| spec.name == s).map(|spec| spec.format).ok_or_else(|| {
      let names: Vec<&str> = ATTR_FORMATS.iter().map(|spec| spec.name).collect();
      format!("unsupported attribute format '{s}' (expected one of {})", names.join("|"))
    })
  }

  /// The string form `parse` accepts, for reporting the layout back out.
  pub fn name(self) -> &'static str {
    self.spec().name
  }

  pub(crate) fn components(self) -> i32 {
    self.spec().components
  }

  /// Bytes one attribute of this format occupies in its record.
  pub fn bytes(self) -> i32 {
    let spec = self.spec();
    let component = match spec.gl_type {
      glow::FLOAT => 4,
      glow::HALF_FLOAT | glow::SHORT | glow::UNSIGNED_SHORT => 2,
      _ => 1,
    };
    spec.components * component
  }

  /// The GL component type the attribute pointer reads.
  pub(crate) fn gl_type(self) -> u32 {
    self.spec().gl_type
  }

  /// Whether the pointer maps integer components onto 0..1 / -1..1.
  pub(crate) fn normalized(self) -> bool {
    self.spec().normalized
  }

  /// The format of a linked program's active attribute by its GL type: the
  /// float form the shader declares, which any format of that component
  /// count may feed. None for types no pipeline layout can feed (matrices,
  /// integer vectors).
  pub fn from_gl(atype: u32) -> Option<Self> {
    Some(match atype {
      glow::FLOAT => AttrFormat::Float32,
      glow::FLOAT_VEC2 => AttrFormat::Float32x2,
      glow::FLOAT_VEC3 => AttrFormat::Float32x3,
      glow::FLOAT_VEC4 => AttrFormat::Float32x4,
      _ => return None,
    })
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

/// Whether a buffer's records advance per vertex or per instance: WebGPU's
/// `GPUVertexStepMode`. Per instance is GL's vertex divisor 1: every
/// vertex of instance N reads record N, so per-instance state rides in a
/// buffer, not in uniforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StepMode {
  #[default]
  Vertex,
  Instance,
}

impl StepMode {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "vertex" => StepMode::Vertex,
      "instance" => StepMode::Instance,
      _ => return Err(format!("unsupported stepMode '{s}' (expected vertex|instance)")),
    })
  }

  /// The string form `parse` accepts, for reporting back out.
  pub fn name(self) -> &'static str {
    match self {
      StepMode::Vertex => "vertex",
      StepMode::Instance => "instance",
    }
  }

  pub(crate) fn divisor(self) -> u32 {
    match self {
      StepMode::Vertex => 0,
      StepMode::Instance => 1,
    }
  }
}

/// One attribute of a buffer layout: the shader `in` it feeds (by name),
/// its byte format, and its byte offset within the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexAttr {
  pub name: String,
  pub format: AttrFormat,
  pub offset: i32,
}

/// The layout of one buffer a pipeline reads: WebGPU's
/// `GPUVertexBufferLayout`. A record of `stride` bytes per vertex or per
/// instance (`step`), holding `attributes` at their offsets. Attributes the
/// shader does not read are skipped over via the stride, and a layout may
/// name only some of a record's fields (an explicit stride wider than the
/// attributes' sum), which is how a depth pass reads positions alone out
/// of a full vertex record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferLayout {
  pub step: StepMode,
  pub stride: i32,
  pub attributes: Vec<VertexAttr>,
}

impl BufferLayout {
  /// A tightly packed layout from an ordered attribute list: offsets run
  /// in list order and the stride is their byte sum (the defaults the API
  /// boundary derives when a layout gives neither).
  pub fn packed(step: StepMode, attributes: Vec<(String, AttrFormat)>) -> Self {
    let mut offset = 0;
    let attributes = attributes
      .into_iter()
      .map(|(name, format)| {
        let attr = VertexAttr { name, format, offset };
        offset += format.bytes();
        attr
      })
      .collect();
    BufferLayout { step, stride: offset, attributes }
  }

  /// The tightly packed vertex-step layout: what most pipelines declare.
  pub fn vertex(attributes: Vec<(String, AttrFormat)>) -> Self {
    Self::packed(StepMode::Vertex, attributes)
  }

  /// The tightly packed instance-step layout.
  pub fn instance(attributes: Vec<(String, AttrFormat)>) -> Self {
    Self::packed(StepMode::Instance, attributes)
  }
}

/// The draw-state half of a render pipeline: everything about HOW a program
/// draws (buffer layouts, primitive assembly, blending, depth), as opposed
/// to where it draws (the target's size, buffer, and clear are per-target).
/// Vocabulary is typed here - callers parse strings at their own boundary
/// (`AttrFormat::parse`, `Topology::parse`, `parse_blend`), so an invalid
/// word fails at the call site, not on the raster thread.
#[derive(Clone, Debug)]
pub struct PipelineDesc {
  /// The buffers the vertex stage fetches from, one layout per buffer in
  /// binding order: an entry binds `buffers[i]` for layout i. Every
  /// attribute name across the list is one `in` of the vertex stage, so a
  /// name twice is rejected at pipeline creation. Empty for attributeless
  /// rendering driven by gl_VertexID; without an instance-step layout,
  /// instances differ only through gl_InstanceID.
  pub buffers: Vec<BufferLayout>,
  pub topology: Topology,
  /// None = overwrite (the default); see `BlendMode`.
  pub blend: Option<BlendMode>,
  pub depth: Option<DepthState>,
  /// None = both faces raster (the default); see `CullMode`.
  pub cull: Option<CullMode>,
}

impl Default for PipelineDesc {
  fn default() -> Self {
    PipelineDesc { buffers: Vec::new(), topology: Topology::Triangles, blend: None, depth: None, cull: None }
  }
}

impl PipelineDesc {
  /// Every attribute the layouts declare, in buffer then offset order.
  pub fn attribute_count(&self) -> usize {
    self.buffers.iter().map(|b| b.attributes.len()).sum()
  }
}

/// The most buffers a pipeline may declare. A hard engine cap so per-buffer
/// state stays fixed-size (and `Copy`) everywhere: a vertex stream or two
/// beside a pose buffer and a style buffer is the designed-for case, eight
/// leaves headroom.
pub const MAX_BUFFERS: usize = 8;

/// The stride and step of one declared buffer, what the UI-side mirrors
/// keep per pipeline to bound draw ranges without an RPC. Stride 0 = no
/// layout at that index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferStride {
  pub step: StepMode,
  pub stride: usize,
}

/// The per-index strides of a pipeline's buffers (see `BufferStride`).
pub fn buffer_strides(buffers: &[BufferLayout]) -> [BufferStride; MAX_BUFFERS] {
  let mut strides = [BufferStride::default(); MAX_BUFFERS];
  for (i, layout) in buffers.iter().enumerate().take(MAX_BUFFERS) {
    strides[i] = BufferStride { step: layout.step, stride: layout.stride as usize };
  }
  strides
}

/// The layout contract of `PipelineDesc::buffers`: at most `MAX_BUFFERS`
/// layouts, each with at least one attribute, a positive stride that is a
/// multiple of 4, attribute offsets that are multiples of 4 and keep the
/// attribute inside the record, and no attribute name twice across the
/// list (each name is one shader `in`). Checked at both pipeline creates,
/// so a bad layout throws at its call site.
pub fn validate_buffers(buffers: &[BufferLayout]) -> Result<(), String> {
  if buffers.len() > MAX_BUFFERS {
    return Err(format!("{} buffer layouts; a pipeline declares at most {MAX_BUFFERS}", buffers.len()));
  }
  let mut names: HashSet<&str> = HashSet::new();
  for (i, layout) in buffers.iter().enumerate() {
    if layout.attributes.is_empty() {
      return Err(format!("buffer layout {i} declares no attributes"));
    }
    if layout.stride <= 0 || layout.stride % 4 != 0 {
      return Err(format!("buffer layout {i} has arrayStride {}; a stride is a positive multiple of 4", layout.stride));
    }
    for attr in &layout.attributes {
      if attr.offset < 0 || attr.offset % 4 != 0 {
        return Err(format!("attribute '{}' has offset {}; an offset is a non-negative multiple of 4", attr.name, attr.offset));
      }
      if attr.offset + attr.format.bytes() > layout.stride {
        return Err(format!(
          "attribute '{}' ({} at offset {}) does not fit the {}-byte record of buffer layout {i}",
          attr.name,
          attr.format.name(),
          attr.offset,
          layout.stride
        ));
      }
      if !names.insert(attr.name.as_str()) {
        return Err(format!("attribute '{}' is declared twice; each name is one vertex-stage input", attr.name));
      }
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
  /// A comparison sampler (`sampler2DShadow`): bound via texture bindings
  /// like Sampler2D, but only a depth texture may back it, and the pass
  /// binds the comparison sampler object (LINEAR + LEQUAL compare, the
  /// hardware 2x2 PCF) instead of the texture's declared sampling.
  Sampler2DShadow,
  /// A cube map sampler (`samplerCube`): bound via texture bindings like
  /// Sampler2D, but only a cube map id (`TextureShape::Cube`) may back it,
  /// and the pass binds the id on the cube map target.
  SamplerCube,
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
      glow::SAMPLER_2D_SHADOW => UniformKind::Sampler2DShadow,
      glow::SAMPLER_CUBE => UniformKind::SamplerCube,
      _ => UniformKind::Other(utype),
    }
  }

  /// True for the kinds texture bindings serve (plain and comparison
  /// samplers alike); params can set neither.
  pub fn is_sampler(self) -> bool {
    matches!(self, UniformKind::Sampler2D | UniformKind::Sampler2DShadow | UniformKind::SamplerCube)
  }

  /// The texture shape a sampler kind binds; None for non-samplers.
  pub fn sampler_shape(self) -> Option<TextureShape> {
    match self {
      UniformKind::Sampler2D | UniformKind::Sampler2DShadow => Some(TextureShape::D2),
      UniformKind::SamplerCube => Some(TextureShape::Cube),
      _ => None,
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
      UniformKind::Sampler2D
      | UniformKind::Sampler2DShadow
      | UniformKind::SamplerCube
      | UniformKind::Inactive
      | UniformKind::Other(_) => None,
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
      UniformKind::Sampler2DShadow => "sampler2DShadow",
      UniformKind::SamplerCube => "samplerCube",
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
        k if k.is_sampler() => format!("param '{name}' is a {}; bind it via textures", k.glsl_name()),
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
    if !slot.kind.is_sampler() || slot.count > 1 {
      return Err(format!("uniform '{name}' is {}, not a sampler", slot.glsl_name()));
    }
  }
  Ok(())
}

/// What the shape rules need to know about a bound texture id: its shape
/// and its format, as the registry on either thread records them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundTexture {
  pub shape: TextureShape,
  pub format: TextureFormat,
}

/// The shape rules of a sampler-binding list, checked against what backs
/// each id (`lookup`; None = not registered, which the existence checks
/// elsewhere answer): a `samplerCube` takes a cube map and a 2D sampler
/// refuses one (a cube name cannot bind on the 2D target - the draw would
/// sample garbage or error), and a `sampler2DShadow` takes a depth texture
/// (a color texture behind a comparison sampler is undefined GL). The
/// reverse of the last stays legal - a depth id on a plain sampler2D is the
/// raw depth read. One copy of the rules for every bind path on both
/// threads: the UI-side rebinds answer from the entry registry, the fused
/// creates (whose uniform kinds only exist post-compile) from the raster
/// map. Runs after `validate_texture_bindings`, so every name here is an
/// active sampler.
pub fn validate_binding_shapes(
  uniforms: &UniformTable,
  textures: &[TextureBinding],
  lookup: impl Fn(u64) -> Option<BoundTexture>,
) -> Result<(), String> {
  for TextureBinding { name, id, .. } in textures {
    let Some(slot) = uniforms.get(name) else { continue };
    let Some(wanted) = slot.kind.sampler_shape() else { continue };
    let Some(bound) = lookup(*id) else { continue };
    if bound.shape != wanted {
      return Err(match wanted {
        TextureShape::Cube => {
          format!("uniform '{name}' is a samplerCube; texture {id} is a 2D texture (bind a cube map from createCubeTexture)")
        }
        TextureShape::D2 => {
          format!("texture {id} is a cube map; uniform '{name}' is a {} (declare it samplerCube)", slot.glsl_name())
        }
      });
    }
    if slot.kind == UniformKind::Sampler2DShadow && bound.format != TextureFormat::Depth24 {
      return Err(format!(
        "uniform '{name}' is a sampler2DShadow; bind a draw target's depth texture (depthTexture(target))"
      ));
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
  /// Replace an ordered entry's projected-key direction (the per-camera-move
  /// update; errs on an entry without a position-keyed instance order).
  /// UI-side state that shapes the NEXT lease publish - alone it sends
  /// nothing to the raster thread and re-renders nothing.
  pub order_direction: Option<[f32; 3]>,
}

/// The registry ids of one draw entry's buffers: one per declared layout
/// (0 = no buffer at that index) plus the index binding with its element
/// format (None = the entry draws unindexed). What `BufferUpdate` merges
/// into, and what the swap carries to the raster thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferIds {
  pub buffers: [u64; MAX_BUFFERS],
  pub index: Option<(u64, IndexFormat)>,
}

impl BufferIds {
  /// These ids with the update's present fields replaced: the setDrawBuffers
  /// merge. Replace-only - which buffers an entry binds is pipeline layout
  /// state and the index binding is the entry's draw vocabulary, so a
  /// buffers list must fill exactly the declared indices with nonzero ids
  /// and an index buffer can only replace one.
  pub fn merged(self, update: BufferUpdate) -> Result<BufferIds, String> {
    let mut next = self;
    if let Some(ids) = update.buffers {
      for (i, (&cur, &id)) in self.buffers.iter().zip(ids.iter()).enumerate() {
        if (cur == 0) != (id == 0) {
          return Err(format!(
            "buffers must cover exactly the pipeline's declared layouts; buffer {i} {}",
            if cur == 0 { "is not declared" } else { "cannot be dropped" }
          ));
        }
      }
      next.buffers = ids;
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
    Ok(next)
  }

  /// The nonzero ids, for "which targets read this buffer" bookkeeping.
  pub fn reads(&self, id: u64) -> bool {
    id != 0 && (self.buffers.contains(&id) || self.index.is_some_and(|(i, _)| i == id))
  }

  /// The bound buffers in layout order (the leading nonzero ids).
  pub fn bound(&self) -> impl Iterator<Item = u64> + '_ {
    self.buffers.iter().copied().take_while(|&id| id != 0)
  }
}

/// A partial update to an entry's buffers (the setDrawBuffers payload);
/// `None` fields keep their current buffer. See `BufferIds::merged` for the
/// replace-only rule.
#[derive(Clone, Copy, Debug, Default)]
pub struct BufferUpdate {
  /// Swap every declared buffer at once; must fill exactly the layouts
  /// the pipeline declares (see `BufferIds::merged`).
  pub buffers: Option<[u64; MAX_BUFFERS]>,
  pub index: Option<(u64, IndexFormat)>,
}

/// The unit nouns of a fetch bound: what the range counts. Vertices through
/// the vertex-step strides on plain entries, indices through the index
/// format's element size on indexed ones - the bound math is identical
/// either way.
fn fetch_nouns(indexed: bool) -> (&'static str, &'static str) {
  if indexed {
    ("index", "indices")
  } else {
    ("vertex", "vertices")
  }
}

/// The fetch bound of one bound buffer: its layout's step and stride and
/// the buffer's byte size (stride 0 = no buffer at that index).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferBound {
  pub step: StepMode,
  pub stride: usize,
  pub size: usize,
}

/// The fetch bounds one entry's draw range is checked against, captured at
/// create from the mirrored buffer sizes. Sizes are fixed at creation and
/// the entry holds its buffers alive, so a captured bound stays correct for
/// the entry's lifetime even after a buffer id itself is destroyed. One
/// value because the update paths (set_draw/set_draw_range) revalidate the
/// merged range against all of it, synchronously, without an RPC.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawBounds {
  /// Present when the entry is indexed: the INDEX buffer as (element
  /// size, byte size). The range then counts indices, and the vertex
  /// fetch runs through the index VALUES, which are not checked against
  /// the vertex buffers (that would mean reading them back).
  pub index: Option<(usize, usize)>,
  /// One bound per declared buffer layout (see `BufferBound`). Vertices
  /// `[first, first + count)` fetch one record from EVERY vertex-step
  /// buffer and instances `[0, instance_count)` one from every
  /// instance-step buffer - there is no base instance - so each count
  /// bounds against the tightest buffer of its step.
  pub buffers: [BufferBound; MAX_BUFFERS],
}

impl DrawBounds {
  /// Whether the range counts indices: picks the vocabulary the update
  /// paths accept (firstIndex/indexCount vs firstVertex/vertexCount) and
  /// the fetch bound's error nouns.
  pub fn indexed(&self) -> bool {
    self.index.is_some()
  }

  /// The buffer of `step` with the fewest whole records, as (stride, byte
  /// size); None when the entry fetches nothing at that step.
  fn tightest(&self, step: StepMode) -> Option<(usize, usize)> {
    self
      .buffers
      .iter()
      .filter(|b| b.stride > 0 && b.step == step)
      .map(|b| (b.stride, b.size))
      .min_by_key(|(stride, size)| size / stride)
  }

  /// What the range's first/count fetch is bounded by: the index buffer
  /// at its element size on an indexed entry, the tightest vertex-step
  /// buffer on a plain one. None when the entry fetches nothing per
  /// vertex (attributeless and unindexed) - gl_VertexID fetches nothing,
  /// so any range is safe.
  pub fn fetch(&self) -> Option<(usize, usize)> {
    self.index.or_else(|| self.tightest(StepMode::Vertex))
  }

  /// The instance-step buffer with the fewest whole records - what the
  /// instance count derives from and validates against - as (stride, byte
  /// size). None when the entry fetches nothing per instance.
  pub fn instance_limit(&self) -> Option<(usize, usize)> {
    self.tightest(StepMode::Instance)
  }
}

/// Check a resolved draw range against the buffers it fetches from: every
/// field must be >= 0 and each fetch must stay within its buffer, or the
/// draw is undefined-behaviour fetch (raw GLES 3.0 has no draw-time bounds
/// check; WebGL made the same case INVALID_OPERATION). `bounds.fetch()`
/// bounds `[first, first + count) * stride`; `bounds.instance_limit()`
/// bounds `instance_count` records. Runs UI-side at the call-site boundary:
/// the create paths via `resolve_draw_range` and the range updates against
/// the mirrored bounds.
pub fn validate_draw_range(range: DrawRange, bounds: DrawBounds) -> Result<(), String> {
  let (noun, nouns) = fetch_nouns(bounds.indexed());
  if range.first_vertex < 0 {
    return Err(format!("first {noun} must be >= 0, got {}", range.first_vertex));
  }
  if range.vertex_count < 0 {
    return Err(format!("{noun} count must be >= 0, got {}", range.vertex_count));
  }
  if range.instance_count < 0 {
    return Err(format!("instance count must be >= 0, got {}", range.instance_count));
  }
  if let Some((stride, size)) = bounds.fetch() {
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
    range.vertex_count = match bounds.fetch() {
      Some((stride, size)) => {
        let capacity = (size / stride) as i32;
        if range.first_vertex > capacity {
          let (noun, nouns) = fetch_nouns(bounds.indexed());
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
