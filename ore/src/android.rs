//! The JNI way into the activity: the process-wide JavaVM and activity
//! alloy publishes in ndk-context at startup, reached from whatever thread
//! a facility is called on (the thread is attached to the VM for its
//! lifetime). The activity's static helpers on `SolidRTActivity` are what
//! a facility calls through it.

use jni::objects::JObject;

/// Run `f` with a JNI env on this thread and the activity.
pub(crate) fn with_activity<T>(
  what: &str,
  f: impl FnOnce(&mut jni::Env<'_>, &JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
  let context = ndk_context::android_context();
  if context.vm().is_null() || context.context().is_null() {
    return Err(format!("no Android context for the {what}"));
  }
  // SAFETY: ndk-context holds the process's JavaVM, valid for the life of
  // the process once published.
  let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
  vm.attach_current_thread(|env| {
    // SAFETY: the activity is a global reference the platform keeps for
    // the process; this wrapper does not own it and never deletes it.
    let activity = unsafe { JObject::from_raw(env, context.context().cast()) };
    f(env, &activity)
  })
  .map_err(|e: jni::errors::Error| format!("{what} JNI call failed: {e}"))
}
