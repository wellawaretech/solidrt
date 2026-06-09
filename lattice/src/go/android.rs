//! JNI bridge to the Android Java shell for QR scanning. The camera capture and
//! barcode decoding live in MainActivity/QrScanner (CameraX + ML Kit, no preview
//! surface); this module just starts/stops that scanner and receives decoded
//! results back through the `nativeOnQrScanned` JNI callback, forwarding them
//! into the dev-server connection supervisor as a `Connect`.

use std::sync::OnceLock;

use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::{JNIEnv, JavaVM};
use tokio::sync::mpsc::UnboundedSender;

use super::connection::DevCmd;

static DEV_CMD_TX: OnceLock<UnboundedSender<DevCmd>> = OnceLock::new();
static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();
static ACTIVITY: OnceLock<GlobalRef> = OnceLock::new();

/// Publish the supervisor command channel and capture the JavaVM + activity for
/// later cross-thread JNI calls. Runs on the SDL main thread (where
/// `SDL_GetAndroidJNIEnv` returns a valid env), called once from `connection::start`.
pub fn set_dev_cmd_tx(tx: UnboundedSender<DevCmd>) {
  let _ = DEV_CMD_TX.set(tx);
  if JAVA_VM.get().is_some() {
    return;
  }

  let env_ptr = unsafe { alloy::sdl3::sys::system::SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  let activity_ptr = unsafe { alloy::sdl3::sys::system::SDL_GetAndroidActivity() } as jni::sys::jobject;
  if env_ptr.is_null() || activity_ptr.is_null() {
    log::error!("[sgo] no Android JNI env/activity; QR scan unavailable");
    return;
  }

  let env = match unsafe { JNIEnv::from_raw(env_ptr) } {
    Ok(e) => e,
    Err(e) => {
      log::error!("[sgo] JNIEnv::from_raw failed: {e}");
      return;
    }
  };
  match env.get_java_vm() {
    Ok(vm) => {
      let _ = JAVA_VM.set(vm);
    }
    Err(e) => {
      log::error!("[sgo] get_java_vm failed: {e}");
      return;
    }
  }
  let activity = unsafe { JObject::from_raw(activity_ptr) };
  match env.new_global_ref(&activity) {
    Ok(g) => {
      let _ = ACTIVITY.set(g);
    }
    Err(e) => log::error!("[sgo] new_global_ref(activity) failed: {e}"),
  }
}

/// Ask the Java shell to start the camera QR scanner (requests the CAMERA
/// permission on first use). Called from the supervisor's tokio thread.
pub fn start_scanner() {
  call_activity_void("startQrScanner");
}

/// Ask the Java shell to stop the scanner and release the camera.
pub fn stop_scanner() {
  call_activity_void("stopQrScanner");
}

/// Invoke a no-arg void method on the cached activity, attaching the current
/// thread to the JVM for the duration of the call.
fn call_activity_void(method: &str) {
  let (Some(vm), Some(activity)) = (JAVA_VM.get(), ACTIVITY.get()) else {
    log::error!("[sgo] QR bridge not initialized; cannot call {method}");
    return;
  };
  let mut env = match vm.attach_current_thread() {
    Ok(e) => e,
    Err(e) => {
      log::error!("[sgo] attach_current_thread failed: {e}");
      return;
    }
  };
  if let Err(e) = env.call_method(activity.as_obj(), method, "()V", &[]) {
    log::error!("[sgo] {method} call failed: {e}");
    let _ = env.exception_clear();
  }
}

/// JNI callback: the Java scanner decoded a QR code. Its payload is the dev
/// server's `host:port`, so forward it as a `Connect` (which interrupts the
/// scan and switches the supervisor to a direct connection).
#[no_mangle]
pub extern "system" fn Java_com_solidrt_app_MainActivity_nativeOnQrScanned(
  mut env: JNIEnv,
  _class: JClass,
  content: JString,
) {
  let scanned: String = match env.get_string(&content) {
    Ok(s) => s.into(),
    Err(e) => {
      log::error!("[sgo] nativeOnQrScanned get_string failed: {e}");
      return;
    }
  };
  let addr = normalize_addr(&scanned);
  log::info!("[sgo] QR scanned: {scanned} -> connecting to {addr}");
  if let Some(tx) = DEV_CMD_TX.get() {
    let _ = tx.send(DevCmd::Connect(addr));
  }
}

/// The QR payload is a bare `host:port`; tolerate an optional ws/http scheme
/// prefix and a trailing slash in case the encoded value ever changes.
fn normalize_addr(raw: &str) -> String {
  let s = raw.trim();
  let s = s.strip_prefix("ws://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
  s.trim_end_matches('/').to_string()
}