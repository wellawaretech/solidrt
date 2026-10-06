// Compile the vendored msdfgen core (alloy/vendor/msdfgen, a submodule
// pinned to a release tag) and the C shim over it (csrc/msdf_shim.cpp)
// through cc: the glyph engine's distance-field generator
// (src/rendertree/text/glyphs/msdf.rs binds the shim). The core only: no
// ext/ (FreeType, Skia, libpng) since the outline comes from swash, and none
// of the core's image writers (save-*.cpp, export-svg.cpp) or its text shape
// format (shape-description.cpp). Portable C++, the same path on every
// target as forge's Basis build; Android links the NDK's shared C++ runtime
// that lattice/Makefile.android stages into every APK.

use std::path::PathBuf;

/// The msdfgen core sources compiled into alloy, relative to the submodule
/// root: every core/*.cpp but the file writers and the shape text format.
const MSDFGEN_SOURCES: &[&str] = &[
  "core/contour-combiners.cpp",
  "core/Contour.cpp",
  "core/convergent-curve-ordering.cpp",
  "core/DistanceMapping.cpp",
  "core/edge-coloring.cpp",
  "core/EdgeHolder.cpp",
  "core/edge-segments.cpp",
  "core/edge-selectors.cpp",
  "core/equation-solver.cpp",
  "core/msdf-error-correction.cpp",
  "core/MSDFErrorCorrection.cpp",
  "core/msdfgen.cpp",
  "core/Projection.cpp",
  "core/rasterization.cpp",
  "core/render-sdf.cpp",
  "core/Scanline.cpp",
  "core/sdf-error-estimation.cpp",
  "core/Shape.cpp",
];

/// The C surface the engine binds, relative to the crate root.
const MSDF_SHIM: &str = "csrc/msdf_shim.cpp";

fn main() {
  println!("cargo:rerun-if-changed=build.rs");
  let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
  let src = manifest_dir.join("vendor").join("msdfgen");
  println!("cargo:rerun-if-changed={}", src.join("core").display());
  println!("cargo:rerun-if-changed={}", src.join("msdfgen.h").display());
  println!("cargo:rerun-if-changed={}", manifest_dir.join(MSDF_SHIM).display());
  if !src.join("msdfgen.h").exists() {
    panic!(
      "alloy/vendor/msdfgen is empty; fetch the submodule first:\n  git submodule update --init alloy/vendor/msdfgen"
    );
  }
  let mut build = cc::Build::new();
  build
    .cpp(true)
    .std("c++17")
    .warnings(false)
    .flag_if_supported("-fvisibility=hidden")
    .define("NDEBUG", None)
    // The core's own build switches: C++11 move semantics on, and no DLL
    // export decoration (left undefined, base.h includes a generated
    // config header instead).
    .define("MSDFGEN_USE_CPP11", None)
    .define("MSDFGEN_PUBLIC", "")
    .include(&src);
  for file in MSDFGEN_SOURCES {
    build.file(src.join(file));
  }
  build.file(manifest_dir.join(MSDF_SHIM));
  build.compile("msdfgen");
}
