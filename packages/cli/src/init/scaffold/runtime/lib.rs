//! The app's runtime: lattice's program with this project's modules and
//! fonts (docs/runtime, "A custom runtime"). A module is a `mod` here, a
//! line in `modules()`, and its types in types/.

mod hello;

/// The modules beside lattice's built-ins, each under its `runtime:*` name,
/// the name the app imports and finds in `Flux.capabilities`.
pub fn modules() -> lattice::Modules {
  lattice::Modules::new().add("runtime:hello", hello::HelloModule)
}

/// The fonts every app on this runtime starts from: the stock set is Noto
/// Sans, Noto Serif and Noto Sans Mono under the role aliases `sans`,
/// `serif` and `mono`. Edit it here and every app on the runtime follows,
/// in the player and packed alike: `.without("mono")` drops a role,
/// `.with("sans", include_bytes!("../fonts/Brand.ttf"))` replaces one or
/// adds a face under a new alias, `lattice::Fonts::none()` starts empty.
pub fn fonts() -> lattice::Fonts {
  lattice::Fonts::stock()
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
