//! The share sheet: the platform's own panel that hands files and a text
//! to another app the user picks (a messenger, mail, a cloud drive).
//! Android only, for now: an `ACTION_SEND` through the chooser, started by
//! the activity (`SolidRTActivity.share`, reached over JNI on the JavaVM
//! and activity alloy publishes in ndk-context; no SDL). Desktop sheets
//! (macOS's picker, Windows' share UI) are additive later.
//!
//! A share settles when the sheet closes: shared once the user picked a
//! target (the chooser's chosen-target broadcast, at the pick) or used one
//! of the sheet's own actions (Copy), cancelled when the sheet closed with
//! neither. Nothing reports whether the target finished the send. Files go
//! by reference: a file in app storage as a `content://` URI the activity's
//! FileProvider serves, a document a file dialog picked as the URI it is,
//! each with a read grant; the target reads the bytes when it likes, so a
//! shared file has to stay in place after the share settles. Only the text
//! travels inside the intent.
//!
//! `Session` is the session an embedder keeps per engine, the shape of
//! `link::Session`: every share is logged, and a session set to record
//! only shows no sheet and answers at once, as a test host's engine does.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// How many shares the log keeps, the newest last: what a test or an
/// agent reads back, without an app that shares in a loop growing it.
pub(crate) const REQUESTED_LOG_CAP: usize = 64;

/// One file to share: where it is (a path, or a `content://` URI a file
/// dialog picked) and the name to show for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedFile {
  pub location: String,
  pub name: String,
}

/// What the app hands the sheet; at least one field is set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShareRequest {
  /// The subject a target may use (a mail's subject line).
  pub title: Option<String>,
  pub text: Option<String>,
  /// Sent after the text, as one more line of it.
  pub url: Option<String>,
  pub files: Vec<SharedFile>,
}

/// Why a share did not go out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareError {
  /// The user closed the sheet without picking a target.
  Cancelled,
  /// A sheet is already open: one at a time.
  Busy,
  /// This platform has no share sheet.
  Unavailable,
  /// A file the platform cannot hand out: a path outside app storage.
  NotShareable(String),
  /// The platform could not show the sheet.
  Failed(String),
}

impl std::fmt::Display for ShareError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ShareError::Cancelled => f.write_str("the share was cancelled"),
      ShareError::Busy => f.write_str("a share sheet is already open"),
      ShareError::Unavailable => f.write_str("no share sheet on this platform"),
      ShareError::NotShareable(location) => {
        write!(f, "{location} cannot be shared: only files in app storage and picked documents can")
      }
      ShareError::Failed(message) => f.write_str(message),
    }
  }
}

/// What a share is answered through, once, from the thread it settled on.
pub type ShareResponder = Box<dyn FnOnce(Result<(), ShareError>) + Send>;

/// Whether this platform can show a share sheet at all.
pub fn available() -> bool {
  cfg!(target_os = "android")
}

/// The share session of one engine (see the module doc).
pub struct Session {
  record_only: AtomicBool,
  requested: Mutex<VecDeque<ShareRequest>>,
}

impl Session {
  pub fn new() -> Arc<Session> {
    Arc::new(Session { record_only: AtomicBool::new(false), requested: Mutex::new(VecDeque::new()) })
  }

  /// Never show a sheet in this session: every share is logged and
  /// resolves (a test host's engine, a render).
  pub fn set_record_only(&self) {
    self.record_only.store(true, Ordering::Relaxed);
  }

  pub fn is_record_only(&self) -> bool {
    self.record_only.load(Ordering::Relaxed)
  }

  /// The shares the app asked for in this session, oldest first, the last
  /// `REQUESTED_LOG_CAP` of them, whatever the sheet answered.
  pub fn requested(&self) -> Vec<ShareRequest> {
    self.requested.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
  }

  /// Show the sheet for `request` and answer through `respond` once it
  /// closes, logging the request first; a record-only session answers at
  /// once.
  pub fn share(&self, request: ShareRequest, respond: ShareResponder) {
    {
      let mut requested = self.requested.lock().unwrap_or_else(PoisonError::into_inner);
      if requested.len() == REQUESTED_LOG_CAP {
        requested.pop_front();
      }
      requested.push_back(request.clone());
    }
    if self.is_record_only() {
      return respond(Ok(()));
    }
    #[cfg(target_os = "android")]
    android::show(request, respond);
    #[cfg(not(target_os = "android"))]
    respond(Err(ShareError::Unavailable));
  }
}

