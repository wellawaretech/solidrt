use std::collections::{HashMap, HashSet};

use taffy::NodeId;

use crate::impellers::Matrix;
use crate::rendertree::{
  BoundaryMode, Damage, Element, ElementKind, PaintCache, Point, Rect, RunOverrides, Size, TextRun, ATOM_CHAR,
};

pub struct RenderTree {
  nodes: HashMap<u64, Element>,
  pub root: Option<u64>,
  // Content revision: bumped on structural changes and on property writes
  // whose damage says content changed (apply_damage, Compose and up).
  // None/Present writes and internal layout writes (node_mut) do not count.
  // Lets a painter detect "tree unchanged since last build" and reuse its
  // display list.
  revision: u64,
}

// Taffy's CompactLength stores f32 values as tagged pointers (*const ()),
// which prevents the auto Send impl. RenderTree is only moved once to the
// UI thread and never shared across threads.
unsafe impl Send for RenderTree {}

impl RenderTree {
  pub fn new() -> Self {
    Self { nodes: HashMap::new(), root: None, revision: 0 }
  }

  pub fn revision(&self) -> u64 {
    self.revision
  }

  fn bump_revision(&mut self) {
    self.revision = self.revision.wrapping_add(1);
  }

  pub fn create_node(&mut self, id: u64, element: Element) -> u64 {
    if self.nodes.contains_key(&id) {
      panic!("duplicate node id {}", id);
    }
    self.nodes.insert(id, element);
    self.bump_revision();
    id
  }

  pub fn insert_node(&mut self, parent_id: u64, node_id: u64, anchor_id: Option<u64>) {
    // Unlink from a previous parent first, so the node never lives in two
    // children lists. Solid's control flow detaches before re-inserting, but a
    // bare move into a different parent (no explicit remove) must stay
    // DOM-faithful. A same-parent reorder falls through to the retain below.
    if let Some(old_parent) = self.try_node(node_id).and_then(|n| n.parent) {
      if old_parent != parent_id && self.try_node(old_parent).is_some() {
        self.detach_node(old_parent, node_id);
      }
    }

    let child_has_layout = {
      let child = self.node_mut(node_id);
      child.parent = Some(parent_id);
      child.has_layout()
    };

    // Detached subtrees must stay entirely detached so a detached node's
    // inherited position and size resolve to a single laid-out ancestor.
    if child_has_layout && !self.node(parent_id).has_layout() {
      panic!(
        "attached node {node_id} cannot be inserted under detached node {parent_id}; \
         detached subtrees must be entirely detached"
      );
    }

    let parent = self.node_mut(parent_id);
    parent.children.retain(|&id| id != node_id);
    if let Some(layout) = &mut parent.layout {
      layout.layout_children.retain(|&id| id != NodeId::from(node_id));
    }

    match anchor_id {
      Some(anchor) => {
        if let Some(pos) = parent.children.iter().position(|&id| id == anchor) {
          parent.children.insert(pos, node_id);
        } else {
          parent.children.push(node_id);
        }
        if child_has_layout {
          if let Some(layout) = &mut parent.layout {
            let anchor_nid = NodeId::from(anchor);
            let node_nid = NodeId::from(node_id);
            if let Some(pos) = layout.layout_children.iter().position(|&id| id == anchor_nid) {
              layout.layout_children.insert(pos, node_nid);
            } else {
              layout.layout_children.push(node_nid);
            }
          }
        }
      }
      None => {
        parent.children.push(node_id);
        if child_has_layout {
          if let Some(layout) = &mut parent.layout {
            layout.layout_children.push(NodeId::from(node_id));
          }
        }
      }
    }

    self.sync_text(parent_id);

    // A detached child is not in layout_children, so inserting it cannot
    // change layout; only paint needs to catch up.
    if child_has_layout {
      self.invalidate_cache(parent_id);
    }
    self.invalidate_paint(parent_id);
    self.bump_revision();
  }

