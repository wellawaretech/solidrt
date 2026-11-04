use crate::locale::{preferred_locales, tag, tags};

#[test]
fn a_language_and_country_make_a_bcp47_tag() {
  assert_eq!(tag(Some("en"), Some("US")).as_deref(), Some("en-US"));
  assert_eq!(tag(Some("de"), None).as_deref(), Some("de"));
  assert_eq!(tag(Some("fr"), Some("")).as_deref(), Some("fr"));
  assert_eq!(tag(None, Some("US")), None);
  assert_eq!(tag(Some(""), None), None);
}

#[test]
fn a_locale_named_twice_is_listed_once_where_it_first_appears() {
  let entry = |language: &str, country: Option<&str>| (Some(language.to_string()), country.map(str::to_string));
  let entries = [entry("de", Some("DE")), entry("fr", None), entry("de", Some("DE")), (None, None)];
  assert_eq!(tags(&entries), vec!["de-DE".to_string(), "fr".to_string()]);
}

#[test]
fn the_platform_locales_read_without_sdl_init() {
  for tag in preferred_locales() {
    assert!(!tag.is_empty() && !tag.contains('_'), "not a BCP 47 tag: {tag}");
  }
}
