// The player and BSOD bundles lib.rs embeds (`include_str!`) are generated
// from apps/player by `make -C lattice player-bundle`, which `make client`
// runs; they are not tracked. A bare cargo build in a fresh checkout has
// none, and include_str!'s missing-file error would not say how to make
// them, so say it here. Cargo reruns this script while a path is missing
// and once it appears.
fn main() {
  for path in ["resources/player/index.sol.js", "resources/bsod/bsod.sol.js"] {
    println!("cargo:rerun-if-changed={path}");
    if !std::path::Path::new(path).exists() {
      panic!("{path} is missing: it is generated, run `make -C lattice player-bundle` (make client runs it)");
    }
  }
}
