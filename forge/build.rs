// Two target-dependent link jobs.
//
// Android: link clang's compiler-rt builtins archive into the final artifact.
// The bundled libffi calls __clear_cache (the aarch64/arm instruction-cache
// flush), which lives in libclang_rt.builtins-<arch>-android.a. The NDK clang
// driver links that archive implicitly for C code, but rustc links with
// -nodefaultlibs and Rust's compiler-builtins does not provide the symbol, so
// without this the cdylib carries an undefined __clear_cache that only
// surfaces as a dlopen failure on device. Ask the target C compiler (the NDK
// clang wrapper cargo-ndk points CC_<target> at) where its builtins archive
// is and link it explicitly.
//
// Everywhere, with the `video` feature: build the vendored libopus
// (forge/vendor/opus, a submodule pinned to a release tag) through its own
// CMake project and link it statically; the cmake crate picks up the
// cross toolchain cargo-ndk exports on Android, as it does for SDL.
//
// Everywhere but Android, with the `video` feature: build the vendored libvpx
// (forge/vendor/libvpx, a submodule pinned to a release tag) and link it
// statically. libvpx has its own configure + make (no cmake); the build runs
// out of tree in OUT_DIR so the checkout stays clean, and configure runs
// once (make is incremental after that; `cargo clean -p forge` after a
// submodule bump that changes the configure options). The VP9 DECODER only:
// the encoder costs 1.28 MB of linked binary (1732 KiB with it, 448 KiB
// without, measured x86_64 2026-09-12) and no amount of not calling it helps,
// because the RTCD tables name every dsp function and pull the encoder objects
// in regardless. Re-enable it in the same breath as the encoder ffi, not
// before. VP8 is not built, nor are the examples, tools and docs. Android
// needs none of this: its VP9 decoder is MediaCodec
// (forge/src/video/mediacodec.rs).

use std::path::PathBuf;
use std::process::Command;

fn main() {
  let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set");
  println!("cargo:rerun-if-changed=build.rs");
  let video = std::env::var_os("CARGO_FEATURE_VIDEO").is_some();
  if target_os == "android" {
    link_android_builtins();
  } else if video {
    build_libvpx(&target_os);
  }
  if video {
    build_libopus();
  }
}

fn build_libopus() {
  let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
  let src = manifest_dir.join("vendor").join("opus");
  println!("cargo:rerun-if-changed={}", src.display());
  if !src.join("CMakeLists.txt").exists() {
    panic!(
      "forge/vendor/opus is empty; fetch the opus submodule first:\n  git submodule update --init forge/vendor/opus"
    );
  }
  // The library only: no programs, tests, docs or install modules, and a
  // fixed lib dir so the link search path is the same on every distro.
  let dst = cmake::Config::new(&src)
    .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
    .define("OPUS_BUILD_PROGRAMS", "OFF")
    .define("OPUS_BUILD_TESTING", "OFF")
    .define("OPUS_INSTALL_PKG_CONFIG_MODULE", "OFF")
    .define("OPUS_INSTALL_CMAKE_CONFIG_MODULE", "OFF")
    .define("CMAKE_INSTALL_LIBDIR", "lib")
    .build();
  println!("cargo:rustc-link-search=native={}", dst.join("lib").display());
  println!("cargo:rustc-link-lib=static=opus");
}

fn link_android_builtins() {
  let compiler = cc::Build::new().get_compiler();
  let output = compiler
    .to_command()
    .arg("--print-libgcc-file-name")
    .output()
    .expect("failed to run the target C compiler to locate clang_rt.builtins");
  if !output.status.success() {
    panic!("--print-libgcc-file-name failed: {}", String::from_utf8_lossy(&output.stderr));
  }

  let path_str = String::from_utf8(output.stdout).expect("non-UTF-8 compiler output");
  let path = std::path::Path::new(path_str.trim());
  let dir = path.parent().expect("builtins archive path has no parent directory");
  let file = path.file_name().and_then(|name| name.to_str()).expect("builtins archive path has no file name");
  let lib = file
    .strip_prefix("lib")
    .and_then(|name| name.strip_suffix(".a"))
    .unwrap_or_else(|| panic!("unexpected builtins archive name: {file}"));

  println!("cargo:rustc-link-search=native={}", dir.display());
  println!("cargo:rustc-link-lib=static={lib}");
}

