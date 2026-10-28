//! The app's runtime: lattice's program with this project's modules and
//! fonts (docs/runtime, "A custom runtime"). A module is a `mod` here, a
//! line in `modules()`, and its types in types/.

mod hello;

/// The modules beside lattice's built-ins, each under its `runtime:*` name,
/// the name the app imports and finds in `Flux.capabilities`.
pub fn modules() -> lattice::Modules {
  lattice::Modules::new().add("runtime:hello", hello::HelloModule)
}

/// The fonts every app on this runtime starts from: the stock Noto Sans,
/// Noto Serif and Noto Sans Mono under the role aliases `sans`, `serif` and
/// `mono`. Edit it here and every app on the runtime follows, in the player
/// and packed alike. The binary carries exactly the faces listed: delete a
/// line to drop a role and its bytes (`false` in package.json drops a role
/// but leaves its bytes in), point one at
/// `include_bytes!("../fonts/Brand.ttf")` to replace it, or add a line under
/// a new alias for a face an app selects by name.
pub fn fonts() -> lattice::Fonts {
  lattice::Fonts::none()
    .with("sans", lattice::fonts::NOTO_SANS)
    .with("serif", lattice::fonts::NOTO_SERIF)
    .with("mono", lattice::fonts::NOTO_SANS_MONO)
}

/// The program: the modules, the fonts, and the two screens the runtime
/// embeds (the failure screen in every build, the player in the dev
/// client), from the @solidrt/cli package beside the app.
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
