//! Window composition: routing the frame draw through the window shader's
//! retained layer (with its clean-tree pass-only fast path and uPrevious
//! history rotation) or straight to FBO 0, and the stats overlay composited
//! over the finished frame. The layer/overlay state structs live with
//! RasterState in the parent module.

use impellers::{DisplayList, ISize};

use super::offscreen::flip_for_fbo;
use super::repaint::WindowRoute;
use super::{ensure_copy_program, LayerTarget, OverlayState, RasterState, WindowShaderState};
use crate::gl;
use crate::gl::release_program;
use crate::gl::PassInput;
use crate::gpu::WindowShader;

impl RasterState {
  /// Draw the display list to the window backbuffer along `route`; true when
  /// a frame reached it. False skips the frame (a zero-sized minimized
  /// window, or a failed draw) - the caller still notifies, so lockstep
  /// consumers (playback) never stall.
  pub(super) fn draw_to_window(&mut self, dl: &DisplayList, size: ISize, route: WindowRoute) -> bool {
    // Resize-race diagnostics: geometry transitions as this thread sees them,
    // once per size.
    if self.last_size.width != size.width || self.last_size.height != size.height {
      log::info!(
        "[alloy] frame size {}x{} -> {}x{}",
        self.last_size.width,
        self.last_size.height,
        size.width,
        size.height
      );
      self.last_size = size;
    }
    if size.width <= 0 || size.height <= 0 {
      return false;
    }
    if self.window_shader.is_some() {
      match self.draw_to_window_shaded(dl, size) {
        Ok(()) => return true,
        // Fall back to the plain path so the app stays visible; the layer or
        // pass failure is a diagnostic, not a black window.
        Err(e) => log::warn!("[alloy] window shader pass failed: {e}; drawing without it"),
      }
    }
    match gl::render_display_list_to_window(&self.gl, &mut self.impeller_ctx, &mut self.offscreen_rig, dl, size, route)
    {
      Ok(()) => true,
      Err(e) => {
        log::warn!("[alloy] frame draw failed at {}x{}: {e}; skipping frame", size.width, size.height);
        false
      }
    }
  }

  /// Shader-active frame: rasterize the display list - flipped, so the layer
  /// reads top-left origin like every sampled texture - through the rig into
  /// the retained layer, then draw the window shader program over it straight
  /// into FBO 0 (no intermediate target, no closing blit). The program's
  /// vertex stage is what flips back to window orientation. `spec.previous`
  /// retains last frame's resolve as a second layer bound as `uPrevious`.
  fn draw_to_window_shaded(&mut self, dl: &DisplayList, size: ISize) -> Result<(), String> {
    let (width, height) = (size.width as u32, size.height as u32);
    let state = self.window_shader.as_mut().expect("shaded draw requires a declared window shader");

    // The clean-tree fast path: the submit declared the display list
    // unchanged (see Context::submit_clean), nothing content-bearing arrived
    // since the last resolve, and the retained layer matches the window - so
    // the layer already holds this frame's pixels and only the pass needs to
    // run. History frames never skip: uPrevious must track the last frame,
    // and a skipped resolve would freeze it on stale content.
    let skip_resolve = !self.content_dirty
      && !state.spec.previous
      && state.layer.as_ref().is_some_and(|l| l.width == width && l.height == height);
    if skip_resolve {
      self.pass_only_frames += 1;
    } else {
      if state.spec.previous {
        // Rotate the history before resolving: the current layer becomes
        // uPrevious and last frame's history buffer is resolved over. On the
        // first shaded frame the fresh history layer samples opaque black (its
        // creation clear).
        std::mem::swap(&mut state.layer, &mut state.prev_layer);
        if state.prev_layer.is_none() {
          let (tex, fbo) = crate::gl::create_layer_target(&self.gl, width, height, [0.0, 0.0, 0.0, 1.0])?;
          state.prev_layer = Some(LayerTarget { tex, fbo, width, height });
        }
      } else if let Some(old) = state.prev_layer.take() {
        unsafe {
          glow::HasContext::delete_framebuffer(&self.gl, old.fbo);
          glow::HasContext::delete_texture(&self.gl, old.tex);
        }
      }

      let flipped = flip_for_fbo(dl, height)?;

      // (Re)allocate the layer at the window's pixel size. A resize drops and
      // recreates it: that is resize-frequency churn, not the per-frame kind
      // the rig exists to avoid.
      if state.layer.as_ref().is_none_or(|l| l.width != width || l.height != height) {
        if let Some(old) = state.layer.take() {
          unsafe {
            glow::HasContext::delete_framebuffer(&self.gl, old.fbo);
            glow::HasContext::delete_texture(&self.gl, old.tex);
          }
        }
        let (tex, fbo) = crate::gl::create_layer_target(&self.gl, width, height, [0.0, 0.0, 0.0, 1.0])?;
        state.layer = Some(LayerTarget { tex, fbo, width, height });
      }
      let layer = state.layer.as_ref().expect("layer allocated above");

      gl::render_display_list_to_layer(
        &self.gl,
        &mut self.impeller_ctx,
        &mut self.offscreen_rig,
        &flipped,
        size,
        layer.fbo,
      )?;
      self.content_dirty = false;
    }
    let layer = state.layer.as_ref().expect("resolved or retained above");

    // The layer binds as uSource, the history layer (when declared and live)
    // as uPrevious - internal textures, no sampler object (their linear/clamp
    // object state stands; Impeller never draws them). Extra declared inputs
    // resolve through the registry by id with their declared sampling, a
    // missing id dropping to unbound (samples black), the same contract as
    // shader targets.
    let mut textures: Vec<PassInput> = vec![PassInput::d2("uSource", layer.tex, None)];
    if state.spec.previous {
      if let Some(prev) = &state.prev_layer {
        textures.push(PassInput::d2("uPrevious", prev.tex, None));
      }
    }
    for b in &state.spec.textures {
      match self.textures.get(&b.id) {
        Some(gpu) => textures.push(PassInput {
          name: b.name.clone(),
          texture: gpu.gl_texture,
          sampler: Some(self.samplers.get(gpu.sampler.overridden(&b.sampler))),
          shape: gpu.shape,
        }),
        None => log::warn!("[alloy] window shader input '{}': texture {} not found", b.name, b.id),
      }
    }
    crate::gl::render_program_to_window(
      &self.gl,
      &state.program,
      width,
      height,
      &state.spec.params,
      &textures,
      state.spec.vertex_count,
    );
    Ok(())
  }

