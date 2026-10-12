//! The font set a program carries, and how an app's own fonts layer over
//! it. A runtime's fonts are the ones every app on it starts from: the stock
//! program carries the three Noto role defaults, a custom runtime edits that
//! set in its `lib.rs` (drop a role, replace one with its own file, add
//! faces), and a pack ships only the files its app binds on top. The dev
//! client and the production runtime register the same set, so an app
//! renders in the player as its pack does.

use crate::manifest::AppFonts;
use alloy::rendertree::FontPayload;
use std::borrow::Cow;

/// The fonts a runtime brings, each under its alias. The three role aliases
/// are `sans` (text's default family, and what an unknown family resolves
/// to), `serif` and `mono`; any other alias is a face an app selects by
/// name. One face per alias: `with` on an alias already in the set replaces
/// it, since two fonts under one alias would form one family in which the
/// first registered wins.
///
/// ```ignore
/// lattice::Fonts::stock().without("mono").with("sans", include_bytes!("../fonts/Brand.ttf"))
/// ```
#[derive(Clone, Default)]
pub struct Fonts(Vec<FontPayload>);

impl Fonts {
  /// The stock set: Noto Sans, Noto Serif and Noto Sans Mono under the
  /// three role aliases, so a default app renders identically on every
  /// platform. Compiled in from `alloy/assets/fonts/`.
  pub fn stock() -> Fonts {
    Fonts::none()
      .with("sans", &include_bytes!("../../alloy/assets/fonts/NotoSans.ttf")[..])
      .with("serif", &include_bytes!("../../alloy/assets/fonts/NotoSerif.ttf")[..])
      .with("mono", &include_bytes!("../../alloy/assets/fonts/NotoSansMono.ttf")[..])
  }

  /// No fonts: a runtime whose apps bring every face they draw.
  pub fn none() -> Fonts {
    Fonts(Vec::new())
  }

  /// The set with `bytes` under `alias`: in place of the font already under
  /// that alias, else appended. `bytes` is a font file (TrueType, OpenType),
  /// typically an `include_bytes!`.
  pub fn with(mut self, alias: &str, bytes: impl Into<Cow<'static, [u8]>>) -> Fonts {
    let font = FontPayload { alias: Some(alias.to_string()), bytes: bytes.into() };
    match self.0.iter_mut().find(|font| font.alias.as_deref() == Some(alias)) {
      Some(slot) => *slot = font,
      None => self.0.push(font),
    }
    self
  }

  /// The set without the font under `alias`. A set that has none is a
  /// programming error in the program's `lib.rs` and panics here.
  pub fn without(mut self, alias: &str) -> Fonts {
    let Some(at) = self.0.iter().position(|font| font.alias.as_deref() == Some(alias)) else {
      panic!("The program's fonts bind no '{alias}' to drop; they bind {}", self.aliases().join(", "));
    };
    self.0.remove(at);
    self
  }

  /// The aliases bound, in registration order.
  pub fn aliases(&self) -> Vec<&str> {
    self.0.iter().filter_map(|font| font.alias.as_deref()).collect()
  }

  /// The set as the platform registers it.
  pub(crate) fn into_payloads(self) -> Vec<FontPayload> {
    self.0
  }
}

/// The font set an app runs on: the program's fonts plus the app's own,
/// where every alias the app's manifest binds is the app's alone: a base
/// entry under it is dropped, whether the app binds a file (its override) or
/// nothing (a dropped default: the role then resolves like an unknown
/// family, to the sans role, or to the first registered face when sans
/// itself is gone; the glyph engine draws registered faces only, nothing
/// comes from the system). An alias dropped with nothing under it is
/// warned about: the app meant to drop a font the runtime does not have.
/// Two fonts under one alias would form one family style set in which style
/// matching keeps the first registered on a tie, so a project's "sans"
/// override would lose to the base Noto Sans (the glyph engine's font set
/// keeps the first registration per name).
pub(crate) fn merge_fonts(base: &[FontPayload], app: AppFonts) -> Vec<FontPayload> {
  let claimed = |font: &FontPayload| font.alias.as_ref().is_some_and(|alias| app.aliases.contains(alias));
  for alias in &app.aliases {
    let under = |font: &FontPayload| font.alias.as_deref() == Some(alias.as_str());
    if !base.iter().any(under) && !app.fonts.iter().any(under) {
      log::warn!("[sol] The app drops the font '{alias}', which the runtime does not bind");
    }
  }
  let mut fonts: Vec<FontPayload> = base.iter().filter(|font| !claimed(font)).cloned().collect();
  fonts.extend(app.fonts);
  fonts
}
