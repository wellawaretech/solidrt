use impellers::{ISize, Texture};
use std::collections::HashSet;
use std::rc::Rc;

use crate::gpu::{SamplerState, TextureBinding, TextureEntry, TextureFormat};
use crate::raster::RasterCmd;
use crate::yuv::{self, YuvLayout, YuvMatrix, YuvRange};

use super::content::bound_sources;
use super::Context;

// The composition behind one YUV texture id: TWO full plane sets, each plane
// as (uniform name, texture id, byte offset in a packed frame), plus the
// packed frame size for validation. The conversion shader samples the
// `front` set; update_yuv uploads into the other set and swaps. The double
// buffering exists for the raster thread: on a pipelined (tile-based) GPU
// the previous frame's conversion pass may still be sampling its planes when
// the next upload lands, and writing a texture with reads in flight makes
// the driver stall or ghost it. Alternating sets keeps every upload
// hazard-free.
pub(super) struct YuvGroup {
  sets: [Vec<(&'static str, u64, usize)>; 2],
  front: usize,
  frame_size: usize,
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

  /// Create a sampleable texture from pixels (RGBA8, or single-channel R8)
  /// and adopt into Impeller, with the given sampling (how every consumer -
  /// shader passes and `<texture>` display - samples it) and an optional
  /// debug label. Returns the registry id assigned to the new texture; errs
  /// on a size over the device limit (named in the message), checked here so
  /// the mistake throws at the call site.
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
    self.gpu_limits().check_texture_size(width, height)?;
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
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format });
    if replace {
      self.note_content(id);
    }
    Ok(())
  }

  /// Re-upload pixels into an existing texture, sized by the id's format
  /// (width*height*4 for rgba8, width*height for r8). `pixels` may be a
  /// larger buffer holding multiple frames; `offset` selects the frame start.
  /// The frame must match the texture's dimensions exactly.
  pub fn update_texture(&self, id: u64, pixels: &[u8], offset: usize) -> Result<(), String> {
    if let Some(owner) = self.depth_owner(id) {
      return Err(format!("texture {id} is target {owner}'s depth texture: render-written, not uploadable"));
    }
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    let (width, height, format) = (entry.width(), entry.height(), entry.format);
    let frame_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
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
  /// (width*height*4 for rgba8, width*height for r8). Rejects render target
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
      return Err(format!("texture {id} is target {owner}'s depth texture; it resizes with the target (setTargetSize)"));
    }
    // Sampling and format are properties of the id and survive the id-stable
    // resize, as does the label (None here = keep, applied raster-side).
    let (sampler, format) = (entry.sampler(), entry.format);
    let frame_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
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
  /// anywhere a texture id is. Feed packed frames with `update_yuv`; the
  /// output re-renders at the next dirty flush like any shader target. Color constants are baked at creation (fixed per
  /// stream; a standard change means a new texture), `sampler` is the
  /// OUTPUT's sampling (planes always sample linear/clamp for chroma
  /// upscaling), and the content starts black until the first frame.
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
        self.yuv_groups.borrow_mut().insert(out, YuvGroup { sets, front: 0, frame_size });
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

  /// Upload one tightly packed frame (every plane, laid out per
  /// `yuv::planes`) into a YUV texture. Takes the frame BY VALUE: the buffer
  /// crosses to the raster thread as-is - no per-plane copies - and the
  /// planes slice it there at their fixed offsets. The upload lands in the
  /// back plane set and the conversion target rebinds to it (double
  /// buffering, see YuvGroup), so planes a still-in-flight conversion pass
  /// samples are never written under it. The conversion target re-renders
  /// and content damage propagates exactly as for `update_texture`.
  pub fn update_yuv(&self, id: u64, frame: Vec<u8>) -> Result<(), String> {
    let (planes, bindings) = {
      let mut groups = self.yuv_groups.borrow_mut();
      let group = groups.get_mut(&id).ok_or_else(|| format!("yuv texture {id} not found"))?;
      if frame.len() < group.frame_size {
        return Err(format!("need {} bytes for a packed frame, buffer has {}", group.frame_size, frame.len()));
      }
      let back = 1 - group.front;
      group.front = back;
      let set = &group.sets[back];
      let planes: Vec<(u64, usize)> = set.iter().map(|&(_, plane, offset)| (plane, offset)).collect();
      let bindings: Vec<TextureBinding> =
        set.iter().map(|&(name, plane, _)| TextureBinding::new(name, plane)).collect();
      (planes, bindings)
    };
    for &(plane, _) in &planes {
      self.note_content(plane);
    }
    self.send(RasterCmd::UpdateYuv { planes, frame });
    // Rebinding through the ordinary path keeps the sampler-graph mirror
    // honest and re-renders the conversion output at the next dirty flush;
    // channel order puts the rebind after the upload.
    self.set_target_textures(id, &bindings)
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
    self.gpu_limits().check_texture_size(width, height)?;
    let handles = self.rpc(|reply| RasterCmd::ResizeShaderTexture { id, width, height, reply })??;
    let sampler = self.textures.get(id).map(|e| e.sampler()).unwrap_or_default();
    self.textures.insert(id, TextureEntry { impeller: handles.color, width, height, sampler, format: TextureFormat::Rgba8 });
    // A depth texture is re-registered at its own stable id with the fresh
    // name the resize allocated (the color rule, applied to depth).
    if let (Some(depth_id), Some(impeller)) = (self.depth_of(id), handles.depth) {
      self.textures.insert(
        depth_id,
        TextureEntry { impeller, width, height, sampler: SamplerState::DEPTH, format: TextureFormat::Depth24 },
      );
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
      return Err(format!("texture {src} is target {owner}'s depth texture: sampler-only, sample it from a pass instead"));
    }
    let src_entry = self.textures.get(src).ok_or_else(|| format!("texture {src} not found"))?;
    let dst_entry = self.textures.get(dst).ok_or_else(|| format!("texture {dst} not found"))?;
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

  /// Point a borrowed id at `texture` (a snapshot boundary's rasterization,
  /// Impeller-owned, `width` x `height` pixels): registry entry for UI-side
  /// consumers, raster-side mirror for shader passes, and a content change
  /// so everything sampling the id re-renders. Called after every
  /// rasterization of the boundary, whether the backing was reused or
  /// reallocated - the id is the stable handle across both.
  pub fn publish_snapshot_texture(&self, id: u64, texture: &Texture, width: u32, height: u32) {
    self.textures.insert(
      id,
      TextureEntry {
        impeller: texture.clone(),
        width,
        height,
        sampler: SamplerState::default(),
        format: TextureFormat::Rgba8,
      },
    );
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
    // A YUV output takes its planes with it. They are never referenced by
    // the render tree, so they reclaim at the next sweep; the group is
    // removed now, so a late update_yuv errs instead of dirtying a target
    // whose planes are going away.
    if let Some(group) = self.yuv_groups.borrow_mut().remove(&id) {
      for (_, plane, _) in group.sets.into_iter().flatten() {
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
        self.targets.borrow_mut().remove(&id);
        self.shader_sources.borrow_mut().remove(&id);
        self.manual_targets.borrow_mut().remove(&id);
        self.send(RasterCmd::DestroyTexture { id });
        false
      });
      if pending.len() == before {
        break;
      }
    }
  }
}
