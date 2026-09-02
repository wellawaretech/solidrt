// Repaint-boundary caching and compositing: the retained paint results
// (PaintCache and its snapshot/shader halves), what a boundary caller hoists
// out of them (Hoist), and the composite paths that put a cached result back
// into the tree - draw_cached_recording for Recording boundaries,
// BoundaryComposite for snapshot boundaries. The walk itself
// (composite::build_recursive / record_node) stays in composite.rs and calls
// in here at each boundary node.

use crate::impellers::{
  ClipOperation, DisplayList, DisplayListBuilder, Matrix, Paint, Point, Rect, Size, Texture, TextureSampling,
};

use crate::rendertree::composite::{
  apply_clip, apply_scroll, effect_paint, emit_backdrop, own_matrix, record_node, service_captures_under_cache,
  view_filter, view_opacity, CLIP_INF,
};
use crate::rendertree::{BuildContext, Element, ElementKind, FilterState, RenderTree};

// What a boundary caller applies itself at composite time, and record_node
// therefore leaves out of the cached content. The record order is matrix,
// clip, scroll, fit, children; a hoist always covers a prefix of the first
// three (a hoisted scroll requires a hoisted clip, otherwise the
// composite-time scroll translate would move a recorded clip that must stay
// put in viewport space; a design-size fit is never hoisted - it is content).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hoist {
  /// Record everything (non-boundary nodes, non-View boundaries).
  None,
  /// The caller applies the View's matrix; clip and scroll stay recorded.
  /// Snapshot boundaries use this: their raster must bake clip and scroll,
  /// since the texture holds only the pixels visible at rasterize time.
  Transform,
  /// The caller applies matrix, clip and scroll; the cache holds the fit and
  /// children only. Recording boundaries use this, making the cache reusable
  /// under scroll writes as well as transform writes (see Damage::Scroll).
  Full,
}

/// A boundary's retained paint result, in node-local coordinates.
pub enum PaintCache {
  Recording(RecordingCache),
  Snapshot(SnapshotCache),
}

/// A Recording boundary's retained recording, plus what it knows about the
/// backdrop panels baked inside it. A replayed recording re-runs its baked
/// backdrop save_layers against the live window every frame, but on reuse
/// frames the walk does not enter the subtree, so the panels cannot push
/// their own damage-widening regions; the boundary pushes one conservative
/// region in their place (build_recursive's cached leg). A snapshot cache
/// needs no counterpart: its baked backdrop sampled the boundary's
/// offscreen at raster time, so window damage never changes its pixels.
pub struct RecordingCache {
  pub dl: DisplayList,
  pub backdrops: BakedBackdrops,
}

/// The backdrop panels inside a Recording cache, summarized from the
/// regions its record walk pushed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BakedBackdrops {
  /// No panels inside; reuse frames push nothing.
  None,
  /// Panels inside, with the largest blur reach among them. Reuse frames
  /// push the boundary's current window-space subtree extent (which
  /// contains every panel) carrying this reach, so damage within reach of
  /// a panel widens to cover the boundary - conservative, never stale.
  Reach(f32),
  /// Some panel's region was unmappable at record time (a non-2D
  /// transform inside the recording); reuse frames push the same
  /// unmappable marker, degrading the resolve to full damage exactly like
  /// the record did.
  Unmappable,
}

impl BakedBackdrops {
  /// Summarize the region entries a boundary's record walk pushed.
  pub(super) fn summarize(regions: &[crate::rendertree::BackdropRegion]) -> Self {
    if regions.is_empty() {
      return BakedBackdrops::None;
    }
    let mut reach = 0.0f32;
    for entry in regions {
      match entry {
        None => return BakedBackdrops::Unmappable,
        Some((_, r)) => reach = reach.max(*r),
      }
    }
    BakedBackdrops::Reach(reach)
  }
}

/// A snapshot boundary's retained rasterization. It remembers the logical
/// size and display scale it was rasterized at: pixels are
/// resolution-dependent, so a mismatch forces re-rasterization even when
/// nothing inside the subtree changed. Invalidation marks it stale
/// (`valid: false`) instead of dropping it: the pixels are worthless but the
/// texture allocation is still exactly the right size, so the next raster
/// re-renders into it instead of reallocating (see boundary::snapshot_node).
/// All storage is exact-size; with an unchanged canvas the allocation is
/// reusable across shader declaration changes in either direction.
pub struct SnapshotCache {
  pub texture: Texture,
  pub width: f32,
  pub height: f32,
  pub scale: f32,
  pub valid: bool,
  /// The shader half, present while a boundary shader is declared (see
  /// `View::set_shader`); its output is composited in place of `texture`.
  pub shaded: Option<ShadedCache>,
}

