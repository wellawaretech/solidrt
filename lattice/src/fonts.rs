//! The font set a program carries, and how an app's own fonts layer over
//! it. A runtime's fonts are the ones every app on it starts from: the stock
//! program carries the three Noto role defaults, a custom runtime lists its
//! own set in its `lib.rs` (the Notos it keeps, its own files, added faces),
//! and a pack ships only the files its app binds on top. The dev client and
//! the production runtime register the same set, so an app renders in the
//! player as its pack does.

use crate::manifest::AppFonts;
use alloy::rendertree::FontPayload;
use std::borrow::Cow;

/// Noto Sans, the stock `sans` role. Each stock face is its own static, so
/// a program links only the faces it names: one it leaves out costs no
/// bytes in the binary.
pub static NOTO_SANS: &[u8] = include_bytes!("../../alloy/assets/fonts/NotoSans.ttf");
/// Noto Serif, the stock `serif` role.
pub static NOTO_SERIF: &[u8] = include_bytes!("../../alloy/assets/fonts/NotoSerif.ttf");
/// Noto Sans Mono, the stock `mono` role.
pub static NOTO_SANS_MONO: &[u8] = include_bytes!("../../alloy/assets/fonts/NotoSansMono.ttf");

/// The fonts a runtime brings, each under its alias. The three role aliases
/// are `sans` (text's default family, and what an unknown family resolves
/// to), `serif` and `mono`; any other alias is a face an app selects by
/// name. One face per alias: `with` on an alias already in the set replaces
/// it, since two fonts under one alias would form one family in which the
/// first registered wins.
///
/// The set is written out face by face, and what it names is what the
/// binary carries. A role left out resolves like an unknown family:
///
/// ```ignore
/// lattice::Fonts::none()
///   .with("sans", include_bytes!("../fonts/Brand.ttf"))
///   .with("serif", lattice::fonts::NOTO_SERIF)
/// ```
#[derive(Clone, Default)]
pub struct Fonts(Vec<FontPayload>);

impl Fonts {
  /// No fonts; `with` adds each face the runtime carries.
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
/// The set registered is logged, by where each font came from, so a wrong
/// face is a log read.
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
  let names = |fonts: &[FontPayload]| fonts.iter().filter_map(|font| font.alias.clone()).collect::<Vec<_>>().join(", ");
  let dropped: Vec<&str> = app
    .aliases
    .iter()
    .filter(|alias| !app.fonts.iter().any(|font| font.alias.as_ref() == Some(alias)))
    .map(String::as_str)
    .collect();
  let mut parts = Vec::new();
  if !fonts.is_empty() {
    parts.push(format!("{} from the runtime", names(&fonts)));
  }
  if !app.fonts.is_empty() {
    parts.push(format!("{} from the app", names(&app.fonts)));
  }
  if !dropped.is_empty() {
    parts.push(format!("{} dropped", dropped.join(", ")));
  }
  if parts.is_empty() {
    parts.push("none registered".to_string());
  }
  log::info!("[sol] Fonts: {}", parts.join("; "));
  fonts.extend(app.fonts);
  fonts
}
