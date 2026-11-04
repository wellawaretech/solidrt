//! The user's preferred locales, most preferred first, as BCP 47 tags
//! (`en-US`, `de`). SDL reads them from the platform: the `LANGUAGE` and
//! `LANG` environment on Linux, the system preferences on macOS and
//! Windows, the configuration on Android. A locale the platform names
//! twice (`LANGUAGE` and `LANG` both) is listed once, where it first
//! appears. `SDL_GetPreferredLocales` is not thread safe, so one caller at
//! a time holds the lock. Each call reads the platform again; a change
//! event comes with the event pump.

use std::ffi::{c_char, CStr};
use std::sync::{Mutex, PoisonError};

use sdl3::sys::locale::SDL_GetPreferredLocales;
use sdl3::sys::stdinc::SDL_free;

static LOCK: Mutex<()> = Mutex::new(());

/// The preferred locales; empty when the platform reports none.
pub fn preferred_locales() -> Vec<String> {
  let _one_caller = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
  let mut count = 0;
  // SAFETY: SDL writes the entry count and returns one allocation holding
  // the null-terminated pointer array and the strings it points to, or
  // null; it is freed once below, after the strings are copied out.
  let list = unsafe { SDL_GetPreferredLocales(&mut count) };
  if list.is_null() {
    return Vec::new();
  }
  let mut entries = Vec::new();
  for i in 0..usize::try_from(count).unwrap_or(0) {
    // SAFETY: `i` is below the count SDL reported for `list`; each entry
    // is a valid SDL_Locale while the allocation lives.
    let locale = unsafe { *list.add(i) };
    if locale.is_null() {
      continue;
    }
    // SAFETY: see above; `language` is a C string, `country` one or null.
    entries.push(unsafe { (text((*locale).language), text((*locale).country)) });
  }
  // SAFETY: `list` is SDL's single allocation, not used after this.
  unsafe { SDL_free(list.cast()) };
  tags(&entries)
}

/// The tags of SDL's (language, country) entries, in order, each once.
pub(crate) fn tags(entries: &[(Option<String>, Option<String>)]) -> Vec<String> {
  let mut tags = Vec::new();
  for (language, country) in entries {
    if let Some(tag) = tag(language.as_deref(), country.as_deref()) {
      if !tags.contains(&tag) {
        tags.push(tag);
      }
    }
  }
  tags
}

/// # Safety
/// `ptr` is null or a valid C string.
unsafe fn text(ptr: *const c_char) -> Option<String> {
  (!ptr.is_null()).then(|| unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}

/// The BCP 47 tag of SDL's language and country (`en` and `US` make
/// `en-US`); none without a language.
pub(crate) fn tag(language: Option<&str>, country: Option<&str>) -> Option<String> {
  let language = language.filter(|l| !l.is_empty())?;
  Some(match country.filter(|c| !c.is_empty()) {
    Some(country) => format!("{language}-{country}"),
    None => language.to_string(),
  })
}