/// The activity's answer to share `id` (`nativeShareResult`): whether it
/// went out. For the embedder's exported JNI symbol to forward.
#[cfg(target_os = "android")]
pub fn android_result(id: i64, shared: bool) {
  android::result(id, shared);
}

#[cfg(target_os = "android")]
mod android {
  use super::{ShareError, ShareRequest, ShareResponder};
  use std::sync::atomic::{AtomicI64, Ordering};
  use std::sync::{Mutex, PoisonError};

  // The share the activity has open, with the responder its result
  // settles: one sheet at a time.
  static PENDING: Mutex<Option<(i64, ShareResponder)>> = Mutex::new(None);
  static NEXT_ID: AtomicI64 = AtomicI64::new(1);

  // What `SolidRTActivity.share` returns besides the index of a file it
  // cannot hand out.
  const STARTED: i32 = -1;
  const NOT_STARTED: i32 = -2;

  pub(super) fn show(request: ShareRequest, respond: ShareResponder) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    {
      let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
      if pending.is_some() {
        drop(pending);
        return respond(Err(ShareError::Busy));
      }
      *pending = Some((id, respond));
    }
    // A relative path is the app's storage, relative to the process's
    // working directory; Java resolves a relative `File` against "/", so
    // the path goes out absolute.
    let locations: Vec<String> = request
      .files
      .iter()
      .map(|f| {
        if f.location.starts_with("content://") {
          f.location.clone()
        } else {
          std::path::absolute(&f.location)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| f.location.clone())
        }
      })
      .collect();
    let names: Vec<String> = request.files.iter().map(|f| f.name.clone()).collect();
    let started = crate::android::with_activity("share", |env, activity| {
      let empty = env.new_string("")?;
      let jlocations = jni::objects::JObjectArray::<jni::objects::JString>::new(env, locations.len(), &empty)?;
      for (i, location) in locations.iter().enumerate() {
        let s = env.new_string(location)?;
        jlocations.set_element(env, i, &s)?;
      }
      let jnames = jni::objects::JObjectArray::<jni::objects::JString>::new(env, names.len(), &empty)?;
      for (i, name) in names.iter().enumerate() {
        let s = env.new_string(name)?;
        jnames.set_element(env, i, &s)?;
      }
      let title = env.new_string(request.title.as_deref().unwrap_or(""))?;
      let text = env.new_string(request.text.as_deref().unwrap_or(""))?;
      let url = env.new_string(request.url.as_deref().unwrap_or(""))?;
      let class = env.get_object_class(activity)?;
      env
        .call_static_method(
          &class,
          jni::jni_str!("share"),
          jni::jni_sig!(
            "(J[Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I"
          ),
          &[
            jni::objects::JValue::Long(id),
            jni::objects::JValue::Object(&jlocations),
            jni::objects::JValue::Object(&jnames),
            jni::objects::JValue::Object(&title),
            jni::objects::JValue::Object(&text),
            jni::objects::JValue::Object(&url),
          ],
        )?
        .i()
    });
    let failed = match started {
      Ok(STARTED) => None,
      Ok(NOT_STARTED) => Some(ShareError::Failed("share: the sheet could not be shown".to_string())),
      Ok(index) => Some(ShareError::NotShareable(
        usize::try_from(index).ok().and_then(|i| locations.get(i)).cloned().unwrap_or_default(),
      )),
      Err(e) => Some(ShareError::Failed(e)),
    };
    if let Some(error) = failed {
      if let Some(respond) = take(id) {
        respond(Err(error));
      }
    }
  }

  fn take(id: i64) -> Option<ShareResponder> {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    match pending.take() {
      Some((pending_id, respond)) if pending_id == id => Some(respond),
      other => {
        *pending = other;
        None
      }
    }
  }

  pub(super) fn result(id: i64, shared: bool) {
    let Some(respond) = take(id) else {
      log::warn!("[ore] share result for unknown request {id}");
      return;
    };
    respond(if shared { Ok(()) } else { Err(ShareError::Cancelled) });
  }
}
