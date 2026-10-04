// Two target-dependent link jobs.
//
// Android: link clang's compiler-rt builtins archive into the final artifact.
// The NDK clang driver links libclang_rt.builtins-<arch>-android.a implicitly
// for C code, but rustc links with -nodefaultlibs, so a builtin the vendored
// C and C++ below call and Rust's compiler-builtins does not provide comes
// out as an undefined symbol that only surfaces as a dlopen failure on
// device (the bundled libffi's __clear_cache was the first case; libffi is
// gone, and whether opus or Basis need a builtin is unverified, so the link
// stays until an Android build without it proves otherwise: okf/tiny.md).
// Ask the target C compiler (the NDK clang wrapper cargo-ndk points
// CC_<target> at) where its builtins archive is and link it explicitly.
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
//
// On MSVC, libvpx's make only generates a Visual Studio solution (configure
// --target=x86_64-win64-vs<N>, N matching the installed Visual Studio) and
// msbuild compiles it. configure and make still need a POSIX environment
// (MSYS2: sh, make, diffutils) and nasm on PATH, which the vcxproj's asm
// step calls too. The make is taken from beside the sh, not from PATH: the
// makefiles run #!/bin/bash generator scripts, and a native Windows make
// launches such a script's interpreter by bare name through CreateProcess,
// which searches System32 before PATH and finds the WSL launcher there
// under that name (every GitHub Windows image has it). The
// CRT follows the Rust target: crt-static builds libvpx /MT (vpxmt.lib),
// else /MD (vpxmd.lib). A mix is not a link error: the UCRT import libs
// let /MD objects into a /MT binary with a warning cargo does not show, so
// check an archive with `dumpbin -directives` (LIBCMT only) rather than by
// linking. Both libraries build Release whatever the cargo profile: Rust
// links the release CRT only, and the Debug configs would pick the debug
// CRT.

//
// Everywhere, with the `ktx2` feature: compile the vendored Basis Universal
// (forge/vendor/basis_universal, a submodule pinned to a release tag)
// straight through cc - its transcoder, its encoder and the C API over
// both (forge/src/ktx2/ffi.rs binds that API). Upstream's CMake project
// builds a command line tool and a C++ library; the C API sources are only
// in its wasm and python targets, so the file list here is upstream's
// ENCODER_LIB_SRC_LIST plus the two API files. The switches are upstream's
// defaults for a portable build (no SSE kernels, no OpenCL, no astcenc)
// plus the transcoder's legacy output formats turned off; the modern ones
// (HDR, XUASTC, ASTC) cannot be turned off in this release (their switches
// no longer compile), which is most of what the transcoder weighs:
// okf/backlog/runtime-optimization.md. Zstd supercompression is on: UASTC
// files ship compressed, and the KTX2 files other tools write are commonly
// UASTC with zstd. The zstd is the single-file library upstream vendors in
// the same submodule (zstd/zstd.c), compiled as C in a build of its own
// because the Basis build is C++; the whole library, not zstddeclib.c,
// because the encoder compresses.

use std::path::PathBuf;
use std::process::Command;

/// The Basis Universal sources compiled into forge, relative to the
/// submodule root: upstream's encoder library list (CMakeLists.txt,
/// ENCODER_LIB_SRC_LIST) and the C API over encoder and transcoder.
const BASIS_SOURCES: &[&str] = &[
  "transcoder/basisu_transcoder.cpp",
  "encoder/basisu_wasm_transcoder_api.cpp",
  "encoder/basisu_wasm_api.cpp",
  "encoder/basisu_backend.cpp",
  "encoder/basisu_basis_file.cpp",
  "encoder/basisu_comp.cpp",
  "encoder/basisu_enc.cpp",
  "encoder/basisu_etc.cpp",
  "encoder/basisu_frontend.cpp",
  "encoder/basisu_gpu_texture.cpp",
  "encoder/basisu_pvrtc1_4.cpp",
  "encoder/basisu_resampler.cpp",
  "encoder/basisu_resample_filters.cpp",
  "encoder/basisu_ssim.cpp",
  "encoder/basisu_uastc_enc.cpp",
  "encoder/basisu_bc7e_scalar.cpp",
  "encoder/basisu_dds_export.cpp",
  "encoder/basisu_bc7enc.cpp",
  "encoder/jpgd.cpp",
  "encoder/basisu_kernels_sse.cpp",
  "encoder/basisu_bc15_spmd.cpp",
  "encoder/basisu_bc15_spmd_sse.cpp",
  "encoder/basisu_opencl.cpp",
  "encoder/pvpngreader.cpp",
  "encoder/basisu_uastc_hdr_4x4_enc.cpp",
  "encoder/basisu_astc_hdr_6x6_enc.cpp",
  "encoder/basisu_astc_hdr_common.cpp",
  "encoder/basisu_astc_ldr_common.cpp",
  "encoder/basisu_astc_ldr_encode.cpp",
  "encoder/basisu_astc_ldr_fencode.cpp",
  "encoder/basisu_xbc7_encode.cpp",
  "encoder/basisu_tinyexr.cpp",
  "encoder/3rdparty/android_astc_decomp.cpp",
];

