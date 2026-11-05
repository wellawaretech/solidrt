// macOS: link clang's compiler-rt builtins archive into every artifact that
// links ore. The statically built SDL holds @available checks (the macOS 26
// mouse position fallback in SDL_cocoawindow.m) that compile to
// ___isPlatformVersionAtLeast calls. That symbol lives in libclang_rt.osx.a,
// which rustc's -nodefaultlibs link never pulls in; a plain release build
// gets it from Rust's compiler-builtins, but fat LTO (release-opt)
// internalizes that copy away since no Rust code references it. Ore is the
// crate that brings SDL in, so the link lives here and reaches every binary
// over it (the client, the runtime, headless flux) without a per-binary flag.

fn main() {
  println!("cargo:rerun-if-changed=build.rs");
  let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
  if target_os == "macos" {
    link_macos_builtins();
  }
}

/// Ask the target C compiler (the one that built SDL) for its resource
/// directory and link the darwin builtins archive in it.
fn link_macos_builtins() {
  let compiler = cc::Build::new().get_compiler();
  let output = compiler
    .to_command()
    .arg("-print-resource-dir")
    .output()
    .expect("failed to run the target C compiler to locate libclang_rt.osx.a");
  if !output.status.success() {
    panic!("-print-resource-dir failed: {}", String::from_utf8_lossy(&output.stderr));
  }

  let resource_dir = String::from_utf8(output.stdout).expect("non-UTF-8 compiler output");
  let dir = std::path::Path::new(resource_dir.trim()).join("lib").join("darwin");
  if !dir.join("libclang_rt.osx.a").is_file() {
    panic!("libclang_rt.osx.a not found in {}", dir.display());
  }

  println!("cargo:rustc-link-search=native={}", dir.display());
  println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
