mod geometry;
mod inspect;
mod transitions;

pub use inspect::{NodeMatch, NodeSnapshot};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use taffy::NodeId;

use crate::rendertree::damage::DamageLedger;
use crate::rendertree::transitions::Transitions;
use crate::rendertree::{
  BoundaryMode, Damage, Element, ElementKind, FrameDamage, PaintCache, RunOverrides, Size, TextRun, ATOM_CHAR,
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
  // Native transitions (see transitions.rs): the running tracks and the
  // animation clock.
  transitions: Transitions,
  // Vended snapshot texture ids (Element::snapshot_texture_id) whose boundary
  // was deleted, awaiting release by the frame loop. Interior-mutable so the
  // paint walk can drain it through the shared tree borrow it already holds.
  released_snapshot_textures: RefCell<Vec<u64>>,
  // Nodes referencing any texture-registry id (Element::references_textures):
  // texture elements with a source, views whose shader samples extra texture
  // inputs. Keeps texture_content_changed and the destroy sweep at
  // O(referencers) instead of O(nodes). Membership is reconciled after every
  // edit/try_edit and on create/destroy, so it cannot drift from element
  // state whatever a write closure touched.
  texture_referencers: HashSet<u64>,
  // Partial repaint (okf/done/partial-repaint.md): the damage accumulated
  // since the last resolve and the resolution state, owned as one protocol
  // (see damage.rs). Every damage and structural mutation path funnels into
  // note_damage; the composite damage resolves drain and settle it.
  damage: DamageLedger,
}

// Taffy's CompactLength stores f32 values as tagged pointers (*const ()),
// which prevents the auto Send impl. RenderTree is only moved once to the
// UI thread and never shared across threads.
unsafe impl Send for RenderTree {}

impl RenderTree {
  pub fn new() -> Self {
    Self {
      nodes: HashMap::new(),
      root: None,
      revision: 0,
      transitions: Transitions::default(),
      released_snapshot_textures: RefCell::new(Vec::new()),
      texture_referencers: HashSet::new(),
      damage: DamageLedger::new(),
    }
  }

  fn reconcile_texture_referencer(&mut self, node_id: u64) {
    if self.nodes.get(&node_id).is_some_and(Element::references_textures) {
      self.texture_referencers.insert(node_id);
    } else {
      self.texture_referencers.remove(&node_id);
    }
  }

  #[cfg(test)]
  pub(crate) fn texture_referencers(&self) -> &HashSet<u64> {
    &self.texture_referencers
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
    self.reconcile_texture_referencer(id);
    self.bump_revision();
    id
  }

  /// Make `id`, an existing window node, the root. Creating a window sets
  /// the root as a side effect, so a second window (an error window
  /// replacing the app's) takes it over; this is the way back to the first
  /// without recreating it, same node and subtree. Layout reads the window
  /// size from the platform each frame, so a re-rooted window needs nothing
  /// else; the paint invalidation and revision bump force a fresh frame.
  pub fn set_root(&mut self, id: u64) {
    if self.root == Some(id) || !self.nodes.contains_key(&id) {
      return;
    }
    self.root = Some(id);
    self.damage_all();
    self.invalidate_paint(id);
    self.bump_revision();
  }

  /// Insert `node_id` under `parent_id` (before `anchor_id`, else last). Refused,
  /// with the tree untouched, when a laid-out node would land under a detached
  /// parent: a detached subtree must stay entirely detached so a detached
  /// node's inherited position and size resolve to a single laid-out ancestor.
  /// The message names the kinds as JSX tags, since that is where the mistake
  /// is made.
  pub fn insert_node(&mut self, parent_id: u64, node_id: u64, anchor_id: Option<u64>) -> Result<(), String> {
    let child_has_layout = self.node(node_id).has_layout();
    let child_is_span = matches!(self.node(node_id).kind, ElementKind::Span(_));
    if child_has_layout && !self.node(parent_id).has_layout() {
      return Err(format!(
        "<{}> cannot be a child of <d-{}>: a detached subtree must be entirely detached (d-* elements only)",
        self.node(node_id).kind.name(),
        self.node(parent_id).kind.name()
      ));
    }

    // A re-insert of an exiting node is a move (Solid detaches before
    // re-inserting), not a removal: abandon the exit and carry on.
    self.abandon_exit(node_id);

    // Unlink from a previous parent first, so the node never lives in two
    // children lists. Solid's control flow detaches before re-inserting, but a
    // bare move into a different parent (no explicit remove) must stay
    // DOM-faithful. A same-parent reorder falls through to the retain below.
    if let Some(old_parent) = self.try_node(node_id).and_then(|n| n.parent) {
      if old_parent != parent_id && self.try_node(old_parent).is_some() {
        self.detach_node_now(old_parent, node_id);
      }
    }

    self.node_mut(node_id).parent = Some(parent_id);

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
    // change layout and only paint needs to catch up - except a span, whose
    // text feeds its paragraph's measure (invalidate_cache walks it up).
    if child_has_layout || child_is_span {
      self.invalidate_cache(parent_id);
    }
    // The parent id carries the damage: its extent covered the subtree
    // before the insert and covers the new child after the next layout.
    self.note_damage(parent_id);
    self.invalidate_paint(parent_id);
    self.bump_revision();

    self.apply_enter_transitions(node_id);
    Ok(())
  }