/// The zstd Basis Universal supercompresses with: upstream's single-file
/// copy of the library, relative to the submodule root.
const BASIS_ZSTD_SOURCE: &str = "zstd/zstd.c";

/// Compile switches for the Basis Universal build (name, value): the
/// portable configuration, and the transcoder output formats no target of
/// ours samples (ETC2 EAC A8, BC7 and UASTC stay on).
const BASIS_DEFINES: &[(&str, &str)] = &[
  ("BASISU_SUPPORT_SSE", "0"),
  ("BASISU_SUPPORT_OPENCL", "0"),
  ("BASISU_SUPPORT_ASTCENC", "0"),
  ("BASISD_SUPPORT_KTX2_ZSTD", "1"),
  ("BASISD_SUPPORT_PVRTC1", "0"),
  ("BASISD_SUPPORT_PVRTC2", "0"),
  ("BASISD_SUPPORT_ATC", "0"),
  ("BASISD_SUPPORT_FXT1", "0"),
  ("BASISD_SUPPORT_DXT1", "0"),
  ("BASISD_SUPPORT_DXT5A", "0"),
  ("BASISD_SUPPORT_ETC2_EAC_RG11", "0"),
];

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
  if std::env::var_os("CARGO_FEATURE_KTX2").is_some() {
    build_basis();
  }
}

fn build_basis() {
  let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
  let src = manifest_dir.join("vendor").join("basis_universal");
  // The two directories compiled from, not the whole checkout (its demos
  // and test files are most of it and none of our business).
  println!("cargo:rerun-if-changed={}", src.join("encoder").display());
  println!("cargo:rerun-if-changed={}", src.join("transcoder").display());
  println!("cargo:rerun-if-changed={}", src.join("zstd").display());
  if !src.join("transcoder").join("basisu_transcoder.cpp").exists() {
    panic!(
      "forge/vendor/basis_universal is empty; fetch the submodule first:\n  git submodule update --init forge/vendor/basis_universal"
    );
  }
  let mut build = cc::Build::new();
  build
    .cpp(true)
    .std("c++17")
    .warnings(false)
    // Upstream requires it (its CMakeLists.txt says so on line one); MSVC
    // never assumes strict aliasing.
    .flag_if_supported("-fno-strict-aliasing")
    .flag_if_supported("-fvisibility=hidden")
    // The C API asserts on a bad handle or file before returning false; a
    // malformed file must fail the call, not abort the runtime.
    .define("NDEBUG", None);
  for (name, value) in BASIS_DEFINES {
    build.define(name, *value);
  }
  // Android links the NDK's shared C++ runtime (cc's default there), the one
  // runtime every C++ library in the process shares: whisper links it too,
  // and lattice/Makefile.android stages libc++_shared.so into every APK.
  for file in BASIS_SOURCES {
    build.file(src.join(file));
  }
  build.compile("basisu");
  // After basisu on the link line: it is what refers to the ZSTD_* symbols.
  cc::Build::new()
    .warnings(false)
    .flag_if_supported("-fvisibility=hidden")
    .define("NDEBUG", None)
    .file(src.join(BASIS_ZSTD_SOURCE))
    .compile("basisu_zstd");
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
  let mut config = cmake::Config::new(&src);
  config
    .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
    .define("OPUS_BUILD_PROGRAMS", "OFF")
    .define("OPUS_BUILD_TESTING", "OFF")
    .define("OPUS_INSTALL_PKG_CONFIG_MODULE", "OFF")
    .define("OPUS_INSTALL_CMAKE_CONFIG_MODULE", "OFF")
    .define("CMAKE_INSTALL_LIBDIR", "lib");
  // opus sets CMAKE_MSVC_RUNTIME_LIBRARY itself (overriding the toolchain
  // file): the DLL CRT unless OPUS_STATIC_RUNTIME, and the debug CRT in the
  // Debug config. Rust links the release CRT only, so on MSVC follow
  // crt-static and always build Release.
  if msvc() {
    config.define("OPUS_STATIC_RUNTIME", if crt_static() { "ON" } else { "OFF" }).profile("Release");
  }
  let dst = config.build();
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
  let msvc = msvc();
  let crt_static = crt_static();
  let target = if msvc { libvpx_msvc_target(&target_arch) } else { libvpx_target(&target_arch, target_os) };
  if matches!(target_arch.as_str(), "x86" | "x86_64") && find_on_path(&["nasm", "yasm"]).is_none() {
    panic!("libvpx needs an assembler for its x86 SIMD code and neither nasm nor yasm is on PATH; install nasm");
  }

  let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR not set"));
  let build_dir = out_dir.join("libvpx");
  std::fs::create_dir_all(&build_dir).expect("create the libvpx build directory");

  let sh = find_on_path(&["sh"]).unwrap_or_else(|| panic!("libvpx: no sh on PATH; its configure needs a POSIX shell"));
  if !build_dir.join("config.mk").exists() {
    // configure finds its source dir by cutting $0 at the last '/', which a
    // Windows path spelled with backslashes does not have.
    let script = src.join("configure").to_string_lossy().replace('\\', "/");
    let mut configure = Command::new(&sh);
    configure
      .arg(script)
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
      .current_dir(&build_dir);
    if msvc {
      // configure bypasses the compiler for vs targets; msbuild finds cl.
      if crt_static {
        configure.arg("--enable-static-msvcrt");
      }
    } else {
      configure.env("CC", cc::Build::new().get_compiler().path());
    }
    run("configure libvpx", &mut configure);
  }

  let jobs = std::env::var("NUM_JOBS").unwrap_or_else(|_| "1".to_string());
  if msvc {
    build_libvpx_msbuild(&build_dir, &sh, &target_arch, crt_static, &jobs);
    return;
  }
  run("make libvpx", Command::new("make").arg(format!("-j{jobs}")).current_dir(&build_dir));

  println!("cargo:rustc-link-search=native={}", build_dir.display());
  println!("cargo:rustc-link-lib=static=vpx");
}

