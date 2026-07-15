// Android: link clang's compiler-rt builtins archive into the final artifact.
//
// The bundled libffi calls __clear_cache (the aarch64/arm instruction-cache
// flush), which lives in libclang_rt.builtins-<arch>-android.a. The NDK clang
// driver links that archive implicitly for C code, but rustc links with
// -nodefaultlibs and Rust's compiler-builtins does not provide the symbol, so
// without this the cdylib carries an undefined __clear_cache that only
// surfaces as a dlopen failure on device. Ask the target C compiler (the NDK
// clang wrapper cargo-ndk points CC_<target> at) where its builtins archive
// is and link it explicitly.

fn main() {
  let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set");
  if target_os != "android" {
    return;
  }

  let compiler = cc::Build::new().get_compiler();
  let output = compiler
    .to_command()
    .arg("--print-libgcc-file-name")
    .output()
    .expect("failed to run the target C compiler to locate clang_rt.builtins");
  if !output.status.success() {
    panic!(
      "--print-libgcc-file-name failed: {}",
      String::from_utf8_lossy(&output.stderr)
    );
  }

  let path_str = String::from_utf8(output.stdout).expect("non-UTF-8 compiler output");
  let path = std::path::Path::new(path_str.trim());
  let dir = path
    .parent()
    .expect("builtins archive path has no parent directory");
  let file = path
    .file_name()
    .and_then(|name| name.to_str())
    .expect("builtins archive path has no file name");
  let lib = file
    .strip_prefix("lib")
    .and_then(|name| name.strip_suffix(".a"))
    .unwrap_or_else(|| panic!("unexpected builtins archive name: {file}"));

  println!("cargo:rustc-link-search=native={}", dir.display());
  println!("cargo:rustc-link-lib=static={lib}");
}
