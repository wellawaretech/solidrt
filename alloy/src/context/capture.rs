use impellers::{DisplayList, Texture};

use crate::gpu::{validate_params, validate_texture_bindings, NodeShader};
use crate::raster::RasterCmd;

use super::Context;

/// The successful outcome of a node capture: the RGBA8 pixels the node's
/// subtree was rasterized into (tightly packed top-to-bottom rows, same
/// layout as `Context::read_texture`) and their device-pixel dimensions.
/// Nothing is registered; there is nothing for the caller to free.
pub struct CaptureInfo {
  pub pixels: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

/// A capture completion callback, invoked exactly once with the outcome after
/// the paint pass that serviced (or failed to service) the request. Runs on the
/// UI thread, out of the tree walk (see `deliver_captures`).
pub type CaptureDone = Box<dyn FnOnce(Result<CaptureInfo, String>)>;

impl Context {
  /// Queue a capture of `node_id`'s subtree, serviced on the next paint pass
  /// that visits the node. `done` is invoked once with the outcome after that
  /// pass. If the node is never visited (not in the live tree), the request is
  /// failed by `fail_unserviced_captures`.
  pub fn request_capture(&self, node_id: u64, done: CaptureDone) {
    self.capture_requests.borrow_mut().entry(node_id).or_default().push(done);
  }

  /// Whether any capture is queued. Checked per visited node on the paint hot
  /// path, so it stays a cheap borrow with no allocation.
  pub fn has_pending_captures(&self) -> bool {
    !self.capture_requests.borrow().is_empty()
  }

  /// Take (removing) the completion callbacks queued for `node_id`, called by
  /// the paint walk when it reaches the node.
  pub fn take_node_captures(&self, node_id: u64) -> Vec<CaptureDone> {
    self.capture_requests.borrow_mut().remove(&node_id).unwrap_or_default()
  }

  /// Record a serviced capture's outcome for delivery at the end of the paint
  /// pass (see `deliver_captures`), rather than invoking the callback mid-walk.
  pub fn complete_capture(&self, done: CaptureDone, result: Result<CaptureInfo, String>) {
    self.capture_ready.borrow_mut().push((done, result));
  }

  /// Fail every still-queued request: the paint walk finished without visiting
  /// their nodes, so they are not in the live tree. Called at end of paint.
  pub fn fail_unserviced_captures(&self) {
    let leftover = std::mem::take(&mut *self.capture_requests.borrow_mut());
    for done in leftover.into_values().flatten() {
      self.complete_capture(done, Err("capture node is not in the live render tree".to_string()));
    }
  }

  /// Invoke every serviced capture's completion callback with its outcome.
  /// Called once at the end of the paint pass, out of the tree walk, so a
  /// callback (which may read back or free textures) never re-enters the walk.
  pub fn deliver_captures(&self) {
    let ready = std::mem::take(&mut *self.capture_ready.borrow_mut());
    for (done, result) in ready {
      done(result);
    }
  }