  /// Unlinks `node_id` from its parent but keeps the subtree alive, so it can be
  /// re-inserted elsewhere (a move). This mirrors DOM removeChild, which detaches
  /// rather than destroys; the renderer frees the node later via destroy_node if
  /// nothing re-attaches it. See renderer.ts for the deferred-destroy sweep.
  ///
  /// Exit animations hook in here: a node whose transition declares `exit`
  /// values stays linked and animates them instead (see `begin_exit`). If the
  /// same tick re-inserts the node - a move - the exit is abandoned and the
  /// unlink happens then, so moves never play removal animations. The
  /// deferred destroy finding the node exiting defers the free to the settle.
  pub fn detach_node(&mut self, parent_id: u64, node_id: u64) {
    if self.begin_exit(parent_id, node_id) {
      return;
    }
    self.detach_node_now(parent_id, node_id);
  }

  fn detach_node_now(&mut self, parent_id: u64, node_id: u64) {
    let child_has_layout = self.try_node(node_id).map(|n| n.has_layout()).unwrap_or(false);
    let child_is_span = self.try_node(node_id).map(|n| matches!(n.kind, ElementKind::Span(_))).unwrap_or(false);
    let parent = self.node_mut(parent_id);
    parent.children.retain(|&id| id != node_id);
    if let Some(layout) = &mut parent.layout {
      layout.layout_children.retain(|&id| id != NodeId::from(node_id));
    }
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.parent = None;
    }
    self.sync_text(parent_id);
    if child_has_layout || child_is_span {
      self.invalidate_cache(parent_id);
    }
    // The parent's last extent covers the detached subtree's pixels.
    self.note_damage(parent_id);
    self.invalidate_paint(parent_id);
    self.bump_revision();
  }


  /// Frees `node_id` and its whole subtree. Call after detach_node once the node
  /// is confirmed dead (not moved). Defensively unlinks from any parent still
  /// referencing it, so a direct destroy leaves no dangling child entry.
  ///
  /// A node mid-exit is not freed yet: the destroy is remembered (`doomed`)
  /// and happens when the exit settles. Descendants of a destroyed node never
  /// exit-animate on their own - only the node the renderer removes does, and
  /// its whole subtree stays painted with it until the settle.
  pub fn destroy_node(&mut self, node_id: u64) {
    if let Some(el) = self.nodes.get_mut(&node_id) {
      if el.exiting {
        el.doomed = true;
        return;
      }
    }
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
        // The parent's last extent covers the destroyed subtree's pixels.
        self.note_damage(parent_id);
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
    self.reconcile_texture_referencer(id);
    self.apply_damage(id, damage);
    self.sync_span_parent(id);
  }


  /// Partial repaint (okf/done/partial-repaint.md): note that `id`'s
  /// on-screen pixels may differ from the last painted frame. Every damage
  /// and structural mutation path funnels here.
  pub(crate) fn note_damage(&mut self, id: u64) {
    self.damage.note(id);
  }

  /// Degrade the frame being accumulated to full damage (resize, re-root).
  pub fn damage_all(&mut self) {
    self.damage.all();
  }

  /// The damage the last painted frame resolved to.
  pub fn frame_damage(&self) -> FrameDamage {
    self.damage.frame_damage()
  }

  /// The damage ledger, for the composite damage resolves (take + resolve;
  /// see damage.rs).
  pub(crate) fn damage_ledger(&mut self) -> &mut DamageLedger {
    &mut self.damage
  }

  /// `apply_damage` for a frame's worth of writes at once: one revision
  /// bump, and the ancestor invalidation walks share a visited set so
  /// common ancestors are cleared once per batch instead of once per write
  /// (the per-write walk is O(depth), and a frame of N animated nodes would
  /// otherwise pay it N times).
  pub fn apply_damage_batch(&mut self, items: &[(u64, Damage)]) {
    let mut any = false;
    let mut visited: HashSet<u64> = HashSet::new();
    for &(node_id, damage) in items {
      if matches!(damage, Damage::None | Damage::Present) {
        continue;
      }
      any = true;
      self.note_damage(node_id);
      if let Some(element) = self.try_node(node_id) {
        element.envelope.clear();
      }
      let walk_from = match damage {
        Damage::None | Damage::Present => None,
        Damage::Compose => self.try_node(node_id).and_then(|e| e.parent),
        Damage::Scroll => {
          let keeps_cache =
            self.try_node(node_id).map(|e| e.repaint_boundary == BoundaryMode::Recording).unwrap_or(false);
          if keeps_cache {
            self.try_node(node_id).and_then(|e| e.parent)
          } else {
            Some(node_id)
          }
        }
        Damage::Paint => Some(node_id),
        Damage::Layout => {
          self.invalidate_cache(node_id);
          Some(node_id)
        }
      };
      if let Some(start) = walk_from {
        self.invalidate_paint_batched(start, &mut visited);
      }
    }
    if any {
      self.bump_revision();
    }
  }

  pub fn try_edit<E>(&mut self, id: u64, f: impl FnOnce(&mut Element) -> Result<Damage, E>) -> Result<(), E> {
    let result = f(self.node_mut(id));
    // Even on Err: the closure may have mutated before failing, and the
    // referencer index must track element state, not the damage outcome.
    self.reconcile_texture_referencer(id);
    let damage = result?;
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
    self.note_damage(node_id);
    // The node's paint envelope (cull.rs) is stated past its own matrix and
    // scroll, so even the damages below that spare the node's own paint cache
    // (Compose, Scroll on a Recording boundary) invalidate it; the ancestors'
    // envelopes go with the invalidate_paint walks.
    if let Some(element) = self.try_node(node_id) {
      element.envelope.clear();
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
      current = self.invalidate_paint_step(id);
    }
  }

  // One node of the invalidate_paint walk: clear the node's retained paint
  // (a snapshot is marked stale, keeping its texture allocation) and its
  // envelope, and hand back the parent to continue with.
  fn invalidate_paint_step(&self, id: u64) -> Option<u64> {
    let element = self.try_node(id)?;
    let mut cache = element.paint_cache.borrow_mut();
    match &mut *cache {
      Some(PaintCache::Snapshot(snap)) => snap.valid = false,
      _ => {
        cache.take();
      }
    }
    element.envelope.clear();
    element.parent
  }

  /// The invalidate_paint walk for a batch of start nodes sharing one
  /// `visited` set: an ancestor already cleared by an earlier walk in the
  /// batch ends the walk, so N animated siblings clear their shared
  /// ancestors once per frame instead of once per write. Idempotent per
  /// node, so stopping at a visited ancestor loses nothing.
  fn invalidate_paint_batched(&self, node_id: u64, visited: &mut HashSet<u64>) {
    let mut current = Some(node_id);
    while let Some(id) = current {
      if !visited.insert(id) {
        return;
      }
      current = self.invalidate_paint_step(id);
    }
  }

  pub(crate) fn node(&self, id: u64) -> &Element {
    // See node_mut: no eager format! on the hot path.
    self.nodes.get(&id).unwrap_or_else(|| panic!("node {} not found", id))
  }

  /// Read access to a node for inspection surfaces (the dev-server tree
  /// query reads current property values through this). Mutation stays
  /// behind edit/try_edit, which own damage tracking.
  pub fn try_node(&self, id: u64) -> Option<&Element> {
    self.nodes.get(&id)
  }


  pub(crate) fn node_mut(&mut self, id: u64) -> &mut Element {
    // unwrap_or_else keeps the hot path allocation-free: expect(&format!(..))
    // would build the message string on every call, hit or miss.
    self.nodes.get_mut(&id).unwrap_or_else(|| panic!("node {} not found", id))
  }

  fn delete_recursive(&mut self, node_id: u64) {
    let child_ids: Vec<u64> = self.nodes.get(&node_id).map(|e| e.children.clone()).unwrap_or_default();
    for child_id in child_ids {
      self.delete_recursive(child_id);
    }
    if let Some(element) = self.nodes.remove(&node_id) {
      self.texture_referencers.remove(&node_id);
      if let Some(id) = element.snapshot_texture_id.get() {
        self.released_snapshot_textures.borrow_mut().push(id);
      }
    }
  }

  /// The texture id of `node_id`'s snapshot rasterization, allocated by
  /// `alloy` on the first call and stable for the node's lifetime. Errs when
  /// the node is not a snapshot repaint boundary: only a boundary retains
  /// pixels to vend.
  pub fn snapshot_texture(&self, node_id: u64, alloy: &crate::Context) -> Result<u64, String> {
    let element = self.try_node(node_id).ok_or_else(|| format!("node {node_id} not found"))?;
    if !matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa) {
      return Err(format!("node {node_id} is not a snapshot repaint boundary"));
    }
    if let Some(id) = element.snapshot_texture_id.get() {
      return Ok(id);
    }
    let id = alloy.borrow_texture_id();
    element.snapshot_texture_id.set(Some(id));
    // A boundary that already baked publishes its current pixels now; the
    // paint walk only re-publishes after a rasterization, which a static
    // subtree may never trigger again.
    if let Some(PaintCache::Snapshot(snap)) = &*element.paint_cache.borrow() {
      if snap.valid {
        let outset = snap.shaded.as_ref().map_or(0.0, |sc| sc.outset);
        let tex_w = ((snap.width + 2.0 * outset) * snap.scale).ceil() as u32;
        let tex_h = ((snap.height + 2.0 * outset) * snap.scale).ceil() as u32;
        alloy.publish_snapshot_texture(id, &snap.texture, tex_w, tex_h);
      }
    }
    Ok(id)
  }

  /// Drain the vended snapshot texture ids whose boundary was deleted since
  /// the last drain; the frame loop releases them (`Context::release_borrowed`).
  pub fn take_released_snapshot_textures(&self) -> Vec<u64> {
    std::mem::take(&mut *self.released_snapshot_textures.borrow_mut())
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
          float: node.float,
          clear: node.clear,
        });
      }
      return;
    };
    let overrides = inherited.layer(&span.overrides);
    if !span.text.is_empty() {
      text.push_str(&span.text);
      runs.push(TextRun {
        text: span.text.clone(),
        overrides: overrides.clone(),
        node: id,
        atom: None,
        float: None,
        clear: None,
      });
    }
    for &child_id in &node.children {
      self.collect_runs(child_id, &overrides, text, runs);
    }
  }

  /// If `node_id` is a Span, or an inline atom (a laid-out child of a Text),
  /// resync the Text that owns it. Called after a property write; no-op for
  /// everything else, so callers need not check.
  pub fn sync_span_parent(&mut self, node_id: u64) {
    let Some(node) = self.try_node(node_id) else {
      return;
    };
    if matches!(node.kind, ElementKind::Span(_)) {
      self.sync_text(node_id);
    } else if let (true, Some(parent)) = (node.has_layout(), node.parent) {
      if matches!(self.try_node(parent).map(|n| &n.kind), Some(ElementKind::Text(_))) {
        self.sync_text(parent);
      }
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
    // Every referencer displaying a changed id shows new pixels at its
    // rect, tree damage or not - partial repaint needs those rects even on
    // the present-only reuse path.
    let mut content_hits: Vec<u64> = Vec::new();
    for &id in &self.texture_referencers {
      let Some(element) = self.nodes.get(&id) else { continue };
      match &element.kind {
        ElementKind::Texture(t) => {
          if let Some(tex) = t.texture_id.filter(|t| ids.contains(t)) {
            content_hits.push(id);
            if self.under_snapshot_boundary(id, tex) {
              bake_hits.push(id);
            }
          }
        }
        // The shader only runs on the view's OWN snapshot boundary (declared
        // without one it is ignored with a warning), so ancestor boundaries
        // are irrelevant here.
        ElementKind::View(v) => {
          if matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa)
            && v.shader.as_ref().is_some_and(|s| s.textures.iter().any(|b| ids.contains(&b.id)))
          {
            shader_hits.push(id);
          }
        }
        _ => {}
      }
    }
    for id in &content_hits {
      self.note_damage(*id);
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
  /// A boundary whose own vended texture (`snapshot_texture`) is the changed
  /// id is excluded: its rasterization IS the change, and invalidating it
  /// would re-rasterize every frame.
  fn under_snapshot_boundary(&self, node_id: u64, texture_id: u64) -> bool {
    let mut current = Some(node_id);
    while let Some(id) = current {
      let Some(element) = self.try_node(id) else {
        return false;
      };
      if matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa) {
        return element.snapshot_texture_id.get() != Some(texture_id);
      }
      current = element.parent;
    }
    false
  }

  /// Texture registry ids referenced by any live element, attached or
  /// detached (a detached node can be re-inserted, so its reference counts):
  /// texture elements' sources and boundary shaders' extra sampler inputs.
  /// Used by the deferred-destroy sweep to decide which pending ids are
  /// reclaimable; only called while destroys are pending, so it stays off
  /// the per-frame path.
  pub fn referenced_texture_ids(&self) -> HashSet<u64> {
    let mut ids = HashSet::new();
    for element in self.texture_referencers.iter().filter_map(|id| self.nodes.get(id)) {
      match &element.kind {
        ElementKind::Texture(t) => {
          if let Some(id) = t.texture_id {
            ids.insert(id);
          }
        }
        ElementKind::View(v) => {
          if let Some(shader) = &v.shader {
            ids.extend(shader.textures.iter().map(|b| b.id));
          }
        }
        _ => {}
      }
    }
    ids
  }

}