/// The MSVC half of the libvpx build: make generates the solution and the
/// rtcd/config headers, msbuild compiles the `vpx` project (not the rate
/// control library next to it) in its Release config (see the header on
/// the CRT) into <platform>/Release/vpx{mt,md}.lib.
fn build_libvpx_msbuild(
  build_dir: &std::path::Path,
  sh: &std::path::Path,
  target_arch: &str,
  crt_static: bool,
  jobs: &str,
) {
  // The POSIX make next to the sh (see the header on why not PATH's make).
  let make = sh.with_file_name(format!("make{}", std::env::consts::EXE_SUFFIX));
  if !make.is_file() {
    panic!(
      "libvpx: no make next to {}; install it into that environment (MSYS2: pacman -S make diffutils)",
      sh.display()
    );
  }
  run(
    "generate the libvpx solution",
    Command::new(&make).arg("NO_LAUNCH_DEVENV=1").arg(format!("-j{jobs}")).current_dir(build_dir),
  );

  let target = std::env::var("TARGET").expect("TARGET not set");
  let mut msbuild = cc::windows_registry::find(&target, "msbuild.exe")
    .unwrap_or_else(|| panic!("libvpx: msbuild.exe not found; install Visual Studio Build Tools"));
  let platform = match target_arch {
    "x86_64" => "x64",
    "aarch64" => "ARM64",
    _ => panic!("libvpx: no msbuild platform for {target_arch}"),
  };
  // The Release projects compile /GL, whose objects are LTCG IL with no COFF
  // symbols: rustc bundles the archive into the rlib and every vpx_* symbol
  // comes out unresolved. A global property overrides the project's.
  msbuild
    .args(["vpx.sln", "-t:vpx", "-nologo", "-v:minimal", "-p:Configuration=Release"])
    .arg("-p:WholeProgramOptimization=false")
    .arg(format!("-p:Platform={platform}"))
    .arg(format!("-m:{jobs}"))
    .current_dir(build_dir);
  run("msbuild libvpx", &mut msbuild);

  let lib = if crt_static { "vpxmt" } else { "vpxmd" };
  println!("cargo:rustc-link-search=native={}", build_dir.join(platform).join("Release").display());
  println!("cargo:rustc-link-lib=static={lib}");
}

/// The libvpx configure target for an MSVC Rust target: the vs<N> variant
/// of the installed Visual Studio, whose toolset the generated projects
/// name. libvpx v1.17 has vs14 through vs18.
fn libvpx_msvc_target(arch: &str) -> String {
  use cc::windows_registry::VsVers;
  let vs = match cc::windows_registry::find_vs_version() {
    Ok(VsVers::Vs14) => 14,
    Ok(VsVers::Vs15) => 15,
    Ok(VsVers::Vs16) => 16,
    Ok(VsVers::Vs17) => 17,
    Ok(VsVers::Vs18) => 18,
    Ok(other) => panic!("libvpx: Visual Studio {other:?} is newer than the vs targets this script knows; add it"),
    Err(error) => panic!("libvpx: no Visual Studio found: {error}"),
  };
  let arch = match arch {
    "x86_64" => "x86_64-win64",
    "aarch64" => "arm64-win64",
    _ => panic!("libvpx: no Windows target for {arch}"),
  };
  format!("{arch}-vs{vs}")
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
    (_, "windows") => panic!("libvpx: only the MSVC Windows targets are wired; build without the video feature"),
    _ => "generic-gnu",
  };
  target.to_string()
}

/// Whether the Rust target is a Windows MSVC one.
fn msvc() -> bool {
  std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc")
}

/// Whether the Rust target links the C runtime statically (/MT on MSVC).
fn crt_static() -> bool {
  std::env::var("CARGO_CFG_TARGET_FEATURE")
    .map(|features| features.split(',').any(|feature| feature == "crt-static"))
    .unwrap_or(false)
}

fn find_on_path(names: &[&str]) -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  let suffix = std::env::consts::EXE_SUFFIX;
  std::env::split_paths(&path)
    .find_map(|dir| names.iter().map(|n| dir.join(format!("{n}{suffix}"))).find(|p| p.is_file()))
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
