use impellers::{ISize, Texture};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::gpu::{check_cube_faces, SamplerState, TextureBinding, TextureEntry, TextureFormat, TextureShape};
use crate::raster::RasterCmd;
use crate::yuv::{self, LatchedFrame, YuvLatchShared, YuvLayout, YuvMatrix, YuvRange};

use super::content::bound_sources;
use super::Context;

// The UI side of one YUV texture id: the latch its frames wait in (shared
// with the producer's sink and the raster thread, which owns the plane sets
// and the flip) and the plane ids, so destroy takes them down with the
// output. The raster thread keeps TWO full plane sets and alternates them
// per latched frame: on a pipelined (tile-based) GPU the previous frame's
// conversion pass may still be sampling its planes when the next upload
// lands, and writing a texture with reads in flight makes the driver stall
// or ghost it. The conversion target's real sampler binding therefore
// changes on the raster side; the UI-side sampler-graph mirror keeps set 0
// bound, which is harmless: both sets have identical edges into the output,
// and the mirror exists for the content closure and the cycle check.
pub(super) struct YuvGroup {
  planes: Vec<u64>,
  latch: Arc<YuvLatchShared>,
  frame_size: usize,
}

/// Where a producer's packed YUV frames go: the latch of one YUV texture,
/// usable from any thread (a video player's worker). Each push carries the
/// time the frame is due on `crate::clock`; the raster thread shows the
/// newest due frame at each frame's presentation deadline. A push latches
/// the platform's frame request and wakes the main loop, so the frame gets
/// built; while `set_playing(true)`, the texture holds standing frame demand
/// (`Context::streaming_textures`). Against a stepped clock (playback) a
/// push against a full latch blocks until the capture takes.
pub struct YuvFrameSink {
  latch: Arc<YuvLatchShared>,
  frame_size: usize,
  frame_request: Arc<AtomicBool>,
  wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl YuvFrameSink {
  /// Queue one tightly packed frame (every plane, laid out per
  /// `yuv::planes`) due at `due_ns`; zero for a producer with no clock of
  /// its own, which latches at the next frame. `pts_us` is what
  /// `shown_pts_us` reports once the frame is on screen. Errs on a frame of
  /// the wrong size.
  pub fn push(&self, data: Vec<u8>, pts_us: i64, due_ns: i64) -> Result<(), String> {
    if data.len() < self.frame_size {
      return Err(format!("need {} bytes for a packed frame, buffer has {}", self.frame_size, data.len()));
    }
    self.latch.push(LatchedFrame { due_ns, pts_us, data }, crate::clock::stepped());
    self.frame_request.store(true, Ordering::Relaxed);
    if let Some(wake) = &self.wake {
      wake();
    }
    Ok(())
  }

  /// The pts of the frame the raster thread last showed.
  pub fn shown_pts_us(&self) -> Option<i64> {
    self.latch.shown_pts_us()
  }

  /// Playback started or stopped: the texture holds standing frame demand
  /// while playing.
  pub fn set_playing(&self, playing: bool) {
    self.latch.set_playing(playing);
    if playing {
      self.frame_request.store(true, Ordering::Relaxed);
      if let Some(wake) = &self.wake {
        wake();
      }
    }
  }

  /// The display's refresh period on the clock, None under a stepped clock.
  pub fn period_ns(&self) -> Option<i64> {
    yuv::period_ns()
  }

  /// Nothing more will be pushed until a seek: a stepped consumer stops
  /// waiting for more.
  pub fn end(&self) {
    self.latch.end();
  }

  /// Forget every queued frame (a seek).
  pub fn flush(&self) {
    self.latch.flush();
  }
}

impl Context {
  pub fn get_or_create_texture(
    &self,
    id: u64,
    size: ISize,
    make_pixels: impl FnOnce() -> Vec<u8>,
  ) -> Result<Rc<TextureEntry>, String> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      self.create_texture_at(
        id,
        size.width as u32,
        size.height as u32,
        &pixels,
        SamplerState::default(),
        TextureFormat::Rgba8,
        None,
      )?;
    }
    Ok(self.textures.get(id).expect("texture must exist after insert"))
  }

  pub fn get_or_update_texture(
    &self,
    id: u64,
    size: ISize,
    make_pixels: impl FnOnce() -> Vec<u8>,
  ) -> Result<Rc<TextureEntry>, String> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      self.create_texture_at(
        id,
        size.width as u32,
        size.height as u32,
        &pixels,
        SamplerState::default(),
        TextureFormat::Rgba8,
        None,
      )?;
    } else if let Err(e) = self.update_texture(id, &pixels, 0) {
      log::warn!("[alloy] texture {id} update failed: {e}");
    }
    Ok(self.textures.get(id).expect("texture must exist after insert or update"))
  }

  /// Create a sampleable texture from pixels (see `TextureFormat` for the
  /// vocabulary) and adopt into Impeller, with the given sampling (how every
  /// consumer - shader passes and `<texture>` display - samples it) and an
  /// optional debug label. Returns the registry id assigned to the new
  /// texture; errs on a size over the device limit (named in the message),
  /// checked here so the mistake throws at the call site. The caller
  /// resolves sampling against the format (`SamplerState::parse_for`): float
  /// formats are nearest-only data textures for shader sampling; displaying
  /// one via `<texture src>` is out of contract.
  pub fn create_texture_from_pixels(
    &self,
    width: u32,
    height: u32,
    pixels: &[u8],
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<u64, String> {
    let id = self.textures.allocate_id();
    self.create_texture_at(id, width, height, pixels, sampler, format, label)?;
    Ok(id)
  }

  /// The declared pixel format of a registered texture id: create-time
  /// state, stable across id-stable resizes. The boundary layer reads it to
  /// type-check upload payloads against the format.
  pub fn texture_format(&self, id: u64) -> Result<TextureFormat, String> {
    self.textures.get(id).map(|entry| entry.format).ok_or_else(|| format!("texture {id} not found"))
  }

  /// Create (or replace) the texture stored at `id`, e.g. to resize a stream
  /// texture without invalidating the id handed out to consumers. Lookups pick
  /// up the new texture immediately; in-flight users of the old entry keep it
  /// alive until released. A `label` of None on a replace keeps the existing
  /// entry's label (the id-stable resize contract). Errs on a size over the
  /// device limit (checked here, before the RPC) or a failed adoption.
  pub fn create_texture_at(
    &self,
    id: u64,
    width: u32,
    height: u32,
    pixels: &[u8],
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<(), String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(width, height)?;
    limits.check_mipmap(format, sampler.mipmap)?;
    // A create at a fresh id cannot be referenced by anything yet; a replace
    // at a live id (stream resize, camera format change) is a content change
    // behind that id like any other.
    let replace = self.textures.get(id).is_some();
    let impeller = self.rpc(|reply| RasterCmd::CreateTexture {
      id,
      width,
      height,
      pixels: pixels.to_vec(),
      sampler,
      format,
      label,
      reply,
    })??;
    self.textures.insert(id, TextureEntry::d2(impeller, width, height, sampler, format));
    if replace {
      self.note_content(id);
    }
    Ok(())
  }

  /// Create a cube map from six `size` x `size` faces in GL order (+X, -X,
  /// +Y, -Y, +Z, -Z), each `format.byte_len(size, size)` bytes - or from
  /// an explicit mip chain, the full chain level-major (see
  /// `check_cube_faces`), which is uploaded as given instead of generated
  /// and so needs no color-renderable format - with the given sampling
  /// (the caller resolves it against the format like every pixel create;
  /// `wrap` is irrelevant - GLES 3.0 filters across faces seamlessly; an
  /// explicit chain requires `mipmap`) and an optional label. Returns the
  /// registry id, an ordinary texture id that only a `samplerCube` binding
  /// consumes: the `<texture>` display, `readTexture`, `copyTexture`,
  /// uploads and resizes all reject it (see `TextureShape::Cube`). Errs on
  /// a face count or size mismatch, or a face edge over the device's cube
  /// map ceiling.
  pub fn create_cube_texture(
    &self,
    size: u32,
    faces: Vec<Vec<u8>>,
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<u64, String> {
    if size == 0 {
      return Err("cube map face size must be non-zero".to_string());
    }
    let limits = self.gpu_limits();
    limits.check_cube_map_size(size)?;
    let levels = check_cube_faces(size, &faces, format)?;
    // Only a GENERATED chain needs the format to be color-renderable; an
    // explicit one is plain uploads.
    limits.check_mipmap(format, sampler.mipmap && levels == 1)?;
    if levels > 1 && !sampler.mipmap {
      return Err("an explicit mip chain is sampled through mipmap: true".to_string());
    }
    let id = self.textures.allocate_id();
    self.rpc(|reply| RasterCmd::CreateCubeTexture { id, size, faces, sampler, format, label, reply })??;
    self.textures.insert(id, TextureEntry::cube(size, sampler, format));
    Ok(id)
  }

  /// The error a 2D-shaped verb returns for a cube map id, or None for
  /// any other id (unknown ids fall through to the caller's own lookup).
  pub(super) fn reject_cube(&self, id: u64, verb: &str) -> Result<(), String> {
    match self.textures.get(id) {
      Some(entry) if entry.shape == TextureShape::Cube => {
        Err(format!("texture {id} is a cube map: sampler-only (bind it to a samplerCube); {verb}"))
      }
      _ => Ok(()),
    }
  }

  /// Re-upload pixels into an existing texture, sized by the id's format
  /// (`TextureFormat::byte_len`). `pixels` may be a
  /// larger buffer holding multiple frames; `offset` selects the frame start.
  /// The frame must match the texture's dimensions exactly.
  pub fn update_texture(&self, id: u64, pixels: &[u8], offset: usize) -> Result<(), String> {
    if let Some(owner) = self.depth_owner(id) {
      return Err(format!("texture {id} is target {owner}'s depth texture: render-written, not uploadable"));
    }
    self.reject_cube(id, "a cube map is create-once, there is no upload into it")?;
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    let (width, height, format) = (entry.width(), entry.height(), entry.format);
    let frame_size = format.byte_len(width, height);
    let end = offset.checked_add(frame_size).ok_or_else(|| "offset overflow".to_string())?;
    if end > pixels.len() {
      return Err(format!(
        "need {frame_size} bytes at offset {offset} for {width}x{height} {}, buffer has {}",
        format.name(),
        pixels.len()
      ));
    }
    self.send(RasterCmd::UpdateTexture { id, pixels: pixels[offset..end].to_vec() });
    self.note_content(id);
    Ok(())
  }

  /// Replace a registered pixel texture with one of a new size at the same id
  /// (an id-stable resize): lookups and shader sampler bindings pick up the
  /// new texture immediately (shaders sampling it re-render), in-flight users
  /// of the old entry keep it alive until released. `pixels` seeds the new
  /// contents and must hold at least one frame at the id's format
  /// (`TextureFormat::byte_len`). Rejects render target
  /// ids - resize those with `resize_target`, which carries the compiled
  /// program and draw state along. The caller must request a frame.
  pub fn resize_texture(&self, id: u64, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    let Some(entry) = self.textures.get(id) else {
      return Err(format!("texture {id} not found"));
    };
    if self.targets.borrow().contains_key(&id) {
      return Err(format!("texture {id} is a render target; resize it with setTargetSize"));
    }
    if let Some(owner) = self.depth_owner(id) {
      return Err(format!(
        "texture {id} is target {owner}'s depth texture; it resizes with the target (setTargetSize)"
      ));
    }
    self.reject_cube(id, "a cube map is create-once, there is no resize")?;
    // Sampling and format are properties of the id and survive the id-stable
    // resize, as does the label (None here = keep, applied raster-side).
    let (sampler, format) = (entry.sampler(), entry.format);
    let frame_size = format.byte_len(width, height);
    if pixels.len() < frame_size {
      return Err(format!(
        "need {frame_size} bytes for {width}x{height} {}, buffer has {}",
        format.name(),
        pixels.len()
      ));
    }
    self.create_texture_at(id, width, height, &pixels[..frame_size], sampler, format, None)
  }

  /// Create a planar YUV texture (see yuv.rs): plane textures for `layout`
  /// (two double-buffered sets, see YuvGroup) plus a conversion shader
  /// target sampling them, whose RGBA output id is returned - usable
  /// anywhere a texture id is. Feed packed frames through the sink
  /// `yuv_frame_sink` hands out; the output re-renders at the frame that
  /// latches one like any shader target. Color constants are baked at
  /// creation (fixed per stream; a standard change means a new texture),
  /// `sampler` is the OUTPUT's sampling (planes always sample linear/clamp
  /// for chroma upscaling), and the content starts black until the first
  /// frame.
  /// Destroying the returned id takes the planes down with it. There is no
  /// id-stable resize: a size change is a new texture (stream dimension
  /// changes replace the player's texture anyway).
  pub fn create_yuv_texture(
    &self,
    width: u32,
    height: u32,
    layout: YuvLayout,
    matrix: YuvMatrix,
    range: YuvRange,
    sampler: SamplerState,
    label: Option<String>,
  ) -> Result<u64, String> {
    if width == 0 || height == 0 {
      return Err(format!("yuv texture size {width}x{height} must be non-zero"));
    }
    self.gpu_limits().check_texture_size(width, height)?;
    let planes = yuv::planes(layout, width, height);
    let frame_size: usize = planes.iter().map(|p| p.byte_len()).sum();
    // Seed planes with black (Y floor, chroma midpoint; NV12's interleaved
    // UV seeds both bytes 128) - zeroed chroma would start the output green.
    let y_black = if range == YuvRange::Limited { 16u8 } else { 0u8 };
    // Two full plane sets, double buffered (see YuvGroup); the shader starts
    // bound to set 0.
    let mut sets: [Vec<(&'static str, u64, usize)>; 2] = [Vec::new(), Vec::new()];
    let mut failure: Option<String> = None;
    'create: for (set, ids) in sets.iter_mut().enumerate() {
      for plane in &planes {
        let value = if plane.name == "uY" { y_black } else { 128u8 };
        let plane_label = label.as_ref().map(|l| format!("{l}.{}{set}", plane.name[1..].to_lowercase()));
        match self.create_texture_from_pixels(
          plane.width,
          plane.height,
          &vec![value; plane.byte_len()],
          SamplerState::default(),
          plane.format,
          plane_label,
        ) {
          Ok(id) => ids.push((plane.name, id, plane.offset)),
          Err(e) => {
            failure = Some(e);
            break 'create;
          }
        }
      }
    }
    let result = match failure {
      Some(e) => Err(e),
      None => {
        let bindings: Vec<TextureBinding> =
          sets[0].iter().map(|&(name, id, _)| TextureBinding::new(name, id)).collect();
        self.create_shader_texture(
          width,
          height,
          &yuv::fragment_src(layout, matrix, range),
          &[],
          &bindings,
          sampler,
          label,
        )
      }
    };
    match result {
      Ok(out) => {
        // The raster side owns the sets and the flip from here; the ordered
        // channel puts the attach after the planes' and the target's
        // creation.
        let latch = Arc::new(YuvLatchShared::new());
        let planes: Vec<u64> = sets.iter().flatten().map(|&(_, id, _)| id).collect();
        self.send(RasterCmd::AttachYuvLatch { id: out, latch: latch.clone(), sets, frame_size });
        self.yuv_groups.borrow_mut().insert(out, YuvGroup { planes, latch, frame_size });
        Ok(out)
      }
      Err(e) => {
        for (_, id, _) in sets.into_iter().flatten() {
          self.destroy_texture(id);
        }
        Err(e)
      }
    }
  }

  /// A sink for pushing frames into the YUV texture `id` from any thread
  /// (see `YuvFrameSink`). `frame_request` is the platform's frame-request
  /// latch (`PlatformContext::frame_request_handle`), which a push sets so
  /// the frame gets built.
  pub fn yuv_frame_sink(&self, id: u64, frame_request: Arc<AtomicBool>) -> Result<YuvFrameSink, String> {
    let groups = self.yuv_groups.borrow();
    let group = groups.get(&id).ok_or_else(|| format!("yuv texture {id} not found"))?;
    Ok(YuvFrameSink {
      latch: group.latch.clone(),
      frame_size: group.frame_size,
      frame_request,
      wake: self.frame_wake.clone(),
    })
  }

  /// The draw gate's peek, before the frame builds: for every YUV texture,
  /// whether the raster thread will latch a frame for the frame presenting
  /// at `present_at` (the same deadline and lookahead the take uses), noted
  /// as content on the output id so a texture node showing it is damaged
  /// and a cached boundary over it re-rasters. Returns whether any will.
  pub fn note_due_video(&self, present_at: Instant) -> bool {
    let deadline_ns = crate::clock::ns(present_at);
    let lookahead_ns = yuv::lookahead_ns();
    let due: Vec<u64> = self
      .yuv_groups
      .borrow()
      .iter()
      .filter(|(_, group)| group.latch.peek(deadline_ns, lookahead_ns))
      .map(|(id, _)| *id)
      .collect();
    for id in &due {
      self.note_content(*id);
    }
    !due.is_empty()
  }

  /// Whether any YUV texture's producer is playing: standing frame demand,
  /// so the loop ticks on the refresh grid while video streams.
  pub fn streaming_textures(&self) -> bool {
    self.yuv_groups.borrow().values().any(|group| group.latch.playing())
  }

  /// Recreate a render target of any kind at a new size under the same id:
  /// the compiled programs, sampler bindings, last-applied params, and draw
  /// state carry over, and the output re-renders at the new size at the next
  /// dirty flush. Lookups pick up the new target right away; in-flight users
  /// of the old one keep it alive until released. The caller must request a
  /// frame.
  pub fn resize_target(&self, id: u64, width: u32, height: u32) -> Result<(), String> {
    if !self.targets.borrow().contains_key(&id) {
      return Err(format!("target {id} not found"));
    }
    if self.textures.get(id).is_some_and(|e| e.shape == TextureShape::Cube) {
      return Err(format!("target {id} is a cube draw target: create-once, it does not resize"));
    }
    // A sub-target's size is its rectangle's; the origin stays.
    let tile = self.sub_targets.borrow().get(&id).map(|t| (t.x, t.y));
    if let Some((x, y)) = tile {
      return self.set_target_rect(id, x, y, width, height);
    }
    self.gpu_limits().check_texture_size(width, height)?;
    let handles = self.rpc(|reply| RasterCmd::ResizeShaderTexture { id, width, height, reply })??;
    let (sampler, format) =
      self.textures.get(id).map(|e| (e.sampler(), e.format)).unwrap_or((SamplerState::default(), TextureFormat::Rgba8));
    let entry = match handles.color {
      Some(color) => TextureEntry::d2(color, width, height, sampler, format),
      None => TextureEntry::d2_sampler_only(width, height, sampler, format),
    };
    self.textures.insert(id, entry);
    // A depth texture is re-registered at its own stable id with the fresh
    // name the resize allocated (the color rule, applied to depth).
    if let (Some(depth_id), Some(impeller)) = (self.depth_of(id), handles.depth) {
      self
        .textures
        .insert(depth_id, TextureEntry::d2(impeller, width, height, SamplerState::DEPTH, TextureFormat::Depth24));
    }
    // The storage is regenerated whatever the kind, manual included, so this
    // notes unconditionally (unlike the pure-mutation paths).
    self.note_content(id);
    Ok(())
  }

  /// Overwrite manual target `dst` with texture `src`'s current pixels: the
  /// GPU-side seed/history write, the copy analog of `update_texture`.
  /// Fire-and-forget on the ordered raster channel, so copies land in call
  /// order with renders and readbacks; the caller must request a frame for
  /// displayed output. Exact: sizes must match (an intentional tight
  /// contract - a scaling copy is an ordinary pass). Errs on unknown ids, a
  /// non-manual destination (the flush owns those contents), a size
  /// mismatch, or src == dst.
  pub fn copy_texture(&self, src: u64, dst: u64) -> Result<(), String> {
    if let Some(owner) = self.depth_owner(src) {
      return Err(format!(
        "texture {src} is target {owner}'s depth texture: sampler-only, sample it from a pass instead"
      ));
    }
    self.reject_cube(src, "render it through a pass instead of copying")?;
    self.reject_cube(dst, "nothing renders into it")?;
    let src_entry = self.textures.get(src).ok_or_else(|| format!("texture {src} not found"))?;
    let dst_entry = self.textures.get(dst).ok_or_else(|| format!("texture {dst} not found"))?;
    if src_entry.format.sample_only() {
      return Err(format!(
        "texture {src} is {}: sample-only (a copy would quantize float to the target's rgba8, or decode sRGB)",
        src_entry.format.name()
      ));
    }
    if !self.manual_targets.borrow().contains(&dst) {
      return Err(format!("target {dst} is not manual (the runtime renders it; create with render: \"manual\")"));
    }
    if src == dst {
      return Err(format!("cannot copy texture {src} into itself"));
    }
    let (sw, sh) = (src_entry.width(), src_entry.height());
    let (dw, dh) = (dst_entry.width(), dst_entry.height());
    if (sw, sh) != (dw, dh) {
      return Err(format!("size mismatch: source is {sw}x{sh}, destination is {dw}x{dh}"));
    }
    self.send(RasterCmd::CopyTexture { src, dst });
    self.note_content(dst);
    Ok(())
  }

  /// Allocate an id for a texture the runtime owns (a snapshot boundary's
  /// rasterization, a camera stream). Valid to reference immediately: until
  /// the owner first publishes or creates at it the registry has no entry,
  /// so consumers see it as absent (a `<texture>` measures 0x0, a shader
  /// pass skips the binding).
  pub fn borrow_texture_id(&self) -> u64 {
    let id = self.textures.allocate_id();
    self.borrow_texture(id);
    id
  }

  /// Mark an existing id as runtime-owned: the app may read it but not
  /// destroy it (`destroy_texture` callers check `is_borrowed`); the owner
  /// releases it with `release_borrowed`.
  pub fn borrow_texture(&self, id: u64) {
    self.borrowed.borrow_mut().insert(id);
  }

  /// Whether `id` is a borrowed (runtime-owned) id the app may not destroy.
  pub fn is_borrowed(&self, id: u64) -> bool {
    self.borrowed.borrow().contains(&id)
  }

  /// The owner of a borrowed id is gone: the id leaves the borrowed set and
  /// takes the ordinary deferred-destroy path, so a still-mounted consumer
  /// keeps drawing the last pixels until it lets go.
  pub fn release_borrowed(&self, id: u64) {
    if self.borrowed.borrow_mut().remove(&id) {
      self.destroy_texture(id);
    }
  }

  /// Release every borrowed id at once: the app instance whose runtime-owned
  /// textures these were is gone (an engine reload), so nothing will release
  /// them one by one. The same deferred path as `release_borrowed`.
  pub fn release_all_borrowed(&self) {
    let ids: Vec<u64> = self.borrowed.borrow().iter().copied().collect();
    for id in ids {
      self.release_borrowed(id);
    }
  }

  /// Point a borrowed id at `texture` (a snapshot boundary's rasterization,
  /// Impeller-owned, `width` x `height` pixels): registry entry for UI-side
  /// consumers, raster-side mirror for shader passes, and a content change
  /// so everything sampling the id re-renders. Called after every
  /// rasterization of the boundary, whether the backing was reused or
  /// reallocated - the id is the stable handle across both.
  pub fn publish_snapshot_texture(&self, id: u64, texture: &Texture, width: u32, height: u32) {
    self
      .textures
      .insert(id, TextureEntry::d2(texture.clone(), width, height, SamplerState::default(), TextureFormat::Rgba8));
    self.send(RasterCmd::AdoptTexture { id, texture: texture.clone(), width, height });
    self.note_content(id);
  }

  /// Free a texture created via `create_texture_from_pixels`, `create_texture_at`,
  /// or `create_shader_texture`. Deferred, not immediate: the id is queued and
  /// actually reclaimed by `reclaim_destroyed` (run by the paint loop) once the
  /// live render tree no longer references it. Deferral makes the natural app
  /// pattern safe - destroy the old id in the same update that repoints
  /// `<texture src>` at its replacement - regardless of how the reactive flush
  /// interleaves with frames: any frame built before the swap lands still finds
  /// the entry and paints the old content instead of a blank. Until
  /// reclamation the id stays fully usable; afterwards the registry entry and
  /// raster-side resources (for shaders: GL program and FBO) are gone, while
  /// in-flight display lists keep the Impeller texture alive until they drop.
  pub fn destroy_texture(&self, id: u64) {
    // A depth id is owned by its target and reclaimed with it (gated
    // app-side too; backstop).
    if let Some(owner) = self.depth_owner(id) {
      log::warn!("[alloy] destroy of depth texture {id} ignored: it dies with target {owner}");
      return;
    }
    let mut pending = self.pending_destroys.borrow_mut();
    if !pending.contains(&id) {
      pending.push(id);
    }
    // A draw target takes its sub-targets with it: they render into its
    // storage and have no texture entry, so nothing else keeps them.
    for (tile, mirror) in self.sub_targets.borrow().iter() {
      if mirror.parent == id && !pending.contains(tile) {
        pending.push(*tile);
      }
    }
    // A YUV output takes its planes with it. They are never referenced by
    // the render tree, so they reclaim at the next sweep; the group is
    // removed now and its latch closed, so a producer still pushing hits a
    // no-op instead of dirtying a target whose planes are going away.
    if let Some(group) = self.yuv_groups.borrow_mut().remove(&id) {
      group.latch.close();
      for plane in group.planes {
        if !pending.contains(&plane) {
          pending.push(plane);
        }
      }
    }
  }

  /// Whether any destroy is awaiting reclamation, so the paint loop can skip
  /// the tree scan entirely in the common no-destroys case.
  pub fn has_pending_destroys(&self) -> bool {
    !self.pending_destroys.borrow().is_empty()
  }

  /// Reclaim every pending destroy whose id is not in `referenced` (the ids
  /// the live render tree currently references, see
  /// `RenderTree::referenced_texture_ids`) and not bound as a sampler source
  /// on a live target (the recorded binding edges). Still-referenced ids
  /// stay queued - and stay alive - until a later sweep finds them
  /// unreferenced, so a destroyed-but-still-mounted texture keeps drawing
  /// rather than glitching to blank. Called by the paint loop after each
  /// painted frame.
  pub fn reclaim_destroyed(&self, referenced: &HashSet<u64>) {
    let mut pending = self.pending_destroys.borrow_mut();
    // Reclaiming a target unbinds its sources, which may have been waiting
    // on exactly that; iterate until a pass reclaims nothing, so a target
    // and its sources go in one sweep (a sweep needs a frame, and an idle
    // app may not produce another).
    loop {
      let bound = bound_sources(&self.shader_sources.borrow());
      let before = pending.len();
      pending.retain(|&id| {
        // A displayed or bound depth texture keeps its owner alive: the
        // depth is the target's storage, not a texture of its own (bindings
        // already record the owner, see source_of).
        let depth = self.depth_of(id);
        if referenced.contains(&id) || bound.contains(&id) || depth.is_some_and(|d| referenced.contains(&d)) {
          return true;
        }
        if let Some(d) = depth {
          self.textures.remove(d);
          self.depth_ids.borrow_mut().remove(&d);
        }
        self.textures.remove(id);
        self.remove_target_records(id);
        self.send(RasterCmd::DestroyTexture { id });
        false
      });
      if pending.len() == before {
        break;
      }
    }
  }
}