  /// Apply a SetOverlay command: adopt the new declaration, keeping the
  /// rasterized layer's storage when the size is unchanged (the once-per-
  /// second figure refresh redraws into it), and mark it stale so the next
  /// frame re-rasterizes. None frees everything.
  pub(super) fn set_overlay(&mut self, overlay: Option<crate::context::Overlay>) {
    let mut layer = self.overlay.take().and_then(|old| old.layer);
    // Free the retained layer on clear, and on a size change (reallocated at
    // the next frame's rasterize); a same-size redeclaration redraws into it.
    let free_layer = match &overlay {
      None => layer.is_some(),
      Some(decl) => layer.as_ref().is_some_and(|l| l.width != decl.width || l.height != decl.height),
    };
    if free_layer {
      let old = layer.take().expect("checked above");
      unsafe {
        glow::HasContext::delete_framebuffer(&self.gl, old.fbo);
        glow::HasContext::delete_texture(&self.gl, old.tex);
      }
    }
    if let Some(decl) = overlay {
      self.overlay = Some(OverlayState { decl, layer, stale: true });
    }
  }

  /// Composite the installed overlay over the finished frame in FBO 0.
  /// A stale declaration is rasterized into the retained layer first - drawn
  /// UNFLIPPED on purpose: the wrapped layer FBO is a bottom-up window
  /// target, so its rows come out in FBO 0's own convention and the plain
  /// copy draw (vUV = p, see the fullscreen vertex stage) lands the overlay
  /// upright with no flip anywhere. The composite is a premultiplied-alpha
  /// blended draw at the declared window rectangle (converted here to GL's
  /// bottom-up viewport origin). Failures warn and skip: a lost overlay
  /// frame must never cost the app frame.
  pub(super) fn draw_overlay(&mut self, window: ISize) {
    let Some(ov) = &mut self.overlay else { return };
    let (width, height) = (ov.decl.width, ov.decl.height);
    if width == 0 || height == 0 {
      return;
    }
    if ov.stale || ov.layer.is_none() {
      if ov.layer.is_none() {
        match crate::gl::create_layer_target(&self.gl, width, height, [0.0, 0.0, 0.0, 0.0]) {
          Ok((tex, fbo)) => ov.layer = Some(LayerTarget { tex, fbo, width, height }),
          Err(e) => {
            log::warn!("[alloy] overlay layer: {e}");
            return;
          }
        }
      }
      let layer = ov.layer.as_ref().expect("layer ensured above");
      let size = ISize::new(width as i64, height as i64);
      if let Err(e) = gl::render_display_list_to_layer(
        &self.gl,
        &mut self.impeller_ctx,
        &mut self.offscreen_rig,
        &ov.decl.dl,
        size,
        layer.fbo,
      ) {
        log::warn!("[alloy] overlay rasterize failed: {e}");
        return;
      }
      ov.stale = false;
    }
    let program = match ensure_copy_program(&self.gl, &mut self.copy_program) {
      Ok(program) => program,
      Err(e) => {
        log::warn!("[alloy] overlay copy program: {e}");
        return;
      }
    };
    let layer = ov.layer.as_ref().expect("rasterized above");
    let origin = (ov.decl.x, window.height as i32 - (ov.decl.y + height as i32));
    let input = PassInput::d2("uSrc", layer.tex, None);
    crate::gl::composite_program_over_window(&self.gl, &program, origin, width, height, &[input]);
  }

  /// Apply a SetWindowShader command. A redeclaration with the same program
  /// keeps the retained layer and just adopts the new params/textures/vertex
  /// count (the per-frame params path); a different program releases the old
  /// state and starts fresh. None clears everything.
  pub(super) fn set_window_shader(&mut self, shader: Option<WindowShader>) {
    let Some(spec) = shader else {
      self.clear_window_shader();
      return;
    };
    if let Some(state) = &mut self.window_shader {
      if state.spec.program == spec.program {
        state.spec = spec;
        return;
      }
    }
    let Some(program) = self.programs.get(&spec.program) else {
      // The UI side validated against its mirror; a miss here means the
      // mirrors diverged. Keep whatever was active rather than flashing the
      // unshaded frame.
      log::warn!("[alloy] window shader: program {} not found", spec.program);
      return;
    };
    let program = program.clone();
    self.clear_window_shader();
    self.window_shader = Some(WindowShaderState { spec, program, layer: None, prev_layer: None });
  }

  /// Free the window shader state: the layer's GL objects die here (they were
  /// never adopted or registered), the program only if nothing else holds it.
  fn clear_window_shader(&mut self) {
    if let Some(state) = self.window_shader.take() {
      for layer in [state.layer, state.prev_layer].into_iter().flatten() {
        unsafe {
          glow::HasContext::delete_framebuffer(&self.gl, layer.fbo);
          glow::HasContext::delete_texture(&self.gl, layer.tex);
        }
      }
      release_program(&self.gl, state.program);
    }
  }
}
