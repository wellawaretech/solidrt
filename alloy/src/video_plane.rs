//! The Android video plane: a SurfaceView beneath SDL's surface that a
//! hardware decoder renders straight into, composited by SurfaceFlinger
//! under the (translucent) UI. alloy owns it because alloy owns the window
//! and the JNI seam to the activity; the decoder is forge's and takes the
//! plane's `NativeWindow` as a plain platform handle. One plane at a time.
//! See okf/plans/android-video-punch-through.md.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use ndk::native_window::NativeWindow;
use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};

// Set from the activity (JNI) when the system destroyed the plane's surface
// under us - the activity went to the background. Reset at creation. A
// static rather than a field so the report has somewhere to land whatever
// thread it arrives on; there is one plane at a time.
static PLANE_LOST: AtomicBool = AtomicBool::new(false);

// The latest Choreographer frame time (CLOCK_MONOTONIC ns) the plane view
// reported, one per display frame while a plane is attached: the phase of
// the display's vsync grid. 0 = no sample yet (reset at creation). A static
// for the same reason as PLANE_LOST.
static PLANE_VSYNC_NS: AtomicI64 = AtomicI64::new(0);

/// Report that the platform destroyed the video plane's surface. Called
/// from the activity (Android JNI); thread-safe.
pub fn set_lost() {
  PLANE_LOST.store(true, Ordering::Relaxed);
}

/// Report one display vsync time (a Choreographer frame time, CLOCK_MONOTONIC
/// ns). Called from the plane view (Android JNI, UI thread); thread-safe.
pub fn set_vsync_ns(ns: i64) {
  PLANE_VSYNC_NS.store(ns, Ordering::Relaxed);
}

/// The most recent vsync time reported for the plane, on the decoder's
/// release clock (CLOCK_MONOTONIC ns); None before the first sample. Any
/// thread.
pub fn vsync_ns() -> Option<i64> {
  match PLANE_VSYNC_NS.load(Ordering::Relaxed) {
    0 => None,
    ns => Some(ns),
  }
}

/// How the video's picture fits the window on the plane (the compositor's
/// scale, free): letterboxed inside it, or cropped to fill it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneFit {
  Contain,
  Cover,
}

// VideoPlaneView.FIT_* on the Java side.
const FIT_CONTAIN: i32 = 0;
const FIT_COVER: i32 = 1;

/// A live video plane. Dropping it removes the view from the activity;
/// drop the decoder rendering into it first, so the codec has released
/// the surface by then.
pub struct VideoPlane {
  window: NativeWindow,
  refresh_period_ns: i64,
  // The Java view, held so teardown removes exactly this plane: a reload
  // drops the old engine after the new one opened its plane, so "remove the
  // current plane" would take the new view (seen on the tablet).
  view: jni::objects::Global<jni::objects::JObject<'static>>,
}

impl VideoPlane {
  /// Create the plane sized for a `video_width` x `video_height` picture.
  /// Blocks (on any thread; SDL attaches it to the JVM) until the surface
  /// exists, at most a couple of seconds. Errs when a plane already exists
  /// or the surface does not come up.
  pub fn create(video_width: u32, video_height: u32, fit: PlaneFit) -> Result<VideoPlane, String> {
    let fit_code = match fit {
      PlaneFit::Contain => FIT_CONTAIN,
      PlaneFit::Cover => FIT_COVER,
    };
    let (window, view, refresh_period_ns) = with_activity(|env, activity| {
      let view = env
        .call_method(
          activity,
          jni::jni_str!("createVideoPlane"),
          jni::jni_sig!("(III)Lcom/solidrt/app/VideoPlaneView;"),
          &[
            jni::objects::JValue::Int(video_width as i32),
            jni::objects::JValue::Int(video_height as i32),
            jni::objects::JValue::Int(fit_code),
          ],
        )?
        .l()?;
      if view.is_null() {
        return Ok(None);
      }
      let surface = env.call_method(&view, jni::jni_str!("surface"), jni::jni_sig!("()Landroid/view/Surface;"), &[])?.l()?;
      let refresh_period_ns = env.call_method(&view, jni::jni_str!("refreshPeriodNs"), jni::jni_sig!("()J"), &[])?.j()?;
      // The casts bridge the jni crate (jni-sys 0.4) and ndk-sys (jni-sys
      // 0.3), which name the same ABI types in different crates. The
      // NativeWindow holds its own reference to the surface; the local
      // Surface ref goes with the env frame.
      let window = unsafe { NativeWindow::from_surface(env.get_raw() as *mut _, surface.as_raw() as *mut _) };
      let global = env.new_global_ref(&view)?;
      Ok(window.map(|w| (w, global, refresh_period_ns)))
    })?
    .ok_or_else(|| "video plane not created (one exists already, or its surface did not come up)".to_string())?;
    PLANE_LOST.store(false, Ordering::Relaxed);
    PLANE_VSYNC_NS.store(0, Ordering::Relaxed);
    Ok(VideoPlane { window, view, refresh_period_ns })
  }

  /// The surface a decoder is configured with.
  pub fn native_window(&self) -> &NativeWindow {
    &self.window
  }

  /// The display's refresh period in ns (the vsync grid's spacing; the
  /// phase is `vsync_ns()`), None when the display did not report a rate.
  pub fn refresh_period_ns(&self) -> Option<i64> {
    (self.refresh_period_ns > 0).then_some(self.refresh_period_ns)
  }

  /// Whether the platform destroyed the surface beneath this plane (the
  /// activity was backgrounded): the decoder rendering into it must stop
  /// and the plane is finished.
  pub fn lost(&self) -> bool {
    PLANE_LOST.load(Ordering::Relaxed)
  }
}

impl Drop for VideoPlane {
  fn drop(&mut self) {
    let result = with_activity(|env, activity| {
      env.call_method(
        activity,
        jni::jni_str!("destroyVideoPlane"),
        jni::jni_sig!("(Lcom/solidrt/app/VideoPlaneView;)V"),
        &[jni::objects::JValue::Object(self.view.as_obj())],
      )?;
      Ok(())
    });
    if let Err(e) = result {
      log::warn!("[alloy] video plane teardown: {e}");
    }
  }
}

/// Run `f` against SDL's JNI env and the activity object on the current
/// thread. A JNI failure or a Java exception comes back as the error.
fn with_activity<T>(
  f: impl FnOnce(&mut jni::Env<'_>, &jni::objects::JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
  let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
  if env_ptr.is_null() || activity_ptr.is_null() {
    return Err("no JNI env/activity for the video plane".to_string());
  }
  let mut unowned = unsafe { jni::EnvUnowned::from_raw(env_ptr) };
  let outcome = unowned
    .with_env(|env| {
      let activity = unsafe { jni::objects::JObject::from_raw(env, activity_ptr) };
      f(env, &activity)
    })
    .into_outcome();
  match outcome {
    jni::Outcome::Ok(value) => Ok(value),
    jni::Outcome::Err(e) => Err(format!("video plane JNI call failed: {e}")),
    jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
  }
}