  /// Rasterize a display list into a new GPU texture of the given pixel size,
  /// ready for sampling. The texture is owned by Impeller (and the caller's
  /// handle), not by the registry. `aa: false` renders single-sample (no
  /// coverage AA), for boundaries that opted out.
  pub fn render_display_list_to_texture(
    &self,
    dl: &DisplayList,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<Texture, String> {
    self.rpc(|reply| RasterCmd::RasterizeDl { dl: dl.clone(), width, height, aa, reply })?
  }

  /// Re-rasterize a display list into an existing texture from
  /// `render_display_list_to_texture`, reusing its storage (invalidated
  /// snapshot boundaries re-render this way instead of reallocating).
  /// Storage is exact-size, so only reuse at the same `width` x `height`.
  pub fn render_display_list_into_texture(
    &self,
    dl: &DisplayList,
    texture: &Texture,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<(), String> {
    self.rpc(|reply| RasterCmd::RasterizeDlInto {
      dl: dl.clone(),
      texture: texture.clone(),
      width,
      height,
      aa,
      reply,
    })?
  }

  /// Rasterize a shaded snapshot boundary's display list and run its node
  /// shader pass in one trip: the subtree renders into the source texture,
  /// then `shader.program` draws one fullscreen pass over it into the
  /// output, which the boundary composites in place of the raw snapshot.
  /// With `shader.previous`, `history` binds as `uPrevious` (created
  /// transparent when None) - the caller owns rotating source and history
  /// roles across calls. Pass Some(handles) to re-render in place; they must
  /// have been created by this method at the same `width` x `height` (only
  /// an exact dimension match reuses). Validates like `set_window_shader`:
  /// known program, params/textures naming active uniforms only (`uSource`,
  /// `uPrevious` and `iResolution` are runtime-filled).
  pub fn rasterize_shaded(
    &self,
    dl: &DisplayList,
    width: u32,
    height: u32,
    aa: bool,
    shader: &NodeShader,
    source: Option<&Texture>,
    output: Option<&Texture>,
    history: Option<&Texture>,
  ) -> Result<(Texture, Texture, Option<Texture>), String> {
    self.validate_node_shader(shader)?;
    self.rpc(|reply| RasterCmd::RasterizeDlShaded {
      dl: dl.clone(),
      width,
      height,
      aa,
      shader: shader.clone(),
      source: source.cloned(),
      output: output.cloned(),
      history: history.cloned(),
      reply,
    })?
  }

  /// Re-run a node shader pass over an existing source/output pair from
  /// `rasterize_shaded` (plus the history binding while `previous` is
  /// declared): the declaration changed (the params path) while the
  /// boundary's content stayed valid. Fire-and-forget on the ordered raster
  /// channel, so the refreshed pixels land ahead of the frame that
  /// composites them; the caller owns requesting that frame.
  pub fn rerun_node_shader(
    &self,
    shader: &NodeShader,
    source: &Texture,
    output: &Texture,
    history: Option<&Texture>,
    width: u32,
    height: u32,
  ) -> Result<(), String> {
    self.validate_node_shader(shader)?;
    self.send(RasterCmd::RerunNodeShader {
      shader: shader.clone(),
      source: source.clone(),
      output: output.clone(),
      history: history.cloned(),
      width,
      height,
    });
    Ok(())
  }

  // Call-site validation for a node shader declaration, against the UI-side
  // mirrors (the same checks as the window shader): unit budget including
  // the runtime-filled uSource (and uPrevious while declared), a known
  // program, and params/textures naming its active uniforms only.
  fn validate_node_shader(&self, shader: &NodeShader) -> Result<(), String> {
    self.gpu_limits().check_texture_units(1 + usize::from(shader.previous) + shader.textures.len())?;
    let programs = self.program_uniforms.borrow();
    let uniforms = programs.get(&shader.program).ok_or_else(|| format!("program {} not found", shader.program))?;
    validate_params(uniforms, &shader.params)?;
    validate_texture_bindings(uniforms, &shader.textures)?;
    Ok(())
  }

  /// Rasterize a display list and read back exactly `width` x `height` RGBA8
  /// pixels (tightly packed top-to-bottom rows, same layout as
  /// `read_texture`). The raster thread rasterizes and reads back in one
  /// trip; the render target never leaves it and no texture is registered. A
  /// caller that wants a texture composes with `create_texture_from_pixels`.
  pub fn capture_node_pixels(&self, dl: &DisplayList, width: u32, height: u32) -> Result<Vec<u8>, String> {
    self.rpc(|reply| RasterCmd::RasterizeReadback { dl: dl.clone(), width, height, reply })?
  }

  /// Read back a registered texture's RGBA8 pixels by id, using the entry's
  /// own dimensions. Errors if the id is not in the registry.
  pub fn read_texture_by_id(&self, id: u64) -> Result<(u32, u32, Vec<u8>), String> {
    if let Some(owner) = self.depth_owner(id) {
      return Err(format!(
        "texture {id} is target {owner}'s depth texture: sampler-only, render it through a pass to read it"
      ));
    }
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    if entry.format.is_float() {
      return Err(format!(
        "texture {id} is {}: float textures are upload-and-sample only (not color-renderable in core GLES 3.0, so no readback path exists)",
        entry.format.name()
      ));
    }
    let (width, height) = (entry.width(), entry.height());
    let pixels = self.read_texture(&entry.impeller, width, height)?;
    Ok((width, height, pixels))
  }

  /// Read back a texture's RGBA8 pixels (tightly packed top-to-bottom rows).
  pub fn read_texture(&self, texture: &Texture, width: u32, height: u32) -> Result<Vec<u8>, String> {
    self.rpc(|reply| RasterCmd::ReadTexture { texture: texture.clone(), width, height, reply })?
  }
}
