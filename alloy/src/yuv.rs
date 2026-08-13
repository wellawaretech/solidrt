// Planar YUV textures (see okf/backlog/video-playback.md): the layout and
// color vocabulary, the plane geometry of a tightly packed frame, and the
// YUV-to-RGB conversion fragment shader. Video-agnostic by design - any
// producer of packed YUV frames (video decoder, camera) can feed one.
//
// A YUV texture is a composition of existing texture-system primitives, wired
// by Context::create_yuv_texture: registry textures for the planes (R8/RG8,
// two sets, double buffered - see YuvGroup in context.rs) plus a shader
// target that samples them into the app-visible RGBA output. An upload moves
// one owned frame buffer to the raster thread (RasterCmd::UpdateYuv, no
// per-plane copies) and rebinds the target to the freshly written set, so
// re-render on upload and content damage propagation are the ordinary
// sampler-graph behavior, nothing YUV-specific. The planes being real
// registry ids is deliberate: exposing them (with the color constants) to
// app shaders is the designed-for postprocessing extension.
//
// Frames are TIGHTLY PACKED: plane rows are exactly the plane width, planes
// follow each other with no padding. Producers with padded output (decoder
// stride/slice-height) repack during the copy out of the decoder's buffer,
// where dropping the padding is free. Chroma dimensions round up, so odd
// frame sizes are legal (the last chroma column/row just covers one texel).

use crate::texture::TextureFormat;

/// Plane arrangement of a packed YUV 4:2:0 frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvLayout {
  /// Y plane, then one interleaved UV plane at half resolution (the
  /// MediaCodec buffer-mode output on the probed TV).
  Nv12,
  /// Y plane, then U, then V, each chroma plane at half resolution (the
  /// openh264 software decoder output).
  I420,
}

/// YUV-to-RGB conversion matrix. Absent stream metadata, the convention is
/// BT.709 for HD (720p and up) and BT.601 for SD; the caller decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvMatrix {
  Bt601,
  Bt709,
}

/// Sample range of the Y and chroma values. Video is almost always limited
/// (Y 16..235, chroma 16..240); full uses all 256 steps (JPEG-style).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvRange {
  Limited,
  Full,
}

/// One plane of a packed frame: its texture geometry and format, the uniform
/// the conversion shader samples it under, and its byte offset in the frame.
pub struct YuvPlane {
  pub name: &'static str,
  pub width: u32,
  pub height: u32,
  pub format: TextureFormat,
  pub offset: usize,
}

impl YuvPlane {
  pub fn byte_len(&self) -> usize {
    self.width as usize * self.height as usize * self.format.bytes_per_pixel()
  }
}

/// The planes of a tightly packed `layout` frame at display size
/// `width` x `height`, in frame order.
pub fn planes(layout: YuvLayout, width: u32, height: u32) -> Vec<YuvPlane> {
  let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
  let y_len = width as usize * height as usize;
  let c_len = cw as usize * ch as usize;
  match layout {
    YuvLayout::Nv12 => vec![
      YuvPlane { name: "uY", width, height, format: TextureFormat::R8, offset: 0 },
      YuvPlane { name: "uUV", width: cw, height: ch, format: TextureFormat::Rg8, offset: y_len },
    ],
    YuvLayout::I420 => vec![
      YuvPlane { name: "uY", width, height, format: TextureFormat::R8, offset: 0 },
      YuvPlane { name: "uU", width: cw, height: ch, format: TextureFormat::R8, offset: y_len },
      YuvPlane { name: "uV", width: cw, height: ch, format: TextureFormat::R8, offset: y_len + c_len },
    ],
  }
}

/// Total byte length of one packed frame.
pub fn frame_size(layout: YuvLayout, width: u32, height: u32) -> usize {
  planes(layout, width, height).iter().map(|p| p.byte_len()).sum()
}

/// The conversion coefficients as plain numbers, for consumers that convert
/// on the CPU or fuse conversion into their own shader (postprocessing
/// tier 2): `[y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u]` where
///   Y' = (y - y_offset) * y_scale,  C = (c - 128/255) * c_scale
///   R = Y' + r_v*Cr,  G = Y' + g_u*Cb + g_v*Cr,  B = Y' + b_u*Cb
pub fn coefficients(matrix: YuvMatrix, range: YuvRange) -> [f32; 7] {
  let (kr, kb) = match matrix {
    YuvMatrix::Bt601 => (0.299f32, 0.114f32),
    YuvMatrix::Bt709 => (0.2126f32, 0.0722f32),
  };
  let kg = 1.0 - kr - kb;
  let (y_scale, y_offset, c_scale) = match range {
    YuvRange::Limited => (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0),
    YuvRange::Full => (1.0, 0.0, 1.0),
  };
  let r_v = 2.0 * (1.0 - kr);
  let b_u = 2.0 * (1.0 - kb);
  let g_u = -2.0 * kb * (1.0 - kb) / kg;
  let g_v = -2.0 * kr * (1.0 - kr) / kg;
  [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u]
}

/// The conversion pass fragment shader for a `layout` frame, color constants
/// baked in (they are fixed per stream; a change of standard recreates the
/// texture). Body-only source for `Context::create_shader_texture`.
pub fn fragment_src(layout: YuvLayout, matrix: YuvMatrix, range: YuvRange) -> String {
  let [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u] = coefficients(matrix, range);
  let (samplers, chroma) = match layout {
    YuvLayout::Nv12 => ("uniform sampler2D uY;\nuniform sampler2D uUV;", "texture(uUV, vUV).rg"),
    YuvLayout::I420 => (
      "uniform sampler2D uY;\nuniform sampler2D uU;\nuniform sampler2D uV;",
      "vec2(texture(uU, vUV).r, texture(uV, vUV).r)",
    ),
  };
  format!(
    "{samplers}
void main() {{
  float y = (texture(uY, vUV).r - {y_offset:.7}) * {y_scale:.7};
  vec2 c = ({chroma} - vec2({c_off:.7})) * {c_scale:.7};
  vec3 rgb = vec3(y + {r_v:.7} * c.y, y + {g_u:.7} * c.x + {g_v:.7} * c.y, y + {b_u:.7} * c.x);
  fragColor = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}}
",
    c_off = 128.0 / 255.0,
  )
}