/// The boundary shader's cache: the pass output composited in place of the
/// raw snapshot, the outset the canvas was rasterized with (it joins the
/// validity key - a different outset means different storage), and, with
/// `previous` declared, the prior rasterization retained as the pass's
/// `uPrevious` input.
pub struct ShadedCache {
  pub output: Texture,
  pub outset: f32,
  pub history: Option<Texture>,
}

/// The identity of a snapshot boundary's storage: the logical box, the
/// display scale the pixels are rasterized at, and the shader canvas outset
/// (zero without a shader; a plain texture counts as outset zero). All
/// storage is exact-size, so every form of reuse - compositing a cached
/// texture, re-rendering into retained storage - requires the whole key to
/// match.
struct SnapshotKey {
  width: f32,
  height: f32,
  scale: f32,
  outset: f32,
}

impl SnapshotKey {
  /// The rasterization canvas in logical px: the box grown by the outset on
  /// every side.
  fn canvas(&self) -> Size {
    Size::new(self.width + 2.0 * self.outset, self.height + 2.0 * self.outset)
  }

  /// The exact-size texture dimensions for the canvas at this scale.
  fn texture_dims(&self) -> (u32, u32) {
    let canvas = self.canvas();
    ((canvas.width * self.scale).ceil() as u32, (canvas.height * self.scale).ceil() as u32)
  }

  /// Whether retained storage was allocated for exactly this key.
  fn matches(&self, snap: &SnapshotCache) -> bool {
    snap.width == self.width
      && snap.height == self.height
      && snap.scale == self.scale
      && snap.shaded.as_ref().map_or(0.0, |sc| sc.outset) == self.outset
  }
}

/// How a snapshot boundary's result is composited at its place in the tree:
/// the hoisted transform, the backdrop layer, and the effect-carrying quad
/// (or the inline fallback), built once per visit so every cache state -
/// reused, re-rendered, freshly rasterized, failed - draws through the same
/// code. Group opacity and the view filter ride on the quad paint (white
/// keeps the texture's colors, the alpha fades it, the filters transform the
/// draw), so the texture itself stays effect-free and survives opacity and
/// filter writes.
struct BoundaryComposite<'e> {
  element: &'e Element,
  /// The View's own box transform, hoisted out of the raster
  /// (Hoist::Transform); None for non-View boundaries.
  own: Option<Matrix>,
  /// The inherited frame, for the backdrop bounds of a detached View.
  frame: Size,
  /// The content's region of the (ceil-padded) texture, in texture px.
  src: Rect,
  /// The quad in logical px: the canvas at the node's paint offset.
  dst: Rect,
  quad_paint: Option<Paint>,
  opacity: f32,
  filter: Option<&'e FilterState>,
  scale: f32,
}

impl<'e> BoundaryComposite<'e> {
  fn new(element: &'e Element, own: Option<Matrix>, frame: Size, offset: (f32, f32), key: &SnapshotKey) -> Self {
    let opacity = view_opacity(element);
    let filter = view_filter(element);
    let quad_paint = (opacity < 1.0 || filter.is_some()).then(|| effect_paint(1.0, opacity, filter));
    // The content occupies the top-left canvas*scale pixels of the
    // (ceil-padded) texture; mapping exactly that region onto the
    // logical-size quad keeps the composite pixel-exact under the root scale
    // transform. The quad sits at the detached paint offset the recording
    // countered, pushed out by the shader outset (the effect's transparent
    // margin extends past the box symmetrically).
    let canvas = key.canvas();
    let src = Rect::new(Point::zero(), Size::new(canvas.width * key.scale, canvas.height * key.scale));
    let dst = Rect::new(Point::new(offset.0 - key.outset, offset.1 - key.outset), canvas);
    Self { element, own, frame, src, dst, quad_paint, opacity, filter, scale: key.scale }
  }

  // The shared prologue/epilogue of every composite leg: the hoisted matrix,
  // then the backdrop layer in box space (bounds before any scroll, like
  // record_node's emission order), then the content.
  fn draw(&self, builder: &mut DisplayListBuilder, content: impl FnOnce(&mut DisplayListBuilder)) {
    if let Some(m) = &self.own {
      builder.save();
      builder.transform(m);
    }
    emit_backdrop(builder, self.element, self.frame);
    content(builder);
    if self.own.is_some() {
      builder.restore();
    }
  }

