//! The Android dialogs, through the activity's own document intents
//! (`SolidRTActivity.showDialog`): the activity owns the intents and
//! reports the result over JNI (`nativeDialogResult`, which the embedder
//! forwards to `result_raw`). Any thread: `with_activity` attaches it.

use super::{DialogError, DialogKind, DialogRequest, DialogResponder, Picked};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

// The dialogs the activity has open, by request id, each with the
// responder its result settles.
static PENDING: Mutex<Option<HashMap<u64, DialogResponder>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

// The `kind` argument of `SolidRTActivity.showDialog`.
const KIND_OPEN_FILE: i32 = 0;
const KIND_SAVE_FILE: i32 = 1;
const KIND_OPEN_DIRECTORY: i32 = 2;

fn take(id: u64) -> Option<DialogResponder> {
  PENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut().and_then(|map| map.remove(&id))
}

pub(super) fn show(request: DialogRequest, respond: DialogResponder) {
  let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  PENDING
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .get_or_insert_with(HashMap::new)
    .insert(id, respond);
  let (kind, multiple, suggested_name) = match request.kind {
    DialogKind::OpenFile { multiple } => (KIND_OPEN_FILE, multiple, None),
    DialogKind::SaveFile { suggested_name } => (KIND_SAVE_FILE, false, suggested_name),
    DialogKind::OpenDirectory => (KIND_OPEN_DIRECTORY, false, None),
  };
  let patterns: Vec<String> = request.filters.iter().map(super::sdl_pattern).collect();
  let launched = crate::android::with_activity("dialog", |env, activity| {
    let empty = env.new_string("")?;
    let filters = jni::objects::JObjectArray::<jni::objects::JString>::new(env, patterns.len(), &empty)?;
    for (i, pattern) in patterns.iter().enumerate() {
      let s = env.new_string(pattern)?;
      filters.set_element(env, i, &s)?;
    }
    let suggested = env.new_string(suggested_name.as_deref().unwrap_or(""))?;
    let start_in = env.new_string(request.start_in.as_deref().unwrap_or(""))?;
    let class = env.get_object_class(activity)?;
    env
      .call_static_method(
        &class,
        jni::jni_str!("showDialog"),
        jni::jni_sig!("(JI[Ljava/lang/String;ZLjava/lang/String;Ljava/lang/String;)Z"),
        &[
          jni::objects::JValue::Long(id as i64),
          jni::objects::JValue::Int(kind),
          jni::objects::JValue::Object(&filters),
          jni::objects::JValue::Bool(multiple),
          jni::objects::JValue::Object(&suggested),
          jni::objects::JValue::Object(&start_in),
        ],
      )?
      .z()
  });
  let failed = match launched {
    Ok(true) => None,
    Ok(false) => Some("dialog: no document picker on this device".to_string()),
    Err(e) => Some(e),
  };
  if let Some(message) = failed {
    if let Some(respond) = take(id) {
      respond(Err(DialogError::Failed(message)));
    }
  }
}

/// The activity's answer to dialog `id` (`nativeDialogResult`): the
/// picked URIs with their display names, both empty on a cancel.
pub fn result(id: u64, locations: Vec<String>, names: Vec<String>) {
  let Some(respond) = take(id) else {
    log::warn!("[ore] dialog result for unknown request {id}");
    return;
  };
  let picked = locations
    .into_iter()
    .zip(names.into_iter().chain(std::iter::repeat(String::new())))
    .map(|(location, name)| {
      let name = if name.is_empty() { super::name_of_path(&location) } else { name };
      Picked { location, name }
    })
    .collect();
  respond(Ok(picked));
}

/// `result` from the raw JNI arguments the activity's native method
/// receives (the env, then the two `String[]`), for the embedder's
/// exported symbol to forward as the opaque pointers it gets.
pub fn result_raw(env: *mut core::ffi::c_void, id: i64, uris: *mut core::ffi::c_void, names: *mut core::ffi::c_void) {
  let env = env as *mut jni::sys::JNIEnv;
  let (uris, names) = (uris as jni::sys::jobjectArray, names as jni::sys::jobjectArray);
  let decoded = decode_strings(env, uris).and_then(|uris| decode_strings(env, names).map(|names| (uris, names)));
  match decoded {
    Ok((uris, names)) => result(id as u64, uris, names),
    Err(e) => {
      log::warn!("[ore] dialog result for request {id} could not be read: {e}");
      if let Some(respond) = take(id as u64) {
        respond(Err(DialogError::Failed(format!("dialog: the result could not be read: {e}"))));
      }
    }
  }
}

fn decode_strings(env: *mut jni::sys::JNIEnv, array: jni::sys::jobjectArray) -> Result<Vec<String>, String> {
  if env.is_null() {
    return Err("no JNI env".to_string());
  }
  if array.is_null() {
    return Ok(Vec::new());
  }
  // SAFETY: `env` is the non-null JNI env the activity's native method
  // received, on this thread, for the duration of that call.
  let mut unowned = unsafe { jni::EnvUnowned::from_raw(env) };
  let outcome = unowned
    .with_env(|env| -> Result<Vec<String>, jni::errors::Error> {
      // SAFETY: `array` is the non-null `String[]` local reference the
      // native method received, valid for that call on this env.
      let array = unsafe { jni::objects::JObjectArray::<jni::objects::JString>::from_raw(env, array) };
      crate::android::java_strings(env, &array)
    })
    .into_outcome();
  match outcome {
    jni::Outcome::Ok(strings) => Ok(strings),
    jni::Outcome::Err(e) => Err(e.to_string()),
    jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
  }
}