  /// Unlinks `node_id` from its parent but keeps the subtree alive, so it can be
  /// re-inserted elsewhere (a move). This mirrors DOM removeChild, which detaches
  /// rather than destroys; the renderer frees the node later via destroy_node if
  /// nothing re-attaches it. See renderer.ts for the deferred-destroy sweep.
  pub fn detach_node(&mut self, parent_id: u64, node_id: u64) {
    let child_has_layout = self.try_node(node_id).map(|n| n.has_layout()).unwrap_or(false);
    let parent = self.node_mut(parent_id);
    parent.children.retain(|&id| id != node_id);
    if let Some(layout) = &mut parent.layout {
      layout.layout_children.retain(|&id| id != NodeId::from(node_id));
    }
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.parent = None;
    }
    self.sync_text(parent_id);
    if child_has_layout {
      self.invalidate_cache(parent_id);
    }
    self.invalidate_paint(parent_id);
    self.bump_revision();
  }

  /// Frees `node_id` and its whole subtree. Call after detach_node once the node
  /// is confirmed dead (not moved). Defensively unlinks from any parent still
  /// referencing it, so a direct destroy leaves no dangling child entry.
  pub fn destroy_node(&mut self, node_id: u64) {
    if let Some(parent_id) = self.try_node(node_id).and_then(|n| n.parent) {
      let child_has_layout = self.try_node(node_id).map(|n| n.has_layout()).unwrap_or(false);
      if let Some(parent) = self.nodes.get_mut(&parent_id) {
        parent.children.retain(|&id| id != node_id);
        if let Some(layout) = &mut parent.layout {
          layout.layout_children.retain(|&id| id != NodeId::from(node_id));
        }
        self.sync_text(parent_id);
        if child_has_layout {
          self.invalidate_cache(parent_id);
        }
        self.invalidate_paint(parent_id);
      }
    }
    self.delete_recursive(node_id);
    self.bump_revision();
  }

  /// Detach then destroy in one step. Retained for callers (and tests) that want
  /// the old remove-and-free semantics without the deferred sweep.
  pub fn delete_node(&mut self, parent_id: u64, node_id: u64) {
    self.detach_node(parent_id, node_id);
    self.destroy_node(node_id);
  }

  /// A scoped property write: hands `f` the element, then completes the write
  /// with the Damage it reports - `apply_damage` (where the revision bumps for
  /// content-bearing damage) plus the span parent-text resync. The closure
  /// must produce a Damage to compile, so a mutation cannot forget the
  /// invalidation half of the transaction.
  pub fn edit(&mut self, id: u64, f: impl FnOnce(&mut Element) -> Damage) {
    let damage = f(self.node_mut(id));
    self.apply_damage(id, damage);
    self.sync_span_parent(id);
  }

  /// `edit` for writes decoded from untrusted input (the FFI property path):
  /// on Err nothing is invalidated and the error returns to the caller to
  /// surface as a script error instead of a process abort.
  pub fn try_edit<E>(&mut self, id: u64, f: impl FnOnce(&mut Element) -> Result<Damage, E>) -> Result<(), E> {
    let damage = f(self.node_mut(id))?;
    self.apply_damage(id, damage);
    self.sync_span_parent(id);
    Ok(())
  }

  /// Completes a property write by invalidating what the setter reported (see
  /// `Damage`). Compose writes keep the node's own paint cache - the state is
  /// applied at composite time - and clear from the parent up, since ancestor
  /// recordings hold the node's old composited result.
  pub fn apply_damage(&mut self, node_id: u64, damage: Damage) {
    match damage {
      Damage::None | Damage::Present => return,
      _ => self.bump_revision(),
    }
    match damage {
      Damage::None | Damage::Present => {}
      Damage::Compose => {
        if let Some(parent) = self.try_node(node_id).and_then(|e| e.parent) {
          self.invalidate_paint(parent);
        }
      }
      Damage::Scroll => {
        // Like Compose on a Recording boundary (its cache holds children
        // only; composite re-applies clip and scroll), like Paint elsewhere
        // (a Snapshot texture lacks scrolled-out pixels; a non-boundary has
        // no cache of its own to keep).
        let keeps_cache =
          self.try_node(node_id).map(|e| e.repaint_boundary == BoundaryMode::Recording).unwrap_or(false);
        if keeps_cache {
          if let Some(parent) = self.try_node(node_id).and_then(|e| e.parent) {
            self.invalidate_paint(parent);
          }
        } else {
          self.invalidate_paint(node_id);
        }
      }
      Damage::Paint => self.invalidate_paint(node_id),
      Damage::Layout => {
        self.invalidate_cache(node_id);
        self.invalidate_paint(node_id);
      }
    }
  }

  /// Take the window root's pending shader change without a paint walk. The
  /// rebuild path flushes the same pending in `Window::build`; the
  /// present-only reuse path (which skips the walk) calls this before
  /// resubmitting the cached display list, keeping the raster channel's
  /// declaration-before-frame ordering.
  pub fn take_pending_window_shader(&mut self) -> Option<Option<crate::gpu::WindowShader>> {
    let root = self.root?;
    match &mut self.nodes.get_mut(&root)?.kind {
      ElementKind::Window(w) => w.take_pending_shader(),
      _ => None,
    }
  }

  /// Clear cached boundary recordings from `node_id` up to the root. A content
  /// or layout change invalidates the enclosing boundary and - because
  /// draw_display_list copies commands into the enclosing recording - every
  /// boundary above it as well. A stale recording is dropped (it is worthless
  /// and costs nothing to rebuild); a stale snapshot keeps its texture and is
  /// only marked invalid, so the next raster reuses the allocation instead of
  /// rebuilding the offscreen rig.
  pub fn invalidate_paint(&self, node_id: u64) {
    let mut current = Some(node_id);
    while let Some(id) = current {
      let Some(element) = self.try_node(id) else {
        break;
      };
      let mut cache = element.paint_cache.borrow_mut();
      match &mut *cache {
        Some(PaintCache::Snapshot(snap)) => snap.valid = false,
        _ => {
          cache.take();
        }
      }
      current = element.parent;
    }
  }

  pub(crate) fn node(&self, id: u64) -> &Element {
    self.nodes.get(&id).expect(&format!("node {} not found", id))
  }

  /// Read access to a node for inspection surfaces (the dev-server tree
  /// query reads current property values through this). Mutation stays
  /// behind edit/try_edit, which own damage tracking.
  pub fn try_node(&self, id: u64) -> Option<&Element> {
    self.nodes.get(&id)
  }

  /// Bounding box of a node relative to its nearest positioning context: the
  /// closest ancestor whose JSX explicitly set `position="relative"`. Falls
  /// back to the window when there is none. This is the frame an absolutely
  /// positioned sibling overlay is drawn in, so coordinates from here can feed
  /// directly into such an overlay. Detached nodes report the box inherited from
  /// their nearest laid-out ancestor. Returns None before the first layout.
  pub fn bounding_box(&self, id: u64) -> Option<Rect> {
    self.compute_bounding_box(id, true)
  }

  /// Bounding box of a node relative to the window root (CSS getBoundingClientRect
  /// semantics), for callers that want absolute coordinates (e.g. snapshot).
  pub fn bounding_box_viewport(&self, id: u64) -> Option<Rect> {
    self.compute_bounding_box(id, false)
  }

  /// Computed lazily: walks from the node upward each call, so nothing is cached
  /// and only queried nodes cost anything. Call after layout_phase (e.g. from
  /// the postLayout hook) for current-frame values. When `stop_at_context` is
  /// set the ascent stops at (and does not fold in) the first positioning
  /// context ancestor, yielding coordinates in that ancestor's frame; otherwise
  /// it continues to the root.
  ///
  /// The four corners of the node's box are carried up through every ancestor:
  /// layout position, scroll (inverse), and the full View paint matrix
  /// (translate, rotate, scale, 3D) all compose, the forward companion of the
  /// hit-test descent. The result is the axis-aligned bounds of the transformed
  /// quad (CSS getBoundingClientRect semantics). Corners transform on the
  /// z = 0 plane with the homogeneous divide, the same approximation hit
  /// testing uses under perspective. Views without matrix props keep the cheap
  /// translation-only path.
  fn compute_bounding_box(&self, id: u64, stop_at_context: bool) -> Option<Rect> {
    // The axis-aligned bounds of the transformed quad.
    self.compute_corners(id, stop_at_context).map(Rect::from_points)
  }

  /// The four corners of the node's painted box in window coordinates, after
  /// every transform on the ancestor chain - the quad `bounding_box_viewport`
  /// collapses to an AABB. Corner order follows `rect_corners` (top-left,
  /// top-right, bottom-right, bottom-left, pre-transform). Under a rotation
  /// or 3D transform the AABB alone says a transform happened but not where
  /// the edges landed; the quad is the readable form.
  pub fn painted_quad(&self, id: u64) -> Option<[Point; 4]> {
    self.compute_corners(id, false)
  }

  fn compute_corners(&self, id: u64, stop_at_context: bool) -> Option<[Point; 4]> {
    let node = self.try_node(id)?;
    let local = node.kind.local_bounds(self.content_fallback(id)?);

    // A View's own paint matrix already contains its translate (which is what
    // local_bounds reports as its origin), so the matrix path starts from the
    // plain layout box and applies the matrix instead; every other case keeps
    // the kind's local offset. The view's OWN box transforms by the user chain
    // only (box_matrix): a viewBox fit maps children into the box, it never
    // moves the box itself.
    let mut corners = match &node.kind {
      ElementKind::View(v) if v.needs_matrix() => {
        let m = v.box_matrix(local.size);
        rect_corners(&Rect::new(Point::zero(), local.size)).map(|p| transform_point(&m, p))
      }
      _ => rect_corners(&local),
    };

    // Detached nodes have no layout placement; they inherit position from the
    // ancestor walk below.
    if let Some(layout) = node.layout.as_ref() {
      let loc = layout.location().to_vector();
      for p in corners.iter_mut() {
        *p += loc;
      }
    }

    // Ascend. Per ancestor, in application order: remove any scroll it applies
    // to its children, apply its paint matrix (or just its translate when no
    // matrix props are set), then add its layout position to enter the next
    // frame up. For the container-relative box, stop before folding in the
    // first positioning context: the result is then expressed in that
    // ancestor's frame. Absolute ancestors are deliberately transparent here -
    // their offset is still accumulated, they just never act as the stop.
    let mut cur_id = id;
    loop {
      let Some(parent_id) = self.try_node(cur_id).and_then(|n| n.parent) else {
        break;
      };
      let Some(parent) = self.try_node(parent_id) else {
        break;
      };
      if stop_at_context && parent.layout.as_ref().is_some_and(|l| l.positioning_context) {
        break;
      }
      if let ElementKind::View(v) = &parent.kind {
        if v.scroll.is_some() {
          let size =
            parent.layout.as_ref().map(|l| l.size()).or_else(|| self.content_fallback(parent_id)).unwrap_or_default();
          // Scroll means box pixels; these corners are in the parent's child
          // frame (design space under a viewBox fit), so the offset divides
          // by the fit scale, matching the hit descent and the paint order.
          let s = v.content_scroll(size);
          for p in corners.iter_mut() {
            *p -= s;
          }
        }
        if v.needs_matrix() {
          let size =
            parent.layout.as_ref().map(|l| l.size()).or_else(|| self.content_fallback(parent_id)).unwrap_or_default();
          let m = v.paint_matrix(size);
          for p in corners.iter_mut() {
            *p = transform_point(&m, *p);
          }
        } else if let Some(t) = v.translate {
          for p in corners.iter_mut() {
            *p += t;
          }
        }
      }
      if let Some(parent_layout) = parent.layout.as_ref() {
        let loc = parent_layout.location().to_vector();
        for p in corners.iter_mut() {
          *p += loc;
        }
      }
      cur_id = parent_id;
    }

    Some(corners)
  }

  /// Fallback size for shapes without explicit w/h: the nearest laid-out node's
  /// box (self, or the ancestor a detached subtree hangs from). None before the
  /// first layout has populated the cache.
  fn content_fallback(&self, id: u64) -> Option<Size> {
    let mut cur = id;
    loop {
      let node = self.try_node(cur)?;
      // A viewBox ANCESTOR redefines the space its children draw in: the box
      // they inherit is the design size, which the fit matrix maps onto the
      // layout box during the ancestor walk. The node's own view_box does not
      // apply to itself (its own box is its layout box).
      if cur != id {
        if let ElementKind::View(v) = &node.kind {
          if let Some(vb) = v.view_box {
            return Some(vb);
          }
        }
      }
      if let Some(layout) = node.layout.as_ref() {
        if layout.cache.is_empty() {
          return None;
        }
        return Some(layout.size());
      }
      cur = node.parent?;
    }
  }

  pub(crate) fn node_mut(&mut self, id: u64) -> &mut Element {
    self.nodes.get_mut(&id).expect(&format!("node {} not found", id))
  }

  fn delete_recursive(&mut self, node_id: u64) {
    let child_ids: Vec<u64> = self.nodes.get(&node_id).map(|e| e.children.clone()).unwrap_or_default();
    for child_id in child_ids {
      self.delete_recursive(child_id);
    }
    self.nodes.remove(&node_id);
  }

  /// Rebuild a Text's computed_text and styled runs from its Span subtree.
  /// `id` may be the Text or any span inside it: spans nest, so the owning
  /// Text is found by walking up. No-op for other kinds. Detached text never
  /// enters layout, so structural and span changes sync eagerly here.
  fn sync_text(&mut self, id: u64) {
    let mut text_id = id;
    loop {
      let Some(element) = self.try_node(text_id) else { return };
      match element.kind {
        ElementKind::Text(_) => break,
        ElementKind::Span(_) => match element.parent {
          Some(parent) => text_id = parent,
          None => return,
        },
        _ => return,
      }
    }
    let mut text = String::new();
    let mut runs = Vec::new();
    let children = self.node(text_id).children.clone();
    for child_id in children {
      self.collect_runs(child_id, &RunOverrides::default(), &mut text, &mut runs);
    }
    if let ElementKind::Text(t) = &mut self.node_mut(text_id).kind {
      // An atom's box is the layout pass's to write; a resync keeps the box
      // it last measured so the paragraph is not re-shaped for nothing.
      for run in runs.iter_mut().filter(|r| r.atom.is_some()) {
        if let Some(size) = t.runs.iter().find(|r| r.node == run.node).and_then(|r| r.atom) {
          run.atom = Some(size);
        }
      }
      t.computed_text = text;
      t.runs = runs;
    }
  }

  /// Depth-first over a span subtree: a span's own text is a run under the
  /// overrides layered so far, then its children under those plus its own. A
  /// laid-out element (necessarily a direct child of the text: an attached
  /// node cannot sit under a span) is an inline atom run.
  fn collect_runs(&self, id: u64, inherited: &RunOverrides, text: &mut String, runs: &mut Vec<TextRun>) {
    let node = self.node(id);
    let ElementKind::Span(span) = &node.kind else {
      if node.has_layout() {
        text.push_str(ATOM_CHAR);
        runs.push(TextRun {
          text: ATOM_CHAR.to_string(),
          overrides: inherited.clone(),
          node: id,
          atom: Some(Size::zero()),
        });
      }
      return;
    };
    let overrides = inherited.layer(&span.overrides);
    if !span.text.is_empty() {
      text.push_str(&span.text);
      runs.push(TextRun { text: span.text.clone(), overrides: overrides.clone(), node: id, atom: None });
    }
    for &child_id in &node.children {
      self.collect_runs(child_id, &overrides, text, runs);
    }
  }

  /// If `node_id` is a Span, resync the Text that owns it. Called after a
  /// property write; no-op for every other kind, so callers need not check.
  pub fn sync_span_parent(&mut self, node_id: u64) {
    if matches!(self.try_node(node_id).map(|n| &n.kind), Some(ElementKind::Span(_))) {
      self.sync_text(node_id);
    }
  }

  pub fn invalidate_cache(&mut self, node_id: u64) {
    let mut current = Some(node_id);
    while let Some(id) = current {
      let element = self.node_mut(id);
      let Some(layout) = &mut element.layout else {
        // Of the layout-less kinds, only a span passes the invalidation
        // through: its text feeds the parent paragraph's measurement. Anything
        // else is a detached node, and a detached subtree can never alter
        // layout, so the attached ancestors' caches stay valid.
        current = match element.kind {
          ElementKind::Span(_) => element.parent,
          _ => None,
        };
        continue;
      };
      if layout.cache.is_empty() {
        break;
      }
      layout.cache.clear();
      crate::rendertree::counters::note_dirtied();
      current = element.parent;
    }
  }

  /// Number of live nodes (attached and detached), for the stats counters.
  pub fn node_count(&self) -> usize {
    self.nodes.len()
  }

  /// GPU-side content changed behind these texture ids - target re-renders,
  /// uploads, copies, camera frames - with no tree damage of their own
  /// (Context::take_content_changes is the source). Live texture references
  /// need nothing: the resubmitted display list samples the fresh pixels at
  /// the raster flush. The one consumer that goes stale is baked pixels, so
  /// this damages exactly the referencing nodes and leaves everything else
  /// alone. Two reference shapes, two costs: a texture element under a
  /// snapshot boundary is IN the bake and re-rasterizes
  /// (`invalidate_paint`); a boundary shader sampling the id as an extra
  /// input only needs its pass re-run over the still-valid snapshot, so it
  /// gets the params-write treatment (`mark_shader_dirty` + Compose damage:
  /// ancestor recordings repaint, the bake survives). Bumps the revision
  /// only on a hit: a frame whose GPU writes touch no snapshot boundary
  /// keeps the present-only reuse path. Returns whether anything was
  /// invalidated.
  pub fn texture_content_changed(&mut self, ids: &HashSet<u64>) -> bool {
    if ids.is_empty() {
      return false;
    }
    let mut bake_hits: Vec<u64> = Vec::new();
    let mut shader_hits: Vec<u64> = Vec::new();
    for (id, element) in self.nodes.iter() {
      match &element.kind {
        ElementKind::Texture(t) => {
          if t.texture_id.is_some_and(|t| ids.contains(&t)) && self.under_snapshot_boundary(*id) {
            bake_hits.push(*id);
          }
        }
        // The shader only runs on the view's OWN snapshot boundary (declared
        // without one it is ignored with a warning), so ancestor boundaries
        // are irrelevant here.
        ElementKind::View(v) => {
          if matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa)
            && v.shader.as_ref().is_some_and(|s| s.textures.iter().any(|(_, t)| ids.contains(t)))
          {
            shader_hits.push(*id);
          }
        }
        _ => {}
      }
    }
    if bake_hits.is_empty() && shader_hits.is_empty() {
      return false;
    }
    for id in &bake_hits {
      self.invalidate_paint(*id);
    }
    for id in shader_hits {
      if let Some(element) = self.try_node(id) {
        if let ElementKind::View(v) = &element.kind {
          v.mark_shader_dirty();
        }
      }
      self.apply_damage(id, Damage::Compose);
    }
    self.bump_revision();
    true
  }

  /// Whether `node_id` or any ancestor is a snapshot repaint boundary - the
  /// probe that keeps texture_content_changed off everything else. Boundary
  /// MODE, not cache presence: a declared boundary that has not baked yet has
  /// nothing stale (the invalidation is a no-op then), and mode is plain
  /// element state.
  fn under_snapshot_boundary(&self, node_id: u64) -> bool {
    let mut current = Some(node_id);
    while let Some(id) = current {
      let Some(element) = self.try_node(id) else {
        return false;
      };
      if matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa) {
        return true;
      }
      current = element.parent;
    }
    false
  }

  /// Texture registry ids referenced by any live element, attached or
  /// detached (a detached node can be re-inserted, so its reference counts).
  /// Texture elements are the only kind holding registry ids. Used by the
  /// deferred-destroy sweep to decide which pending ids are reclaimable; only
  /// called while destroys are pending, so it stays off the per-frame path.
  pub fn referenced_texture_ids(&self) -> HashSet<u64> {
    let mut ids = HashSet::new();
    for element in self.nodes.values() {
      if let ElementKind::Texture(t) = &element.kind {
        if let Some(id) = t.texture_id {
          ids.insert(id);
        }
      }
    }
    ids
  }

  /// Number of nodes reachable from the root. The difference to node_count()
  /// is the orphan population: nodes created but never inserted, or detached
  /// and never destroyed - a growing gap at a stable tree shape means an
  /// unmount leak. Walks the mounted tree, so it is meant for on-demand
  /// diagnostics (the stats query), not per-frame use.
  pub fn mounted_count(&self) -> usize {
    let Some(root) = self.root else { return 0 };
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
      let Some(node) = self.try_node(id) else { continue };
      count += 1;
      stack.extend(node.children.iter().copied());
    }
    count
  }

  /// Plain-data copy of the whole tree for external inspection (debug and dev
  /// tooling). Engine-free: the caller decides how to encode it. Boxes are
  /// window-relative (see bounding_box_viewport) and zero before the first
  /// layout.
  pub fn snapshot(&self) -> Option<NodeSnapshot> {
    self.snapshot_from(None, None)
  }

  /// Subtree snapshot: start at `root` (the tree root when None) and include
  /// `depth` levels of children below it (unlimited when None; 0 = the start
  /// node only). A node whose children are cut off by the cap still reports
  /// them through `child_count`. None when the tree is empty or `root` is not
  /// a current node.
  pub fn snapshot_from(&self, root: Option<u64>, depth: Option<usize>) -> Option<NodeSnapshot> {
    let start = root.or(self.root)?;
    self.snapshot_node(start, depth.unwrap_or(usize::MAX))
  }

  /// Depth-first search of the subtree under `root` (the tree root when None)
  /// for nodes whose kind equals `query` or whose text contains it, both
  /// case-insensitive. Matches are childless snapshots paired with the id
  /// path from the search root down to the node (inclusive), capped at
  /// `limit`. None when the tree is empty or `root` is not a current node.
  pub fn snapshot_matches(&self, root: Option<u64>, query: &str, limit: usize) -> Option<Vec<NodeMatch>> {
    let start = root.or(self.root)?;
    self.try_node(start)?;
    let needle = query.to_lowercase();
    let mut matches = Vec::new();
    let mut path = Vec::new();
    self.collect_matches(start, &needle, limit, &mut path, &mut matches);
    Some(matches)
  }

  fn collect_matches(&self, id: u64, needle: &str, limit: usize, path: &mut Vec<u64>, out: &mut Vec<NodeMatch>) {
    if out.len() >= limit {
      return;
    }
    let Some(node) = self.try_node(id) else { return };
    path.push(id);
    let kind_hit = node.kind.name().eq_ignore_ascii_case(needle);
    let text_hit = match &node.kind {
      ElementKind::Text(t) => t.computed_text.to_lowercase().contains(needle),
      ElementKind::Span(s) => s.text.to_lowercase().contains(needle),
      _ => false,
    };
    if kind_hit || text_hit {
      if let Some(snapshot) = self.snapshot_node(id, 0) {
        out.push(NodeMatch { path: path.clone(), node: snapshot });
      }
    }
    for &child in &node.children {
      self.collect_matches(child, needle, limit, path, out);
    }
    path.pop();
  }

  fn snapshot_node(&self, id: u64, depth: usize) -> Option<NodeSnapshot> {
    let node = self.try_node(id)?;
    let bounds = self.bounding_box_viewport(id).unwrap_or(Rect::zero());
    let text = match &node.kind {
      ElementKind::Text(t) => Some(t.computed_text.clone()),
      ElementKind::Span(s) => Some(s.text.clone()),
      _ => None,
    };
    let children = if depth == 0 {
      Vec::new()
    } else {
      node.children.iter().filter_map(|&child| self.snapshot_node(child, depth - 1)).collect()
    };
    Some(NodeSnapshot {
      id,
      kind: node.kind.name(),
      detached: !node.has_layout(),
      x: bounds.origin.x,
      y: bounds.origin.y,
      width: bounds.size.width,
      height: bounds.size.height,
      text,
      child_count: node.children.len(),
      children,
    })
  }
}