  /// Composite a rasterization (the raw snapshot or a shader pass output) as
  /// the boundary's quad.
  fn draw_texture(&self, builder: &mut DisplayListBuilder, texture: &Texture) {
    self.draw(builder, |b| {
      b.draw_texture_rect(texture, &self.src, &self.dst, TextureSampling::Linear, self.quad_paint.as_ref());
    });
  }

  /// Rasterization failed: replay the recording inline this frame. The
  /// recording carries its own device-scale transform and content offset, so
  /// counter both before replaying.
  fn draw_inline(&self, builder: &mut DisplayListBuilder, dl: &DisplayList) {
    self.draw(builder, |b| {
      b.save();
      b.translate(self.dst.origin.x, self.dst.origin.y);
      b.scale(1.0 / self.scale, 1.0 / self.scale);
      draw_dl_with_effects(b, dl, self.opacity, self.filter);
      b.restore();
    });
  }
}

// A node's painted box relative to its parent-translated origin: a laid-out
// node's layout box. A detached (d-*) node has none, but it is still drawn
// into a definite rectangle: its kind's painted box, sized with the same
// inherited frame its build() reads, so snapshot, capture and paint box the
// node identically by construction rather than by separate derivations. The
// returned offset is the node's own paint offset, countered in the recording
// so the content lands at the texture origin and restored on the composited
// quad's dst - except for a View, whose offset (translate) lives in the
// matrix that Hoist::Transform keeps out of the recording anyway.
pub(super) fn painted_box(element: &Element, frame: Size) -> (f32, f32, (f32, f32)) {
  match element.layout.as_ref() {
    Some(l) => (l.size().width, l.size().height, (0.0, 0.0)),
    None => {
      let local = element.kind.local_bounds(frame);
      let offset = match &element.kind {
        ElementKind::View(_) => (0.0, 0.0),
        _ => (local.origin.x, local.origin.y),
      };
      (local.size.width, local.size.height, offset)
    }
  }
}

// Replays a recorded display list under the view's composite-time effects: a
// filter needs a save_layer carrying it (draw_display_list has only an
// opacity argument), plain opacity stays on the cheap path.
fn draw_dl_with_effects(
  builder: &mut DisplayListBuilder,
  dl: &DisplayList,
  opacity: f32,
  filter: Option<&FilterState>,
) {
  if filter.is_some() {
    let paint = effect_paint(0.0, opacity, filter);
    let bounds = Rect::new(Point::new(-CLIP_INF, -CLIP_INF), Size::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
    builder.save_layer(&bounds, Some(&paint), None);
    builder.draw_display_list(dl, 1.0);
    builder.restore();
  } else {
    builder.draw_display_list(dl, opacity);
  }
}

// Composites a Recording boundary's cached content. A View boundary's cache
// holds children only (Hoist::Full): its current matrix, clip and scroll are
// applied around the draw here, so transform and scroll writes replay the
// same cache. A non-View boundary's cache holds everything and draws bare.
pub(super) fn draw_cached_recording(
  builder: &mut DisplayListBuilder,
  element: &Element,
  matrix: Option<&Matrix>,
  dl: &DisplayList,
  inherited: Size,
) {
  let opacity = view_opacity(element);
  let filter = view_filter(element);
  if let Some(m) = matrix {
    builder.save();
    builder.transform(m);
    apply_clip(builder, element);
    // Box-space bounds: before the scroll translate, like record_node's
    // emission order.
    emit_backdrop(builder, element, inherited);
    apply_scroll(builder, element);
    draw_dl_with_effects(builder, dl, opacity, filter);
    builder.restore();
  } else {
    draw_dl_with_effects(builder, dl, opacity, filter);
  }
}

// Snapshot gate: the subtree is rasterized into a texture at the current
// display scale and composited as a single quad until something inside it
// changes, its layout size changes, or the display scale changes. Content
// painting outside the layout box is cropped (unlike a recording boundary);
// the crop happens in untransformed local space, since the boundary's own
// transform is hoisted out of the raster and applied to the quad instead.
// All storage is exact-size. A declared boundary shader adds one pass over
// the rasterization and composites its output instead (see View::set_shader).
pub(super) fn snapshot_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  aa: bool,
) {
  // The texture outlives this frame's viewport (an ancestor scroll does not
  // invalidate it), so the raster must hold the whole subtree.
  let cull = ctx.cull.take();
  snapshot_node_unculled(scene, node_id, ctx, builder, aa);
  ctx.cull = cull;
}

