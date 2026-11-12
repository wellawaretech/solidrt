//! Runtime permissions: asking the user for the right to use a device.
//! One contract on every platform: `request` answers once, with whether
//! the app may go on. Android asks with its runtime prompt. The other
//! platforms have no prompt an app raises ahead of use (the OS asks at the
//! device open, or not at all), so there the answer is yes, at once, and
//! the open that follows reports what the OS decided.
//!
//! The first shape of the one model across camera, microphone, location,
//! notifications and exact alarms (okf/design/platform-layering.md,
//! "Permissions"); today it holds the microphone alone.

/// What an app asks the user for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
  /// Recording from an audio input device.
  Microphone,
}

/// Ask for `permission` without waiting for the answer: `on_answer` runs
/// once with whether it was granted, on a thread of the platform's choice
/// (Android: the activity's thread), or before this returns when no prompt
/// is needed (already held on Android, every time elsewhere). SDL's own
/// audio recording open asks Android for the microphone with a blocking
/// callback (SDL_aaudio.c spins until the answer), so a caller asks here
/// first and opens once granted, when SDL's own request answers at once.
/// The error is the platform's text when the request could not be made;
/// `on_answer` is dropped unrun then.
pub fn request(permission: Permission, on_answer: impl FnOnce(bool) + Send + 'static) -> Result<(), String> {
  #[cfg(target_os = "android")]
  {
    android::request(permission, Box::new(on_answer))
  }
  #[cfg(not(target_os = "android"))]
  {
    let _ = permission;
    on_answer(true);
    Ok(())
  }
}

#[cfg(target_os = "android")]
mod android {
  use std::ffi::{c_char, c_void, CStr};
  use std::panic::AssertUnwindSafe;

  use sdl3::sys::system::SDL_RequestAndroidPermission;

  use super::Permission;

  type Answer = Box<dyn FnOnce(bool) + Send>;

  /// The Android name of a permission.
  fn name(permission: Permission) -> &'static CStr {
    match permission {
      Permission::Microphone => c"android.permission.RECORD_AUDIO",
    }
  }

  pub(super) fn request(permission: Permission, on_answer: Answer) -> Result<(), String> {
    let lent = Box::into_raw(Box::new(on_answer));
    // SAFETY: the name is a static C string; `lent` is an owned allocation
    // that `answered` takes back exactly once, or this function does below
    // when SDL refuses the request and never calls it.
    if unsafe { SDL_RequestAndroidPermission(name(permission).as_ptr(), Some(answered), lent as *mut c_void) } {
      return Ok(());
    }
    // SAFETY: SDL did not take the request, so `lent` is still ours alone.
    drop(unsafe { Box::from_raw(lent) });
    Err(crate::sdl_error())
  }

  /// SDL's answer to a request: runs once, on the activity's thread. A
  /// panic must not cross the C boundary (it would abort), so it ends here.
  unsafe extern "C" fn answered(userdata: *mut c_void, _permission: *const c_char, granted: bool) {
    // SAFETY: `userdata` is the allocation `request` lent, handed back once.
    let on_answer = unsafe { Box::from_raw(userdata as *mut Answer) };
    if std::panic::catch_unwind(AssertUnwindSafe(move || on_answer(granted))).is_err() {
      log::error!("[ore] A permission answer panicked");
    }
  }
}
