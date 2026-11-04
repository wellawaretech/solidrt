//! `navigator`: the global the web's navigator facilities hang on, in
//! every engine. Here, `languages` and `language`: the user's preferred
//! locales (`ore::locale`), most preferred first, as BCP 47 tags. They
//! are read on every access, `languages` as a frozen array, as on the web;
//! `languagechange` comes once the OS layer owns the event pump. With no locale reported, the list is the fallback
//! below, so neither is ever empty, as on the web. The gui seam adds
//! `navigator.clipboard` (clipboard.rs).

use rquickjs::object::Accessor;
use rquickjs::{Array, Ctx, Function, Object};

/// The locale reported when the platform reports none (a `C` locale, an
/// unset `LANG`): what browsers default to.
const FALLBACK_LANGUAGE: &str = "en-US";

/// Install the `navigator` global with `languages` and `language`.
pub fn init_navigator(ctx: &Ctx<'_>) {
  let navigator = Object::new(ctx.clone()).expect("failed to create navigator");
  navigator.prop("languages", Accessor::new_get(languages).enumerable()).expect("failed to set navigator.languages");
  navigator.prop("language", Accessor::new_get(language).enumerable()).expect("failed to set navigator.language");
  ctx.globals().set("navigator", navigator).expect("failed to set navigator global");
}

fn languages<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Array<'js>> {
  let languages = Array::new(ctx.clone())?;
  for (i, tag) in tags().into_iter().enumerate() {
    languages.set(i, tag)?;
  }
  let object: Object = ctx.globals().get("Object")?;
  let freeze: Function = object.get("freeze")?;
  freeze.call::<_, ()>((languages.clone(),))?;
  Ok(languages)
}

fn language() -> String {
  tags().swap_remove(0)
}

/// The preferred locales, never empty.
fn tags() -> Vec<String> {
  let tags = ore::locale::preferred_locales();
  if tags.is_empty() {
    vec![FALLBACK_LANGUAGE.to_string()]
  } else {
    tags
  }
}
