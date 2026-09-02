use crate::impellers::{DisplayList, DisplayListBuilder};
use crate::rendertree::composite::{self, PaintStats};
use crate::rendertree::{PlatformContext, RenderTree};

// The last built display list together with the inputs it was built from.
// While all of these are unchanged, a requested frame is present-only (texture
// content changed, e.g. a camera upload): re-rendering the same display list
// samples the new texture contents, so the build can be skipped.
struct DlCache {
  dl: DisplayList,
  revision: u64,
  textures_generation: u64,
  window: (f32, f32),
  scale: f32,
}

/// The retained frame protocol over the composite phases: demand gating,
/// GPU-content damage application, present-only reuse of the last built
/// display list, and the interlocks that keep captures, deferred destroys and
/// window-shader writes serviced on every path. One driver per frame
/// producer; it owns nothing but the retained list, so drivers over the same
/// tree stay correct independently.
///
/// Control is inverted: the driver never calls back into the embedder. The
/// caller sequences `begin` -> `commit` -> (`layout` -> `paint` -> `finish`)
/// and runs its own work between the calls - a post-layout hook after
/// `layout`, hover refresh after `paint` - with tree borrows scoped per call.
/// `composite::render` remains the driverless one-shot alternative.
pub struct FrameDriver {
  cache: Option<DlCache>,
}

impl FrameDriver {
  pub fn new() -> Self {
    Self { cache: None }
  }

  /// The demand gate. Consumes the one-shot frame request; `extra_demand` is
  /// the caller's own reason to draw (an overlay refresh due, or a direct
  /// draw call, where the call itself is the demand). `None` means nothing
  /// wanted a frame and nothing was drawn. On `Some`, push any pre-frame
  /// raster state (e.g. a stats overlay) before `commit`: the ordered raster
  /// channel then applies it to exactly this frame. Playback
  /// (`always_render`) never gates.
  pub fn begin(&mut self, platform: &PlatformContext, extra_demand: bool) -> Option<PendingFrame<'_>> {
    let requested = platform.take_frame_requested();
    if !requested && !extra_demand && !platform.always_render() {
      return None;
    }
    Some(PendingFrame { driver: self })
  }
}

impl Default for FrameDriver {
  fn default() -> Self {
    Self::new()
  }
}

/// A frame past the demand gate, not yet resolved into reuse or rebuild.
pub struct PendingFrame<'d> {
  driver: &'d mut FrameDriver,
}

impl<'d> PendingFrame<'d> {
  /// Resolves the frame. First applies GPU content damage: writes since the
  /// last frame (target re-renders, uploads, camera frames) change pixels
  /// behind unchanged texture ids and leave no tree damage of their own;
  /// baked snapshot boundaries are the one consumer that would go stale.
  /// Applied before the clean check because it bumps the revision exactly
  /// when such a boundary is hit - pure-GPU frames with no snapshot consumer
  /// keep the reuse path.
  ///
  /// Then, when nothing that feeds the display list changed, resubmits the
  /// retained list instead of rebuilding (`Reused`): layout and paint are
  /// skipped entirely. Bypassed in playback mode to keep captures identical
  /// to a rebuild, and when captures are pending: they are serviced by the
  /// paint walk, which reuse skips, so reusing would strand them. Otherwise
  /// hands back the build handle. `Err` means the render thread is gone.
  pub fn commit(
    self,
    tree: &mut RenderTree,
    platform: &PlatformContext,
    alloy: &crate::Context,
  ) -> Result<Commit<'d>, ()> {
    let content = alloy.take_content_changes();
    if !content.is_empty() {
      tree.texture_content_changed(&content);
    }

    if !platform.always_render() && !alloy.has_pending_captures() {
      if let Some(c) = self.driver.cache.as_ref() {
        if c.revision == tree.revision()
          && c.textures_generation == alloy.textures.generation()
          && c.window == platform.window_size()
          && c.scale == platform.display_scale()
        {
          // A window-shader prop write lands here (Damage::Present bumps no
          // revision): flush it ahead of the frame, the ordering the build
          // path gets from the paint walk.
          if let Some(change) = tree.take_pending_window_shader() {
            if let Err(e) = alloy.set_window_shader(change) {
              log::warn!("[render] window shader: {e}");
            }
          }
          // Clean resubmit: the raster side may run only the shader pass
          // over its retained layer (see Context::submit_clean). The damage
          // is resolved without a walk - only GPU-content changes land on
          // this path, and their nodes' extents are current.
          let (w, h) = platform.window_size();
          let damage = composite::resolve_reuse_damage(tree, crate::impellers::Size::new(w, h));
          alloy.submit_clean(c.dl.clone(), crate::PresentDamage::from_frame(damage, platform.display_scale()))?;
          // The reuse path skips paint_phase, whose end-of-frame sweep
          // reclaims deferred destroys - run it here too so a destroy with
          // no other tree change (its requested frame lands in this path)
          // is not stranded until the next rebuild. The cached list's Rc'd
          // Impeller handles keep its textures alive regardless.
          for id in tree.take_released_snapshot_textures() {
            alloy.release_borrowed(id);
          }
          if alloy.has_pending_destroys() {
            alloy.reclaim_destroyed(&tree.referenced_texture_ids());
          }
          return Ok(Commit::Reused { content_changed: !content.is_empty() });
        }
      }
    }

    let mut builder = DisplayListBuilder::new(None);
    let scale = platform.display_scale();
    builder.scale(scale, scale);
    Ok(Commit::Build(FrameBuilder { driver: self.driver, builder }))
  }
}

/// How `commit` resolved the frame.
pub enum Commit<'d> {
  /// The retained display list was resubmitted; the frame is done.
  /// `content_changed` says whether GPU writes since the last frame changed
  /// pixels behind it (a layer or shader app's every frame): the picture
  /// moved even though the tree did not.
  Reused { content_changed: bool },
  /// Something changed: sequence the builder to produce the frame.
  Build(FrameBuilder<'d>),
}

/// The rebuild half of a frame. `layout` and `paint` are the composite
/// phases; the caller runs its own hooks between them (paint re-runs layout
/// internally, so mutations made after `layout` are absorbed). `finish`
/// builds, retains and submits the list.
pub struct FrameBuilder<'d> {
  driver: &'d mut FrameDriver,
  builder: DisplayListBuilder,
}

impl FrameBuilder<'_> {
  pub fn layout(&mut self, tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) {
    composite::layout_phase(tree, platform, alloy);
  }

  pub fn paint(&mut self, tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> PaintStats {
    composite::paint_phase(&mut self.builder, tree, platform, alloy)
  }

  /// Builds and submits the display list, retaining it for present-only
  /// reuse. The cache key is sampled here, after the build: hooks run since
  /// `layout` may have mutated the tree, and a first build can itself create
  /// textures. `Err` means the render thread is gone.
  pub fn finish(self, tree: &RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> Result<(), ()> {
    let FrameBuilder { driver, mut builder } = self;
    if let Some(dl) = builder.build() {
      driver.cache = Some(DlCache {
        dl: dl.clone(),
        revision: tree.revision(),
        textures_generation: alloy.textures.generation(),
        window: platform.window_size(),
        scale: platform.display_scale(),
      });
      alloy.submit(dl, crate::PresentDamage::from_frame(tree.frame_damage(), platform.display_scale()))?;
    }
    Ok(())
  }
}
