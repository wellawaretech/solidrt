fn main() {
  let version = std::process::Command::new("git")
    .args(["describe", "--tags", "--match", "v*", "--always"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().trim_start_matches('v').to_string())
    .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

  println!("cargo:rustc-env=FLUX_VERSION={version}");
  println!("cargo:rerun-if-changed=.git/HEAD");
  println!("cargo:rerun-if-changed=.git/refs/tags");
}
