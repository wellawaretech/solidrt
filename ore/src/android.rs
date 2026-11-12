//! The JNI way into the activity, and the facts only Android has. The
//! process-wide JavaVM and activity are published once in ndk-context
//! (`init_context`), where this crate, forge and JNI-using dependencies
//! read them; a facility reaches the activity through `with_activity`
//! from whatever thread it is called on (the thread is attached to the VM
//! for its lifetime). The activity's static helpers on `SolidRTActivity`
//! are what a facility calls through it.
//!
//! `with_activity` and `java_strings` are public for the crates whose
//! activity calls have not moved here yet (alloy's dialogs, video plane and
//! redraw answer, forge's content providers). They hand out `jni` types,
//! which no ore contract does; they go back to crate-private when those
//! calls move.

use std::path::PathBuf;
use std::sync::OnceLock;

use jni::objects::{JObject, JObjectArray, JString, JValue};

/// Publish the JavaVM and the activity in the process-wide ndk-context,
/// once: later calls answer what the first one did. JNI-using dependencies
/// read them there (a network monitor on its own threads, forge's content
/// providers), and so does `with_activity`, which makes this call itself;
/// a host calls it at startup so that a dependency that never goes through
/// ore finds the context too. Both come from SDL, which holds them from
/// the moment the activity loaded the native side, before `SDL_Init`.
/// False (logged) when SDL has no JNI env or no activity: nothing is
/// published then, and every facility here reports no Android context.
pub fn init_context() -> bool {
  static PUBLISHED: OnceLock<bool> = OnceLock::new();
  *PUBLISHED.get_or_init(|| match publish_context() {
    Ok(()) => true,
    Err(e) => {
      log::error!("[ore] Android context not published: {e}");
      false
    }
  })
}

fn publish_context() -> Result<(), String> {
  use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};
  // SAFETY: both calls take no arguments; SDL attaches the calling thread
  // to the VM and returns null when it has no env or no activity.
  let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  if env_ptr.is_null() {
    return Err("SDL has no JNI env".to_string());
  }
  // SAFETY: as above; the result is a local reference owned by this call.
  let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
  if activity_ptr.is_null() {
    return Err("SDL has no activity".to_string());
  }
  // SAFETY: `env_ptr` is the non-null JNI env of this thread, checked above.
  let mut unowned = unsafe { jni::EnvUnowned::from_raw(env_ptr) };
  let outcome = unowned
    .with_env(|env| -> Result<(), jni::errors::Error> {
      let vm = env.get_java_vm()?;
      // SAFETY: `activity_ptr` is the non-null local reference SDL handed
      // out for this thread, wrapped once and deleted below.
      let activity = unsafe { JObject::from_raw(env, activity_ptr) };
      // A local reference is valid on this thread only, and the context is
      // read from any thread: promote it to a global one, kept for the
      // life of the process (`into_raw`, never deleted).
      let global = env.new_global_ref(&activity)?.into_raw();
      env.delete_local_ref(activity);
      // SAFETY: the VM and the global reference live as long as the
      // process, and the `OnceLock` in `init_context` makes this the only
      // publish (ndk-context asserts on a second one).
      unsafe {
        ndk_context::initialize_android_context(vm.get_raw() as *mut std::ffi::c_void, global as *mut std::ffi::c_void);
      }
      Ok(())
    })
    .into_outcome();
  match outcome {
    jni::Outcome::Ok(()) => Ok(()),
    jni::Outcome::Err(e) => Err(e.to_string()),
    jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
  }
}

