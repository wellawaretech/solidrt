//! Android's held redraws. SDL's SurfaceView sits beneath a window that is
//! see-through over it, so a surface without a frame shows whatever is
//! beneath the app (the launcher, during a launch). SolidRTSurface.java
//! therefore holds each redraw the platform asks of the surface - the
//! window's own draw, the splash's removal - until the engine has drawn into
//! it. The ask arrives here with a generation, the main loop decides which
//! swap answers it (see alloy's liveness.rs), and the answer goes back with
//! that generation, so a late answer to an older ask never releases a newer
//! one. Other platforms never ask.

use std::sync::Mutex;

use crate::platform::Waker;

static PENDING: Mutex<Option<i32>> = Mutex::new(None);
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// The pump's waker, so an ask arriving while it sleeps on the SDL event
/// queue is noticed. Set once, by `Platform::init`.
pub(crate) fn set_waker(waker: Waker) {
  *WAKER.lock().expect("redraw waker lock poisoned") = Some(waker);
}

/// The platform holds a redraw until the surface has a frame. Called on the
/// Android UI thread; at a launch this comes before the main loop exists,
/// and the loop takes it on its first iteration.
pub fn request(generation: i32) {
  *PENDING.lock().expect("redraw lock poisoned") = Some(generation);
  if let Some(waker) = WAKER.lock().expect("redraw waker lock poisoned").as_ref() {
    waker.wake();
  }
}

/// The ask since the last take, if any; a newer one replaces an older.
pub fn take() -> Option<i32> {
  PENDING.lock().expect("redraw lock poisoned").take()
}

/// Answer the held redraw `generation`: a frame built after the ask has
/// reached the surface.
pub fn answer(generation: i32) {
  #[cfg(target_os = "android")]
  if let Err(e) = crate::android::with_activity("redraw answer", |env, activity| {
    let class = env.get_object_class(activity)?;
    env.call_static_method(
      &class,
      jni::jni_str!("redrawPresented"),
      jni::jni_sig!("(I)V"),
      &[jni::objects::JValue::Int(generation)],
    )?;
    Ok(())
  }) {
    log::warn!("[ore] {e}; the platform shows the window at its own timeout");
  }
  #[cfg(not(target_os = "android"))]
  let _ = generation;
}
