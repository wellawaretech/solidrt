use crate::gl::ResetStatus;
use crate::raster::is_context_lost_error;

#[test]
fn sdl_error_text_names_a_lost_context() {
  // SDL's swap and make-current errors carry the EGL error name (measured
  // on ANGLE/D3D11 after a device removal).
  assert!(is_context_lost_error(
    "unable to show color buffer in an OS-native window (call to eglSwapBuffers failed, reporting an error of EGL_CONTEXT_LOST)"
  ));
  assert!(is_context_lost_error(
    "Unable to make EGL context current (call to eglMakeCurrent failed, reporting an error of EGL_CONTEXT_LOST)"
  ));
  // The Android background case is "no surface", not a loss.
  assert!(!is_context_lost_error("eglSwapBuffers failed, reporting an error of EGL_BAD_SURFACE"));
  assert!(!is_context_lost_error("The specified window has not been made current"));
}

#[test]
fn reset_status_maps_the_robustness_tokens() {
  assert_eq!(ResetStatus::from_gl(0), ResetStatus::None);
  assert!(!ResetStatus::None.is_reset());
  assert_eq!(ResetStatus::from_gl(0x8253), ResetStatus::Guilty);
  assert_eq!(ResetStatus::from_gl(0x8254), ResetStatus::Innocent);
  assert_eq!(ResetStatus::from_gl(0x8255), ResetStatus::Unknown);
  // A token outside the spec's three is still a reset, never ignored.
  assert_eq!(ResetStatus::from_gl(0x1234), ResetStatus::Unknown);
  assert!(ResetStatus::Guilty.is_reset());
}
