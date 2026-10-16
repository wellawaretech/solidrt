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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use crate::backend::{pack_size, FrameOutput, GlBinding};
use crate::egl_headless::{HeadlessEgl, HeadlessEglBinding};
use crate::gl;
use crate::gpu::{SamplerState, ShaderStage, TextureFormat};
use crate::raster::{FrameSink, RasterState, RasterStats};

// The pbuffer and the fixtures: one pixel size for everything that has one.
const SURFACE: u32 = 16;
const UPLOAD_ID: u64 = 1;
const BUFFER_ID: u64 = 2;
const TARGET_ID: u64 = 3;
const VERTEX_ID: u64 = 4;
const FRAGMENT_ID: u64 = 5;
const PROGRAM_ID: u64 = 6;
const BUFFER_BYTES: usize = 64;

#[test]
fn recovery_rebuilds_every_object_behind_its_id() {
  // The raster half of a context loss (raster/recover.rs) on the headless EGL
  // pbuffer. Mesa cannot lose a context on demand, so the platform's reply
  // is a binding onto the same context: under test is the rebuild (every
  // registry re-created on the context handed back, behind the same ids,
  // and the report of what the app re-supplies), not the loss itself. The
  // window-recreation half is app.rs's recover_gl, checked on a device.
  let egl = match HeadlessEgl::new(SURFACE, SURFACE) {
    Ok(egl) => Arc::new(egl),
    Err(e) => {
      eprintln!("skipped: no headless EGL context here ({e})");
      return;
    }
  };
  let binding: Box<dyn GlBinding> = Box::new(HeadlessEglBinding(egl.clone()));
  assert!(binding.bind(), "GL make-current failed: {}", binding.error());
  let gl_ctx = gl::create_gl_context(&*binding);
  let impeller = gl::create_impeller_context(&*binding);
  let (tx, rx) = mpsc::channel();
  let stats = Arc::new(RasterStats::new());
  let surface_size = Arc::new(AtomicU64::new(pack_size(SURFACE, SURFACE)));
  let mut state =
    RasterState::new(gl_ctx, impeller, binding, surface_size, FrameSink::Discard, stats.clone(), tx, None);

  // What the UI thread registers: an uploaded texture, a buffer, a two-stage
  // program and a shader target (rendered at flush, so not the app's to
  // re-render).
  let pixels = vec![0u8; TextureFormat::Rgba8.byte_len(SURFACE, SURFACE)];
  let upload = state
    .create_texture(UPLOAD_ID, SURFACE, SURFACE, &pixels, SamplerState::default(), TextureFormat::Rgba8, None)
    .expect("upload texture");
  drop(upload);
  state.test_create_buffer(BUFFER_ID, &[0u8; BUFFER_BYTES], Some("vertices".into())).expect("buffer");
  let (target, _uniforms) = state
    .create_shader_texture(
      TARGET_ID,
      SURFACE,
      SURFACE,
      "void main() { fragColor = vec4(vUV, 0.0, 1.0); }",
      &[],
      Vec::new(),
      SamplerState::default(),
      None,
    )
    .expect("shader target");
  drop(target);
  state
    .test_compile_stage(VERTEX_ID, ShaderStage::Vertex, "void main() { gl_Position = vec4(0.0, 0.0, 0.0, 1.0); }")
    .expect("vertex stage");
  state
    .test_compile_stage(FRAGMENT_ID, ShaderStage::Fragment, "void main() { fragColor = vec4(1.0); }")
    .expect("fragment stage");
  state.link_program(PROGRAM_ID, VERTEX_ID, FRAGMENT_ID, None).expect("program");

  let before = state.resources();
  let old_upload = state.texture_name(UPLOAD_ID).expect("upload registered");
  let old_target = state.texture_name(TARGET_ID).expect("target registered");
  let old_buffer = state.buffer_name(BUFFER_ID).expect("buffer registered");

  // The platform half: answer the loss with a binding, then take the
  // restore report the way the platform loop does.
  let platform_egl = egl.clone();
  let platform = std::thread::spawn(move || {
    let reply = match rx.recv() {
      Ok(FrameOutput::ContextLost { reply, .. }) => reply,
      _ => panic!("the raster side asks for a context first"),
    };
    let fresh: Box<dyn GlBinding> = Box::new(HeadlessEglBinding(platform_egl));
    reply.send(Some(fresh)).expect("recover awaits the reply");
    match rx.recv() {
      Ok(FrameOutput::ContextRestored { handles, lost }) => {
        let mut ids: Vec<u64> = handles.into_iter().map(|(id, _)| id).collect();
        ids.sort_unstable();
        (ids, lost)
      }
      _ => panic!("the raster side reports the restore"),
    }
  });
  let state = state.recover("forced by the test".to_string()).expect("recovery on the pbuffer context");
  let (adopted, lost) = platform.join().expect("platform half");

  assert_eq!(stats.context_losses.load(Ordering::Relaxed), 1);
  assert!(state.awaiting_resync(), "frames are refused until the UI thread resyncs");

  // Every registry is back behind the same ids, as new GL objects (the old
  // names stay alive on this context, so a rebuilt one cannot reuse them).
  let after = state.resources();
  let ids = |infos: &[crate::gpu::GpuTextureInfo]| infos.iter().map(|t| t.id).collect::<Vec<u64>>();
  assert_eq!(ids(&after.textures), ids(&before.textures));
  assert_eq!(after.buffers.len(), before.buffers.len());
  assert_eq!(after.programs.len(), before.programs.len());
  assert_eq!(after.pipelines.len(), before.pipelines.len());
  assert_ne!(state.texture_name(UPLOAD_ID), Some(old_upload));
  assert_ne!(state.texture_name(TARGET_ID), Some(old_target));
  assert_ne!(state.buffer_name(BUFFER_ID), Some(old_buffer));

  // The app re-supplies what only it holds: the upload's pixels and the
  // buffer's bytes. The shader target is the engine's: dirty, re-rendered
  // at the next flush, not reported. Both 2D textures were adopted again
  // for Impeller.
  assert_eq!(lost.textures, vec![UPLOAD_ID]);
  assert_eq!(lost.buffers, vec![BUFFER_ID]);
  assert!(lost.targets.is_empty());
  assert!(state.is_dirty(TARGET_ID));
  assert_eq!(adopted, vec![UPLOAD_ID, TARGET_ID]);
}
