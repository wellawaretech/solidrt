//! The draw-state vocabulary and its parsers: the typed words every GPU
//! extension adds to (formats, topologies, blend modes, depth state, stages)
//! and the value/descriptor types built from them. Callers parse strings at
//! their own boundary, so an invalid word fails at the call site, not on the
//! raster thread.

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
  pub topology: Topology,
  /// None = overwrite (the default); see `BlendMode`.
  pub blend: Option<BlendMode>,
  pub depth: Option<DepthState>,
}

impl Default for PipelineDesc {
  fn default() -> Self {
    PipelineDesc { attributes: Vec::new(), topology: Topology::Triangles, blend: None, depth: None }
  }
}

/// Byte stride of one interleaved vertex for the given attribute list.
pub fn vertex_stride(attributes: &[(String, AttrFormat)]) -> i32 {
  attributes.iter().map(|(_, f)| f.components() * 4).sum()
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
