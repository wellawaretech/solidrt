//! The stock SolidRT program: lattice's `Program` with no modules of its
//! own, behind the two binaries (`solidrt`, the production runtime, and
//! `solidrt-go`, the dev client) and the Android cdylib of each. A custom
//! runtime (docs/runtime, "A custom runtime") is this crate with its
//! modules in `modules()`, and its screens taken from the `@solidrt/cli`
//! package instead of `resources/`.

/// The modules beside lattice's built-ins: none in the stock program.
pub fn modules() -> lattice::Modules {
  lattice::Modules::new()
}

/// The program: the modules and the screens lattice embeds, bundled from
/// apps/player into `resources/` by `make -C lattice player-bundle`.
pub fn program() -> lattice::Program {
  lattice::Program {
    modules: modules(),
    bsod: include_str!("../resources/bsod/bsod.sol.js"),
    #[cfg(feature = "go")]
    player: include_str!("../resources/player/index.sol.js"),
  }
}

// The Android .so's entry; nothing on other targets.
lattice::android_entry!(crate::program());
