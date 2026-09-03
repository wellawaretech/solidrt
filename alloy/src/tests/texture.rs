use crate::impellers::{Point, Rect, Size};
use crate::rendertree::{fit_rects, TextureFit};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn parts(r: Rect) -> (f32, f32, f32, f32) {
  (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

// A 200x100 source into a 100x100 box exercises every fit distinctly.
const SRC: (f32, f32, f32, f32) = (0.0, 0.0, 200.0, 100.0);
const DST: (f32, f32, f32, f32) = (10.0, 20.0, 100.0, 100.0);

fn run(fit: TextureFit) -> ((f32, f32, f32, f32), (f32, f32, f32, f32)) {
  let (s, d) = fit_rects(fit, rect(SRC.0, SRC.1, SRC.2, SRC.3), rect(DST.0, DST.1, DST.2, DST.3));
  (parts(s), parts(d))
}

#[test]
fn fill_passes_rects_through() {
  assert_eq!(run(TextureFit::Fill), (SRC, DST));
}

#[test]
fn cover_crops_source_centered() {
  // Scale = max(100/200, 100/100) = 1; visible source = 100x100, centered
  // horizontally in the 200-wide source; the destination box is filled.
  assert_eq!(run(TextureFit::Cover), ((50.0, 0.0, 100.0, 100.0), DST));
}

#[test]
fn contain_letterboxes_destination_centered() {
  // Scale = min(100/200, 100/100) = 0.5; drawn size = 100x50, centered
  // vertically in the box; the full source is shown.
  assert_eq!(run(TextureFit::Contain), (SRC, (10.0, 45.0, 100.0, 50.0)));
}

#[test]
fn none_draws_intrinsic_size_cropped() {
  // Scale 1: the 200-wide source is cropped to the 100-wide box, centered;
  // the 100-high axis fits exactly.
  assert_eq!(run(TextureFit::None), ((50.0, 0.0, 100.0, 100.0), DST));
}

#[test]
fn scale_down_acts_as_contain_when_source_is_larger() {
  assert_eq!(run(TextureFit::ScaleDown), run(TextureFit::Contain));
}

#[test]
fn scale_down_never_upscales() {
  // A 50x25 source in a 100x100 box: contain would scale by 2; scale-down
  // pins to 1 and centers at intrinsic size.
  let (s, d) = fit_rects(TextureFit::ScaleDown, rect(0.0, 0.0, 50.0, 25.0), rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(parts(s), (0.0, 0.0, 50.0, 25.0));
  assert_eq!(parts(d), (25.0, 37.5, 50.0, 25.0));
}

#[test]
fn none_centers_smaller_source_without_crop() {
  let (s, d) = fit_rects(TextureFit::None, rect(0.0, 0.0, 40.0, 20.0), rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(parts(s), (0.0, 0.0, 40.0, 20.0));
  assert_eq!(parts(d), (30.0, 40.0, 40.0, 20.0));
}

#[test]
fn cover_respects_source_crop_offset() {
  // A src_* crop (origin 20,10) stays the coordinate base for the sub-rect.
  let (s, d) = fit_rects(TextureFit::Cover, rect(20.0, 10.0, 200.0, 100.0), rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(parts(s), (70.0, 10.0, 100.0, 100.0));
  assert_eq!(parts(d), (0.0, 0.0, 100.0, 100.0));
}

#[test]
fn degenerate_sizes_pass_through() {
  let z = rect(0.0, 0.0, 0.0, 100.0);
  let d = rect(0.0, 0.0, 100.0, 100.0);
  assert_eq!((parts(z), parts(d)), {
    let (s, o) = fit_rects(TextureFit::Cover, z, d);
    (parts(s), parts(o))
  });
  let s2 = rect(0.0, 0.0, 100.0, 100.0);
  let z2 = rect(0.0, 0.0, 100.0, 0.0);
  assert_eq!((parts(s2), parts(z2)), {
    let (s, o) = fit_rects(TextureFit::Contain, s2, z2);
    (parts(s), parts(o))
  });
}

// The texture element is the one raster kind in the tree, and it reaches its
// paint through the same accessor the vector kinds do. Without this arm the
// property adapter never offers a paint prop to a texture and `blendMode`
// comes back as an unknown property.
#[test]
fn texture_exposes_its_paint() {
  use crate::impellers::BlendMode;
  use crate::rendertree::{Damage, ElementKind, Texture};

  let mut kind = ElementKind::Texture(Texture::default());
  let paint = kind.paint_mut().expect("texture kind exposes a paint");
  assert_eq!(paint.blend_mode, BlendMode::SourceOver);
  assert_eq!(paint.set_blend_mode(Some(BlendMode::Plus)), Damage::Paint);

  match &kind {
    ElementKind::Texture(tex) => assert_eq!(tex.paint.blend_mode, BlendMode::Plus),
    _ => panic!("kind changed"),
  }
}

// SamplerState::parse is the validation gate for the app-facing filter/wrap
// strings; defaults are linear + clamp on every creation path.
#[test]
fn sampler_state_parses_options_and_defaults() {
  use crate::gpu::texture::{SamplerFilter, SamplerOptions, SamplerState, SamplerWrap};

  let parse = |filter, wrap, mipmap, anisotropy| SamplerState::parse(&SamplerOptions { filter, wrap, mipmap, anisotropy });
  let state = parse(None, None, None, None).expect("defaults parse");
  assert_eq!(state, SamplerState { filter: SamplerFilter::Linear, wrap: SamplerWrap::Clamp, mipmap: false, anisotropy: 1 });

  let state = parse(Some("nearest"), Some("repeat"), Some(true), Some(8.0)).expect("explicit values parse");
  assert_eq!(state, SamplerState { filter: SamplerFilter::Nearest, wrap: SamplerWrap::Repeat, mipmap: true, anisotropy: 8 });

  let state = parse(Some("linear"), None, Some(false), None).expect("partial options parse");
  assert_eq!(state, SamplerState::default());

  assert!(parse(Some("bilinear"), None, None, None).expect_err("unknown filter rejected").contains("filter"));
  assert!(parse(None, Some("mirror"), None, None).expect_err("unknown wrap rejected").contains("wrap"));
}

// The anisotropy level is a wish the hardware quantizes: any number >= 1
// rounds down to a power of two and caps at 16, the engines' clamp-not-error
// semantics; below 1 or non-finite is the one rejected input.
#[test]
fn sampler_state_anisotropy_rounds_and_caps() {
  use crate::gpu::texture::{SamplerOptions, SamplerState};

  let parse = |a: f64| SamplerState::parse(&SamplerOptions { anisotropy: Some(a), ..SamplerOptions::default() });
  let level = |a: f64| parse(a).expect("level parses").anisotropy;
  assert_eq!(level(1.0), 1);
  assert_eq!(level(2.0), 2);
  assert_eq!(level(3.0), 2);
  assert_eq!(level(7.9), 4);
  assert_eq!(level(16.0), 16);
  assert_eq!(level(64.0), 16);
  assert_eq!(level(1.5), 1);

  for bad in [0.0, 0.5, -4.0, f64::NAN, f64::INFINITY] {
    assert!(parse(bad).expect_err("rejected").contains("anisotropy"));
  }
}

// Every SamplerState maps to its own cache slot: a collision would hand two
// states one GL sampler object and silently sample one of them wrong. No GL
// needed - the index is pure arithmetic over the enumeration `new` walks.
#[test]
fn sampler_cache_index_is_a_bijection() {
  use crate::gl::SamplerCache;
  use crate::gpu::texture::{SamplerFilter, SamplerState, SamplerWrap, ANISOTROPY_LEVELS, MIN_ANISOTROPY};

  let mut seen = vec![false; SamplerCache::COUNT];
  for filter in [SamplerFilter::Linear, SamplerFilter::Nearest] {
    for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
      for mipmap in [false, true] {
        for slot in 0..ANISOTROPY_LEVELS {
          let state = SamplerState { filter, wrap, mipmap, anisotropy: MIN_ANISOTROPY << slot };
          let i = SamplerCache::index(state);
          assert!(i < SamplerCache::COUNT, "{state:?} indexes past the cache");
          assert!(!seen[i], "{state:?} collides with an earlier state at slot {i}");
          seen[i] = true;
        }
      }
    }
  }
  assert!(seen.iter().all(|s| *s), "every slot is claimed exactly once");
}

// A per-binding override replaces only the fields it names; the mip flag is
// id state and never moves. `merge_bindings` replaces a named binding whole,
// so a rebind without an override drops the previous override.
#[test]
fn sampler_override_composes_and_merges() {
  use crate::gpu::texture::{SamplerFilter, SamplerOverride, SamplerState, SamplerWrap};
  use crate::gpu::{merge_bindings, TextureBinding};

  let own = SamplerState { filter: SamplerFilter::Nearest, wrap: SamplerWrap::Clamp, mipmap: true, anisotropy: 4 };
  let o = SamplerOverride::parse(Some("linear"), None).expect("filter-only override parses");
  assert_eq!(o, SamplerOverride { filter: Some(SamplerFilter::Linear), wrap: None });
  assert_eq!(
    own.overridden(&o),
    SamplerState { filter: SamplerFilter::Linear, wrap: SamplerWrap::Clamp, mipmap: true, anisotropy: 4 }
  );
  assert!(SamplerOverride::parse(None, None).expect("empty override parses").is_empty());
  assert_eq!(own.overridden(&SamplerOverride::default()), own);
  assert!(SamplerOverride::parse(Some("cubic"), None).expect_err("unknown filter rejected").contains("filter"));

  let mut record = vec![TextureBinding::new("uA", 1), TextureBinding { name: "uB".into(), id: 2, sampler: o }];
  merge_bindings(&mut record, &[TextureBinding::new("uB", 3), TextureBinding::new("uC", 4)]);
  assert_eq!(record.len(), 3);
  assert_eq!(record[1], TextureBinding::new("uB", 3));
  assert_eq!(record[2], TextureBinding::new("uC", 4));
}

// TextureFormat::parse is the app-facing format vocabulary gate and byte_len
// the sizing seam every upload validates through; the internal-only formats
// (rg8, depth24) stay out of the vocabulary.
#[test]
fn texture_format_parses_and_sizes() {
  use crate::gpu::texture::TextureFormat;

  assert_eq!(TextureFormat::parse(None).expect("default parses"), TextureFormat::Rgba8);
  assert_eq!(TextureFormat::parse(Some("r8")).expect("r8 parses"), TextureFormat::R8);
  assert_eq!(TextureFormat::parse(Some("r32f")).expect("r32f parses"), TextureFormat::R32f);
  assert_eq!(TextureFormat::parse(Some("rgba32f")).expect("rgba32f parses"), TextureFormat::Rgba32f);
  assert_eq!(TextureFormat::parse(Some("rgba16f")).expect("rgba16f parses"), TextureFormat::Rgba16f);
  assert_eq!(TextureFormat::parse(Some("rgba8-srgb")).expect("rgba8-srgb parses"), TextureFormat::Rgba8Srgb);
  assert!(TextureFormat::parse(Some("rg8")).is_err());
  assert!(TextureFormat::parse(Some("depth24")).is_err());

  assert_eq!(TextureFormat::Rgba8.byte_len(3, 5), 60);
  assert_eq!(TextureFormat::R8.byte_len(3, 5), 15);
  assert_eq!(TextureFormat::R32f.byte_len(3, 5), 60);
  assert_eq!(TextureFormat::Rgba32f.byte_len(3, 5), 240);
  assert_eq!(TextureFormat::Rgba16f.byte_len(3, 5), 120);
  assert_eq!(TextureFormat::Rgba8Srgb.byte_len(3, 5), 60);
}

// The half-float and sRGB formats sample like byte formats (RGBA16F is
// texture-filterable in core GLES 3.0, sRGB decodes before filtering):
// linear by default, mipmaps and anisotropy accepted. Only the 32-bit floats
// are nearest-only, and only the float formats and sRGB refuse readback.
#[test]
fn half_float_and_srgb_sample_like_byte_formats() {
  use crate::gpu::texture::{SamplerFilter, SamplerOptions, SamplerState, TextureFormat};

  for format in [TextureFormat::Rgba16f, TextureFormat::Rgba8Srgb] {
    let state = SamplerState::parse_for(format, &SamplerOptions { filter: None, wrap: None, mipmap: Some(true), anisotropy: Some(4.0) })
      .expect("linear, mipmap and anisotropy parse");
    assert_eq!(state.filter, SamplerFilter::Linear);
    assert!(state.mipmap);
    assert_eq!(state.anisotropy, 4);
    assert!(format.filterable());
    assert!(format.sample_only());
  }
  assert!(TextureFormat::Rgba16f.is_float());
  assert!(!TextureFormat::Rgba8Srgb.is_float());
  assert!(!TextureFormat::R32f.filterable());
  assert!(!TextureFormat::Rgba8.sample_only());
}

// f16_bytes packs the f32 payload bytes to native-endian halves: the
// canonical bit patterns of 1, -2, 0.5 and the largest finite half.
#[test]
fn f16_bytes_packs_halves() {
  use crate::gpu::texture::TextureFormat;

  let floats = [1.0f32, -2.0, 0.5, 65504.0];
  let mut f32_bytes = Vec::new();
  for v in floats {
    f32_bytes.extend_from_slice(&v.to_ne_bytes());
  }
  let packed = TextureFormat::f16_bytes(&f32_bytes);
  assert_eq!(packed.len(), 8);
  let halves: Vec<u16> = packed.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect();
  assert_eq!(halves, vec![0x3C00, 0xC000, 0x3800, 0x7BFF]);
}

// Float formats are nearest-only data textures (linear float filtering is
// not in core GLES 3.0): parse_for flips their filter default to nearest and
// refuses linear, mipmaps and anisotropy; byte formats resolve exactly as
// SamplerState::parse does.
#[test]
fn float_formats_sample_nearest_only() {
  use crate::gpu::texture::{SamplerFilter, SamplerOptions, SamplerState, TextureFormat};

  let parse = |format, filter, mipmap, anisotropy| {
    SamplerState::parse_for(format, &SamplerOptions { filter, wrap: None, mipmap, anisotropy })
  };
  let state = parse(TextureFormat::R32f, None, None, None).expect("float defaults parse");
  assert_eq!(state.filter, SamplerFilter::Nearest);
  let state = parse(TextureFormat::Rgba32f, Some("nearest"), None, None).expect("explicit nearest parses");
  assert_eq!(state.filter, SamplerFilter::Nearest);

  assert!(parse(TextureFormat::R32f, Some("linear"), None, None).expect_err("linear refused").contains("nearest-only"));
  assert!(parse(TextureFormat::Rgba32f, None, Some(true), None).expect_err("mipmap refused").contains("mip"));
  assert!(parse(TextureFormat::R32f, None, None, Some(4.0)).expect_err("anisotropy refused").contains("anisotropy"));

  let state = parse(TextureFormat::Rgba8, None, Some(true), Some(4.0)).expect("byte format unaffected");
  assert_eq!(state.filter, SamplerFilter::Linear);
  assert!(state.mipmap);
  assert_eq!(state.anisotropy, 4);
}