/// Run `f` with a JNI env on this thread and the activity. A JNI failure
/// or a Java exception comes back as the error, prefixed with `what`.
/// Local references `f` makes are freed when it returns (the call runs in
/// a JNI frame of its own), so it hands back plain values or global
/// references only.
pub fn with_activity<T>(
  what: &str,
  f: impl FnOnce(&mut jni::Env<'_>, &JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
  if !init_context() {
    return Err(format!("no Android context for the {what}"));
  }
  let context = ndk_context::android_context();
  // SAFETY: ndk-context holds the process's JavaVM, valid for the life of
  // the process once published.
  let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
  vm.attach_current_thread(|env| {
    // SAFETY: the activity is a global reference kept for the process;
    // this wrapper does not own it and never deletes it.
    let activity = unsafe { JObject::from_raw(env, context.context().cast()) };
    f(env, &activity)
  })
  .map_err(|e: jni::errors::Error| format!("{what} JNI call failed: {e}"))
}

/// The strings of a Java `String[]`, in order.
pub fn java_strings(
  env: &mut jni::Env<'_>,
  array: &JObjectArray<'_, JString<'_>>,
) -> Result<Vec<String>, jni::errors::Error> {
  let len = array.len(env)?;
  let mut out = Vec::with_capacity(len);
  for i in 0..len {
    let element = array.get_element(env, i)?;
    out.push(element.mutf8_chars(env)?.to_str().into_owned());
  }
  Ok(out)
}

/// Android's own answer to "does this device have a touchscreen"
/// (PackageManager.hasSystemFeature). SDL's touch-device enumeration
/// over-reports on TV boxes whose virtual input drivers claim pointer
/// sources (measured: a Philips TPM171E lists a touch device while the
/// platform declares no android.hardware.touchscreen feature), so a touch
/// fact gates on the platform feature too. Cached: hardware features
/// cannot change at runtime. Errors default to true so a JNI hiccup can
/// only ever fail toward the touch-device default, never strip a real
/// touchscreen of its policy.
pub fn has_touchscreen_feature() -> bool {
  static FEATURE: OnceLock<bool> = OnceLock::new();
  *FEATURE.get_or_init(|| {
    let queried = with_activity("touchscreen feature query", |env, activity| {
      let pm = env
        .call_method(
          activity,
          jni::jni_str!("getPackageManager"),
          jni::jni_sig!("()Landroid/content/pm/PackageManager;"),
          &[],
        )?
        .l()?;
      let feature = env.new_string("android.hardware.touchscreen")?;
      env
        .call_method(
          &pm,
          jni::jni_str!("hasSystemFeature"),
          jni::jni_sig!("(Ljava/lang/String;)Z"),
          &[JValue::Object(&feature)],
        )?
        .z()
    });
    queried.unwrap_or_else(|e| {
      log::warn!("[ore] {e}; assuming touch");
      true
    })
  })
}

/// Locate a stored (uncompressed) asset inside this app's own APK: the APK's
/// absolute path (`ApplicationInfo.sourceDir`) plus the asset's byte offset
/// and length within it. A host uses this to read a payload packed into the
/// APK in place (ranged reads against the APK, no extraction). The file
/// descriptor Android opens to report the offset is closed again here;
/// readers reopen the APK by path. None (with a warning) when the asset is
/// absent or compressed - the caller treats that as "no payload".
pub fn packed_asset_location(name: &str) -> Option<(PathBuf, u64, u64)> {
  let asset_name = std::ffi::CString::new(name).ok()?;
  let located = with_activity("packed asset lookup", |env, activity| {
    let info = env
      .call_method(
        activity,
        jni::jni_str!("getApplicationInfo"),
        jni::jni_sig!("()Landroid/content/pm/ApplicationInfo;"),
        &[],
      )?
      .l()?;
    let source_dir = env.get_field(&info, jni::jni_str!("sourceDir"), jni::jni_sig!("Ljava/lang/String;"))?.l()?;
    let apk = env.cast_local::<JString>(source_dir)?.try_to_string(env)?;
    let assets = env
      .call_method(activity, jni::jni_str!("getAssets"), jni::jni_sig!("()Landroid/content/res/AssetManager;"), &[])?
      .l()?;
    // The casts bridge the jni crate (jni-sys 0.4) and ndk-sys (jni-sys
    // 0.3), which name the same ABI types in different crates.
    // SAFETY: the env is this thread's and `assets` a live local reference
    // to the Java AssetManager. The AAssetManager it returns is owned by
    // that Java object, which the activity keeps for the life of the
    // process; no ownership transfers here.
    let manager = unsafe { ndk_sys::AAssetManager_fromJava(env.get_raw() as *mut _, assets.as_raw() as *mut _) };
    let Some(manager) = std::ptr::NonNull::new(manager) else { return Ok(None) };
    // SAFETY: non-null, and valid for the life of the process (see above).
    let manager = unsafe { ndk::asset::AssetManager::from_ptr(manager) };
    let Some(asset) = manager.open(&asset_name) else { return Ok(None) };
    // Fails for a compressed asset, which has no contiguous bytes to point
    // at - a packer stores the payload uncompressed for this reason.
    let fd = match asset.open_file_descriptor() {
      Ok(fd) => fd,
      Err(e) => {
        log::warn!("[ore] Asset {name} has no readable file range (stored uncompressed?): {e}");
        return Ok(None);
      }
    };
    Ok(Some((PathBuf::from(apk), fd.offset as u64, fd.size as u64)))
  });
  located.unwrap_or_else(|e| {
    log::warn!("[ore] {e}");
    None
  })
}
