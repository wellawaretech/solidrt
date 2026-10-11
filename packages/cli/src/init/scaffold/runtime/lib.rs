//! The app's runtime: lattice's program with this project's modules
//! (docs/runtime, "A custom runtime"). A module is a `mod` here, a line in
//! `modules()`, and its types in types/.

mod hello;

/// The modules beside lattice's built-ins, each under its `runtime:*` name,
/// the name the app imports and finds in `Flux.capabilities`.
pub fn modules() -> lattice::Modules {
  lattice::Modules::new().add("runtime:hello", hello::HelloModule)
}

/// The program: the modules, and the two screens the runtime embeds (the
/// failure screen in every build, the player in the dev client), from the
/// @solidrt/cli package beside the app.
pub fn program() -> lattice::Program {
  lattice::Program {
    modules: modules(),
    bsod: include_str!("../../node_modules/@solidrt/cli/player/bsod.sol.js"),
    #[cfg(feature = "go")]
    player: include_str!("../../node_modules/@solidrt/cli/player/index.sol.js"),
  }
}

// The Android .so's entry; nothing on other targets.
lattice::android_entry!(crate::program());