// The four corners of a rectangle, clockwise from top-left.
fn rect_corners(r: &Rect) -> [Point; 4] {
  let (min, max) = (r.origin, r.origin + r.size);
  [min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)]
}

// Forward companion of View::transform_to_local: applies a paint matrix to a
// point on the z = 0 plane with the homogeneous divide. A degenerate w (only
// reachable under perspective; euclid returns None for w <= 0) leaves the
// point untransformed rather than poisoning the box with infinities.
fn transform_point(m: &Matrix, p: Point) -> Point {
  m.transform_point2d(p).unwrap_or(p)
}

/// One node of a RenderTree::snapshot: kind, window-relative box, text content
/// and children. `children` may hold fewer entries than `child_count` when a
/// depth cap cut the copy off.
pub struct NodeSnapshot {
  pub id: u64,
  pub kind: &'static str,
  pub detached: bool,
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
  pub text: Option<String>,
  pub child_count: usize,
  pub children: Vec<NodeSnapshot>,
}

/// One result of RenderTree::snapshot_matches: the childless node snapshot
/// plus the id path from the search root down to it (inclusive), for rooting
/// a follow-up snapshot_from at the match or an ancestor.
pub struct NodeMatch {
  pub path: Vec<u64>,
  pub node: NodeSnapshot,
}
