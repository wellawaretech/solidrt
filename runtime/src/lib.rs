//! The stock SolidRT program: lattice's `Program` with no modules of its
//! own, behind the two binaries (`solidrt`, the production runtime, and
//! `solidrt-go`, the dev client) and the Android cdylib of each. A custom
//! runtime (docs/runtime, "A custom runtime") is this crate with its
//! modules in `modules()`.

/// The modules beside lattice's built-ins: none in the stock program.
pub fn modules() -> lattice::Modules {
  lattice::Modules::new()
}

/// The fonts every app on the stock runtime starts from: Noto Sans, Noto
/// Serif and Noto Sans Mono under the role aliases `sans`, `serif` and
/// `mono`. A custom runtime lists its own set (docs/runtime, "The fonts").
pub fn fonts() -> lattice::Fonts {
  lattice::Fonts::none()
    .with("sans", lattice::fonts::NOTO_SANS)
    .with("serif", lattice::fonts::NOTO_SERIF)
    .with("mono", lattice::fonts::NOTO_SANS_MONO)
}

/// The program: the modules, the fonts, and the two screens the runtime
/// embeds (the failure screen in every build, the player in the dev
/// client), from the @solidrt/cli package: bundled there from apps/player
/// by `make player-bundle` in the checkout, shipped in the package for an
/// app.
pub fn program() -> lattice::Program {
  lattice::Program {
    modules: modules(),
    fonts: fonts(),
    bsod: include_str!("../../node_modules/@solidrt/cli/player/bsod.sol.js"),
    #[cfg(feature = "go")]
    player: include_str!("../../node_modules/@solidrt/cli/player/index.sol.js"),
  }
}

// The Android .so's entry; nothing on other targets.
lattice::android_entry!(crate::program());
