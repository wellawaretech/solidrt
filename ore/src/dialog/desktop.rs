//! The desktop dialogs, over SDL's dialog calls (`sdl3::sys::dialog`): the
//! sdl3 crate's safe wrappers want a `&sdl3::video::Window` for the parent,
//! and the bridge's call has only the raw handle `window` publishes
//! (`window::dialog_parent`). Main thread only, which the session's bridge
//! call guarantees.

use std::ffi::{c_char, c_int, c_void, CStr, CString};

use sdl3::sys::dialog::{
  SDL_DialogFileFilter, SDL_ShowOpenFileDialog, SDL_ShowOpenFolderDialog, SDL_ShowSaveFileDialog,
};

use super::{name_of_path, sdl_pattern, DialogAnswer, DialogError, DialogKind, DialogRequest, DialogResponder, Picked};

// One shown dialog, boxed into SDL's userdata until its callback takes it
// back: the responder, and what the request's C pointers point into. SDL
// reads the filters and the default location until the callback runs, so
// they live here, not on `show`'s stack.
struct Call {
  respond: DialogResponder,
  /// Each filter's name and pattern; `filters` points into them.
  _strings: Vec<(CString, CString)>,
  filters: Vec<SDL_DialogFileFilter>,
  default_location: Option<CString>,
}

/// Show the dialog `request` asks for, with the live window as its parent
/// (none while there is no window). Main thread only. The responder runs
/// once, on whatever thread SDL answers from; a request SDL could not take
/// (a NUL in one of its strings) is refused here, before anything shows.
pub(super) fn show(request: DialogRequest, respond: DialogResponder) {
  let (strings, default_location) = match c_strings(&request) {
    Ok(prepared) => prepared,
    Err(error) => return respond(Err(error)),
  };
  let filters = strings
    .iter()
    .map(|(name, pattern)| SDL_DialogFileFilter { name: name.as_ptr(), pattern: pattern.as_ptr() })
    .collect();
  let call = Box::into_raw(Box::new(Call { respond, _strings: strings, filters, default_location }));
  // SAFETY: `call` was just leaked and nothing else holds it yet; the
  // pointers read here point into its heap buffers, which stay put until
  // the callback drops the box.
  let (filters, nfilters, location) = unsafe {
    let call = &*call;
    let location = call.default_location.as_ref().map_or(std::ptr::null(), |l| l.as_ptr());
    // A count SDL cannot take reads fewer filters, never past the array.
    (call.filters.as_ptr(), c_int::try_from(call.filters.len()).unwrap_or(c_int::MAX), location)
  };
  let parent = crate::window::dialog_parent();
  let userdata = call.cast::<c_void>();
  // SAFETY: `answered` matches SDL_DialogFileCallback and takes back the
  // box `userdata` owns; SDL calls it once, with the result or the error
  // (also when it cannot show the dialog at all). The filters and the
  // location are owned by that box, valid until the callback. `parent` is
  // the live window or null (no parent), read on this, the main thread,
  // where the window is also destroyed.
  unsafe {
    match request.kind {
      DialogKind::OpenFile { multiple } => {
        SDL_ShowOpenFileDialog(Some(answered), userdata, parent, filters, nfilters, location, multiple)
      }
      DialogKind::SaveFile { .. } => {
        SDL_ShowSaveFileDialog(Some(answered), userdata, parent, filters, nfilters, location)
      }
      DialogKind::OpenDirectory => SDL_ShowOpenFolderDialog(Some(answered), userdata, parent, location, false),
    }
  }
}

// Each filter's name and pattern, and the default location.
type CStrings = (Vec<(CString, CString)>, Option<CString>);

// The request's strings as C strings: each filter's name and `;`-joined
// pattern, and the default location. For the save dialog that is one file
// path: the suggested name joined onto the start directory (or standing on
// its own, which the platform resolves against its default folder).
fn c_strings(request: &DialogRequest) -> Result<CStrings, DialogError> {
  let strings = request
    .filters
    .iter()
    .map(|filter| Ok((c_string(&filter.name)?, c_string(&sdl_pattern(filter))?)))
    .collect::<Result<Vec<_>, DialogError>>()?;
  let start_in = request.start_in.as_deref();
  let default_location = match &request.kind {
    DialogKind::SaveFile { suggested_name } => match (start_in, suggested_name.as_deref()) {
      (Some(dir), Some(name)) => Some(std::path::Path::new(dir).join(name).to_string_lossy().into_owned()),
      (Some(dir), None) => Some(dir.to_string()),
      (None, Some(name)) => Some(name.to_string()),
      (None, None) => None,
    },
    DialogKind::OpenFile { .. } | DialogKind::OpenDirectory => start_in.map(str::to_string),
  };
  Ok((strings, default_location.as_deref().map(c_string).transpose()?))
}

fn c_string(s: &str) -> Result<CString, DialogError> {
  CString::new(s).map_err(|_| DialogError::Failed(format!("dialog: {s:?} contains a NUL character")))
}

// SDL's answer: a null list is an error (SDL's reason), an empty list a
// cancel, otherwise the picked paths. Runs once per shown dialog, on
// whatever thread SDL answers from; the responder must not panic, as this
// runs under SDL's C frames.
unsafe extern "C" fn answered(userdata: *mut c_void, filelist: *const *const c_char, _filter: c_int) {
  // SAFETY: `userdata` is the box `show` leaked for this dialog, and SDL
  // calls this once per dialog.
  let call = unsafe { Box::from_raw(userdata.cast::<Call>()) };
  let answer = if filelist.is_null() {
    Err(DialogError::Failed(format!("dialog: {}", crate::sdl_error())))
  } else {
    // SAFETY: a non-null list from SDL is a null-terminated array of
    // C strings, valid for the duration of this callback.
    unsafe { picked(filelist) }
  };
  let Call { respond, .. } = *call;
  respond(answer);
}

// The picked paths of SDL's null-terminated list.
//
// SAFETY (caller): `filelist` is non-null and null-terminated, and every
// entry before the terminator is a valid C string.
unsafe fn picked(filelist: *const *const c_char) -> DialogAnswer {
  let mut picks = Vec::new();
  let mut at = filelist;
  loop {
    // SAFETY: `at` stays within the list up to and including its null
    // terminator (the caller's contract).
    let entry = unsafe { *at };
    if entry.is_null() {
      return Ok(picks);
    }
    // SAFETY: a non-null entry of the list is a valid C string.
    let Ok(location) = unsafe { CStr::from_ptr(entry) }.to_str() else {
      return Err(DialogError::Failed("dialog: a picked path is not valid UTF-8".to_string()));
    };
    picks.push(Picked { name: name_of_path(location), location: location.to_string() });
    // SAFETY: the entry was not the terminator, so the next one is in the list.
    at = unsafe { at.add(1) };
  }
}