fn snapshot_node_unculled<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  aa: bool,
) {
  let element = scene.node(node_id);
  // The inherited frame, copied out for the BoundaryComposite's backdrop
  // emission (ctx cannot be re-borrowed at draw time).
  let frame = ctx.size;
  let (width, height, offset) = painted_box(element, ctx.size);
  let scale = ctx.platform.display_scale();
  let box_key = SnapshotKey { width, height, scale, outset: 0.0 };
  let (tex_w, tex_h) = box_key.texture_dims();

  // Without a positive painted box there is nothing to rasterize into; paint
  // inline so overflowing content still shows up.
  if tex_w == 0 || tex_h == 0 {
    record_node(scene, node_id, ctx, builder, Hoist::None);
    return;
  }

  let own = own_matrix(element, ctx.size);
  let hoist = if own.is_some() { Hoist::Transform } else { Hoist::None };

  // The boundary shader (views only) and its pending-write flag, consumed
  // here whichever branch runs: every shaded branch re-runs the pass, and
  // the plain path has nothing to re-run.
  let (shader, shader_dirty) = match &element.kind {
    ElementKind::View(v) => (v.shader.as_ref(), v.take_shader_dirty()),
    _ => (None, false),
  };

  // A withdrawn shader keeps the snapshot: the source texture and its
  // validity are untouched, only the pass output (and any history) drops.
  // Except with an outset - that canvas is bigger than the plain box-sized
  // texture, so the storage cannot be kept.
  if shader.is_none() {
    let mut cache = element.paint_cache.borrow_mut();
    let drop_all = if let Some(PaintCache::Snapshot(snap)) = &mut *cache {
      snap.shaded.take().is_some_and(|sc| sc.outset > 0.0)
    } else {
      false
    };
    if drop_all {
      cache.take();
    }
  }

  if let Some(decl) = shader {
    // The outset grows the canvas symmetrically: content sits at
    // (outset, outset) clipped to the layout box, the margin is transparent
    // and belongs to the effect, and the composited quad extends past the
    // box by the same amount. The pass and all textures work at canvas
    // size, which the key (and the quad derived from it) carries.
    let key = SnapshotKey { outset: decl.outset.max(0.0), ..box_key };
    let outset = key.outset;
    let (tex_w, tex_h) = key.texture_dims();
    let quad = BoundaryComposite::new(element, own, frame, offset, &key);

    // Valid content with matching shader storage: composite the cached
    // output, re-running the pass in place first when a declaration write
    // is pending (the params path - the snapshot is not re-rasterized). An
    // outset change or a `previous` toggle fails the compare instead; both
    // change what storage must exist.
    let cached = {
      let cache = element.paint_cache.borrow();
      match &*cache {
        Some(PaintCache::Snapshot(snap)) => match &snap.shaded {
          Some(sc) if snap.valid && key.matches(snap) && sc.history.is_some() == decl.previous => {
            Some((snap.texture.clone(), sc.output.clone(), sc.history.clone()))
          }
          _ => None,
        },
        _ => None,
      }
    };
    if let Some((source, output, history)) = cached {
      service_captures_under_cache(scene, node_id, ctx, hoist);
      if shader_dirty {
        if let Err(e) = ctx.alloy.rerun_node_shader(decl, &source, &output, history.as_ref(), tex_w, tex_h) {
          log::warn!("boundary shader re-run failed for node {node_id}: {e}");
        }
      }
      ctx.snapshots_reused += 1;
      quad.draw_texture(builder, &output);
      return;
    }

    // Content changed (or the declaration needs different storage): record,
    // rasterize and run the pass in one trip. Dimension-matched storage is
    // re-rendered in place; exact storage means only an exact key match
    // qualifies.
    let mut sub = DisplayListBuilder::new(None);
    sub.scale(scale, scale);
    if outset > 0.0 {
      sub.translate(outset, outset);
      // Without an outset the box crop is the texture viewport itself; with
      // a margin the crop must be explicit, or overflowing content would
      // paint into the effect's transparent margin.
      sub.clip_rect(&Rect::new(Point::new(0.0, 0.0), Size::new(width, height)), ClipOperation::Intersect);
    }
    record_node(scene, node_id, ctx, &mut sub, hoist);
    let Some(dl) = sub.build() else { return };

    // Reusable storage: the source (plain or shaded), plus output and
    // history when the cache was already shaded. A plain cache's texture
    // counts as outset 0, so declaring a no-outset shader over an existing
    // snapshot re-renders its storage instead of reallocating.
    let retained = {
      let cache = element.paint_cache.borrow();
      match &*cache {
        Some(PaintCache::Snapshot(snap)) if key.matches(snap) => {
          let output = snap.shaded.as_ref().map(|sc| sc.output.clone());
          let history = snap.shaded.as_ref().and_then(|sc| sc.history.clone());
          Some((snap.texture.clone(), output, history))
        }
        _ => None,
      }
    };
    // Storage roles for this rasterization. Without `previous` the source
    // re-renders in place. With it, the roles rotate: render into the old
    // history's storage (fresh when none) and bind the old source as
    // uPrevious - the previous rasterization by construction, no copy.
    let (render_into, history_pass, reuse_output) = match &retained {
      Some((source, output, history)) => {
        if decl.previous {
          (history.clone(), Some(source.clone()), output.clone())
        } else {
          (Some(source.clone()), None, output.clone())
        }
      }
      None => (None, None, None),
    };
    let result = ctx.alloy.rasterize_shaded(
      &dl,
      tex_w,
      tex_h,
      aa,
      decl,
      render_into.as_ref(),
      reuse_output.as_ref(),
      history_pass.as_ref(),
    );
    match result {
      Ok((source, output, history)) => {
        if retained.is_some() {
          ctx.snapshots_rerendered += 1;
        } else {
          ctx.snapshots_rasterized += 1;
        }
        publish_snapshot(element, ctx, &source, tex_w, tex_h);
        quad.draw_texture(builder, &output);
        *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot(SnapshotCache {
          texture: source,
          width,
          height,
          scale,
          valid: true,
          shaded: Some(ShadedCache { output, outset, history }),
        }));
      }
      Err(e) => {
        // Paint inline (unshaded) this frame and drop the cache, so no
        // stale storage is offered for in-place reuse on the next damage.
        log::warn!("shaded snapshot failed for node {node_id}: {e}; painting inline unshaded");
        element.paint_cache.borrow_mut().take();
        quad.draw_inline(builder, &dl);
      }
    }
    return;
  }

  let quad = BoundaryComposite::new(element, own, frame, offset, &box_key);

  let reusable = {
    let cache = element.paint_cache.borrow();
    match &*cache {
      Some(PaintCache::Snapshot(snap)) if snap.valid && box_key.matches(snap) => Some(snap.texture.clone()),
      _ => None,
    }
  };
  if let Some(texture) = reusable {
    service_captures_under_cache(scene, node_id, ctx, hoist);
    ctx.snapshots_reused += 1;
    quad.draw_texture(builder, &texture);
    return;
  }

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  if offset != (0.0, 0.0) {
    sub.translate(-offset.0, -offset.1);
  }
  record_node(scene, node_id, ctx, &mut sub, hoist);
  let Some(dl) = sub.build() else { return };

  // Stale storage at unchanged dimensions is re-rendered in place: the
  // offscreen draw clears and rewrites the full allocation, so no stale
  // pixels survive. Exact-size storage means any dimension change
  // reallocates.
  let retained = {
    let cache = element.paint_cache.borrow();
    match &*cache {
      Some(PaintCache::Snapshot(snap)) if box_key.matches(snap) => Some(snap.texture.clone()),
      _ => None,
    }
  };
  if let Some(texture) = retained {
    match ctx.alloy.render_display_list_into_texture(&dl, &texture, tex_w, tex_h, aa) {
      Ok(()) => {
        ctx.snapshots_rerendered += 1;
        publish_snapshot(element, ctx, &texture, tex_w, tex_h);
        quad.draw_texture(builder, &texture);
        *element.paint_cache.borrow_mut() =
          Some(PaintCache::Snapshot(SnapshotCache { texture, width, height, scale, valid: true, shaded: None }));
        return;
      }
      Err(e) => {
        log::warn!("snapshot re-render failed for node {node_id}: {e}; reallocating");
        element.paint_cache.borrow_mut().take();
      }
    }
  }

  match ctx.alloy.render_display_list_to_texture(&dl, tex_w, tex_h, aa) {
    Ok(texture) => {
      ctx.snapshots_rasterized += 1;
      publish_snapshot(element, ctx, &texture, tex_w, tex_h);
      quad.draw_texture(builder, &texture);
      *element.paint_cache.borrow_mut() =
        Some(PaintCache::Snapshot(SnapshotCache { texture, width, height, scale, valid: true, shaded: None }));
    }
    Err(e) => {
      log::warn!("snapshot rasterization failed for node {node_id}: {e}; painting inline");
      quad.draw_inline(builder, &dl);
    }
  }
}

// Re-point a boundary's vended texture id (see RenderTree::snapshot_texture)
// at the rasterization just produced. A boundary nobody asked for publishes
// nothing.
fn publish_snapshot(element: &Element, ctx: &BuildContext<'_>, texture: &Texture, tex_w: u32, tex_h: u32) {
  if let Some(id) = element.snapshot_texture_id.get() {
    ctx.alloy.publish_snapshot_texture(id, texture, tex_w, tex_h);
  }
}