fn build_libvpx(target_os: &str) {
  let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
  let src = manifest_dir.join("vendor").join("libvpx");
  // A submodule bump re-runs this script; make then rebuilds what changed.
  println!("cargo:rerun-if-changed={}", src.display());
  if !src.join("configure").exists() {
    panic!(
      "forge/vendor/libvpx is empty; fetch the libvpx submodule first:\n  git submodule update --init forge/vendor/libvpx"
    );
  }

  let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
  let target = libvpx_target(&target_arch, target_os);
  if matches!(target_arch.as_str(), "x86" | "x86_64") && find_on_path(&["nasm", "yasm"]).is_none() {
    panic!("libvpx needs an assembler for its x86 SIMD code and neither nasm nor yasm is on PATH; install nasm");
  }

  let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR not set"));
  let build_dir = out_dir.join("libvpx");
  std::fs::create_dir_all(&build_dir).expect("create the libvpx build directory");

  if !build_dir.join("config.mk").exists() {
    let compiler = cc::Build::new().get_compiler();
    let mut configure = Command::new("sh");
    configure
      .arg(src.join("configure"))
      .arg(format!("--target={target}"))
      .args([
        "--enable-vp9",
        "--disable-vp8",
        "--enable-static",
        "--disable-shared",
        "--enable-pic",
        "--disable-examples",
        "--disable-tools",
        "--disable-docs",
        "--disable-unit-tests",
        "--disable-install-bins",
        "--disable-install-docs",
        "--disable-postproc",
        "--disable-vp9-postproc",
        "--disable-webm-io",
        "--disable-libyuv",
        "--disable-vp9-encoder",
      ])
      .env("CC", compiler.path())
      .current_dir(&build_dir);
    run("configure libvpx", &mut configure);
  }

  let jobs = std::env::var("NUM_JOBS").unwrap_or_else(|_| "1".to_string());
  run("make libvpx", Command::new("make").arg(format!("-j{jobs}")).current_dir(&build_dir));

  println!("cargo:rustc-link-search=native={}", build_dir.display());
  println!("cargo:rustc-link-lib=static=vpx");
}

/// The libvpx configure target for the Rust target. The darwin suffix is the
/// Darwin kernel major (20 = macOS 11, the oldest macOS Rust itself
/// supports); libvpx only uses it for the deployment-target flag.
fn libvpx_target(arch: &str, os: &str) -> String {
  let target = match (arch, os) {
    ("x86_64", "linux") => "x86_64-linux-gcc",
    ("x86", "linux") => "x86-linux-gcc",
    ("aarch64", "linux") => "arm64-linux-gcc",
    ("arm", "linux") => "armv7-linux-gcc",
    ("x86_64", "macos") => "x86_64-darwin20-gcc",
    ("aarch64", "macos") => "arm64-darwin20-gcc",
    (_, "windows") => panic!(
      "the libvpx build is not wired for Windows yet (configure --target=x86_64-win64-vs17 + msbuild); build without the video feature"
    ),
    _ => "generic-gnu",
  };
  target.to_string()
}

fn find_on_path(names: &[&str]) -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  std::env::split_paths(&path).find_map(|dir| names.iter().map(|n| dir.join(n)).find(|p| p.is_file()))
}

fn run(what: &str, command: &mut Command) {
  let output = command.output().unwrap_or_else(|e| panic!("{what}: failed to start: {e}"));
  if !output.status.success() {
    panic!(
      "{what} failed ({}):\n{}\n{}",
      output.status,
      String::from_utf8_lossy(&output.stdout),
      String::from_utf8_lossy(&output.stderr)
    );
  }
}
