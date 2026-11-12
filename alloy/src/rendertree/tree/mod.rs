mod geometry;
mod inspect;
mod transitions;

pub use inspect::{Match, NodeMatch, NodeQuery, NodeSnapshot};
pub use transitions::SlideRemaining;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use taffy::NodeId;

use crate::gpu::DirtySet;
use crate::rendertree::cull;
use crate::rendertree::damage::DamageLedger;
use crate::rendertree::transitions::Transitions;
use crate::rendertree::{
  BoundaryMode, Damage, Element, ElementKind, FrameDamage, PaintCache, Rect, RunOverrides, Shown, Size, TextRun,
  Texture, ATOM_CHAR,
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
  // Text nodes holding a layer texture (Text::has_layer): the ones the
  // release sweep visits (release_stale_text_layers), so it costs the
  // texts with pixels and not every node.
  text_layers: RefCell<HashSet<u64>>,
  // Texts and recording boundaries that kept their raster under a composite
  // scale that moved this frame and want a look at the next, to tell rest
  // from motion (text::raster_scale): the frame start gives each a Compose
  // damage, caches intact, so the walk reaches it.
  scale_waits: RefCell<HashSet<u64>>,
  // Recording boundaries' frames as the last walk that reached each saw
  // them (cull::RecordingFrame): what content damage under a replayed
  // recording composes its window position from. Written on every visit of
  // a Recording boundary (composite::build_recursive), dropped when the
  // node is destroyed or stops being one.
  recording_frames: RefCell<HashMap<u64, cull::RecordingFrame>>,
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
  // Nodes declaring a `layout` transition whose solved box a layout pass
  // changed, with the box they had (None: the empty box, never laid out or
  // hidden). Filled at the one seam a solved box changes
  // (layout/context.rs set_unrounded_layout), drained by the pass that ran
  // (tree/transitions.rs start_layout_slides).
  reflowed: Vec<(u64, Option<Rect>)>,
  // Nodes whose children need a layout against their painted box: a
  // layout slide changing size is in flight on them, or has just settled
  // (one last run at the solved size lands the children on their solved
  // boxes). The value is the painted size the last run laid them out
  // against, so the second layout_phase of a frame (paint_phase re-runs
  // it) skips an unchanged box; None forces a run (a fresh entry, or a
  // layout invalidation anywhere, which may have re-solved the children).
  // Fed by the slide diff and the advance, drained by layout/context.rs
  // animated_layouts, which re-adds the ones still in flight.
  resizing: HashMap<u64, Option<Size>>,
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
      text_layers: RefCell::new(HashSet::new()),
      scale_waits: RefCell::new(HashSet::new()),
      recording_frames: RefCell::new(HashMap::new()),
      texture_referencers: HashSet::new(),
      damage: DamageLedger::new(),
      reflowed: Vec::new(),
      resizing: HashMap::new(),
    }
  }

  fn reconcile_texture_referencer(&mut self, node_id: u64) {
    if self.nodes.get(&node_id).is_some_and(Element::references_textures) {
      self.texture_referencers.insert(node_id);
    } else {
      self.texture_referencers.remove(&node_id);
    }
  }

  // A node that is no Recording boundary after a write has no frame to
  // keep (`recording_frames`).
  fn reconcile_recording_frame(&mut self, node_id: u64) {
    if !self.nodes.get(&node_id).is_some_and(|e| e.repaint_boundary == BoundaryMode::Recording) {
      self.recording_frames.get_mut().remove(&node_id);
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
      let child = self.node(node_id).kind.name();
      let parent = self.node(parent_id).kind.name();
      return Err(format!(
        "<{child}> cannot be a child of <{parent} detached>: a detached subtree must be entirely detached (write the child as <{child} detached>)"
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
        let pos = match parent.children.iter().position(|&id| id == anchor) {
          Some(pos) => {
            parent.children.insert(pos, node_id);
            pos
          }
          None => {
            parent.children.push(node_id);
            parent.children.len() - 1
          }
        };
        if child_has_layout {
          if let Some(layout) = &mut parent.layout {
            // The anchor may sit outside the layout flow (a d-* element, an
            // exiting node): the layout slot is before the first sibling
            // after it that is in the flow.
            let node_nid = NodeId::from(node_id);
            let layout_pos = parent.children[pos + 1..]
              .iter()
              .find_map(|&id| layout.layout_children.iter().position(|&lid| lid == NodeId::from(id)));
            match layout_pos {
              Some(lpos) => layout.layout_children.insert(lpos, node_nid),
              None => layout.layout_children.push(node_nid),
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

    // The enter pass (`from`) runs at the frame's advance, not here: the
    // config and the mounted values may still be on their way.
    if !self.node(node_id).lifecycle.entered {
      self.transitions.entering.push(node_id);
    }
    Ok(())
  }

  /// Unlinks `node_id` from its parent but keeps the subtree alive, so it can be
  /// re-inserted elsewhere (a move). This mirrors DOM removeChild, which detaches
  /// rather than destroys; the renderer frees the node later via destroy_node if
  /// nothing re-attaches it. See renderer.ts for the deferred-destroy sweep.
  ///
  /// Exit animations hook in here: a subtree that declares `exit` values
  /// anywhere in it stays linked and animates them instead, the removed node
  /// as the exit root (see `begin_exit`). It leaves the layout flow at once
  /// (`pop_from_layout`), so siblings reflow as if it were gone while it is
  /// painted at its last box. If the same tick re-inserts the node - a move
  /// - the exit is abandoned and the unlink happens then, so moves never
  /// play removal animations. The deferred destroy finding the node under an
  /// exit root defers the free to the settle.
  pub fn detach_node(&mut self, parent_id: u64, node_id: u64) {
    if self.begin_exit(parent_id, node_id) {
      return;
    }
    self.detach_node_now(parent_id, node_id);
  }

  /// Takes `node_id` out of `parent_id`'s layout flow while it stays a
  /// painted child: the exit pop-out. Siblings reflow at the next layout;
  /// the node keeps its last computed box, relative to the parent, so it
  /// follows the parent's moves but not the parent's own reflow. A detached
  /// (d-*) child was never in the flow and needs nothing.
  fn pop_from_layout(&mut self, parent_id: u64, node_id: u64) {
    let child_has_layout = self.try_node(node_id).map(|n| n.has_layout()).unwrap_or(false);
    if !child_has_layout {
      return;
    }
    if let Some(layout) = &mut self.node_mut(parent_id).layout {
      layout.layout_children.retain(|&id| id != NodeId::from(node_id));
    }
    self.invalidate_cache(parent_id);
    self.note_damage(parent_id);
    self.invalidate_paint(parent_id);
    self.bump_revision();
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
  /// A node under an exit root - the exiting node itself, or a descendant
  /// of one whose own removal reached the cascade in flight - is not freed
  /// yet: the destroy is remembered (`doomed`) and happens when the cascade
  /// settles, the root freeing its whole subtree. A doomed descendant whose
  /// root turns out to be a move is freed at the abandon instead.
  pub fn destroy_node(&mut self, node_id: u64) {
    if self.exit_root_of(node_id).is_some() {
      if let Some(el) = self.nodes.get_mut(&node_id) {
        el.lifecycle.doomed = true;
      }
      return;
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
    self.reconcile_recording_frame(id);
    self.reconcile_slide(id);
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
    self.reconcile_recording_frame(id);
    self.reconcile_slide(id);
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
  /// ancestors once per frame instead of once per write, and a layout
  /// pass clears what a resize moved once per node (LayoutContext).
  /// Idempotent per node, so stopping at a visited ancestor loses nothing.
  pub(crate) fn invalidate_paint_batched(&self, node_id: u64, visited: &mut HashSet<u64>) {
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
      self.text_layers.borrow_mut().remove(&node_id);
      self.scale_waits.borrow_mut().remove(&node_id);
      self.recording_frames.get_mut().remove(&node_id);
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

  /// A text node was built: whether it holds a layer now. The paint walk
  /// notes every text it builds.
  pub fn note_text_layer(&self, node_id: u64, has_layer: bool) {
    let mut layers = self.text_layers.borrow_mut();
    if has_layer {
      layers.insert(node_id);
    } else {
      layers.remove(&node_id);
    }
  }

  /// A text or a recording boundary kept its raster under a composite scale
  /// that moved this frame and wants a look at the next (see
  /// `take_scale_waits`).
  pub fn note_scale_wait(&self, node_id: u64) {
    self.scale_waits.borrow_mut().insert(node_id);
  }

  /// A Recording boundary was visited by the paint walk: its frame's map to
  /// the window is `frame`, and `enclosing` the recording the walk was
  /// recording when it got there (BuildContext::recording), which this
  /// boundary's replay is part of. The frame it was recorded at stands.
  pub(crate) fn note_recording_frame(
    &self,
    node_id: u64,
    enclosing: Option<(u64, cull::WindowMap)>,
    frame: cull::WindowMap,
  ) {
    let mut frames = self.recording_frames.borrow_mut();
    let recorded = frames.get(&node_id).and_then(|f| f.recorded);
    let entry = cull::RecordingFrame {
      within: enclosing.map(|(id, _)| id),
      to_window: frame,
      to_within: enclosing.and_then(|(_, outer)| cull::map_relative(&frame, &outer)),
      recorded,
    };
    frames.insert(node_id, entry);
  }

  /// The walk that visited Recording boundary `node_id` records it: the
  /// frame noted for this visit is the one its recording is made at.
  pub(crate) fn note_recording_recorded(&self, node_id: u64) {
    if let Some(frame) = self.recording_frames.borrow_mut().get_mut(&node_id) {
      frame.recorded = Some(frame.to_window);
    }
  }

  /// The node whose cell stands for damaged node `id`'s window extent, when
  /// it is not `id` itself: the outermost repaint boundary above it, of
  /// any kind, where a boundary on the way may show its cached content
  /// somewhere else than the cells inside say. A Recording boundary does
  /// where it moved since it recorded or has no frame noted
  /// (`RecordingFrame::moved_since_recorded`); a snapshot boundary keeps no
  /// frame and always counts as moved. A reused cache is not walked, so the
  /// cells inside it keep the walk that made it, while the outermost
  /// boundary of any kind has a parent outside every cache, whose loop
  /// writes its cell on every walk. None under no boundary, or under
  /// Recording boundaries alone that each show their recording where it
  /// was made, and the cells inside are where they show. Asked for damaged
  /// nodes only (composite::damaged_extent).
  pub(crate) fn boundary_stand_in(&self, id: u64) -> Option<u64> {
    let frames = self.recording_frames.borrow();
    let mut outermost = None;
    let mut moved = false;
    let mut current = self.try_node(id).and_then(|e| e.parent);
    while let Some(cur) = current {
      let Some(element) = self.try_node(cur) else { break };
      match element.repaint_boundary {
        BoundaryMode::Recording => {
          outermost = Some(cur);
          moved |= frames.get(&cur).is_none_or(cull::RecordingFrame::moved_since_recorded);
        }
        BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa => {
          outermost = Some(cur);
          moved = true;
        }
        BoundaryMode::None => {}
      }
      current = element.parent;
    }
    outermost.filter(|_| moved)
  }

  /// A walk entered Recording boundary `node_id`'s replayed subtree without
  /// recording it (a capture's walk) and wrote the cells inside at the
  /// boundary's current frame: the recorded frame no longer says where
  /// they are, so the boundary counts as moved until it records again.
  pub(crate) fn note_recording_walked(&self, node_id: u64) {
    if let Some(frame) = self.recording_frames.borrow_mut().get_mut(&node_id) {
      frame.recorded = None;
    }
  }

  /// The nodes waiting for the next frame's look, taken once per frame by
  /// the frame producer, which gives each a Compose damage.
  pub fn take_scale_waits(&self) -> Vec<u64> {
    self.scale_waits.borrow_mut().drain().collect()
  }

  /// The text nodes holding a layer.
  pub fn text_layer_nodes(&self) -> Vec<u64> {
    self.text_layers.borrow().iter().copied().collect()
  }

  /// Drop every retained paint result and text layer: they are textures
  /// and display lists of a GL context that is gone (the context-loss
  /// restore, okf/done/gpu-context-loss.md). Snapshot caches go whole,
  /// allocation included (a stale snapshot otherwise keeps its texture for
  /// the next raster to reuse), and the revision moves so the next frame
  /// rebuilds rather than re-presenting the retained display list. The
  /// vended snapshot texture ids stand: the next raster publishes the new
  /// backing under them.
  pub fn drop_paint_caches(&mut self) {
    for element in self.nodes.values() {
      element.paint_cache.borrow_mut().take();
      element.envelope.clear();
      if let ElementKind::Text(text) = &element.kind {
        text.drop_layer();
      }
    }
    self.text_layers.borrow_mut().clear();
    self.bump_revision();
  }

  /// Release the layer textures of texts that went unbuilt long enough
  /// (Text::release_stale_layer) as of text atlas frame `frame`: the walk
  /// never enters a culled, hidden or scrolled-out text, so its stamp ages
  /// on its own. Called once per paint walk, after it.
  pub fn release_stale_text_layers(&self, frame: u64) {
    let mut layers = self.text_layers.borrow_mut();
    layers.retain(|&id| match self.nodes.get(&id).map(|e| &e.kind) {
      Some(ElementKind::Text(text)) => text.release_stale_layer(frame),
      _ => false,
    });
  }

  /// Drain the vended snapshot texture ids whose boundary was deleted since
  /// the last drain; the frame loop releases them (`Context::release_borrowed`).
  pub fn take_released_snapshot_textures(&self) -> Vec<u64> {
    std::mem::take(&mut *self.released_snapshot_textures.borrow_mut())
  }

  /// Drain every snapshot texture id the tree still holds: the queued
  /// releases plus the live boundaries' own, which leave their elements. For
  /// whoever drops the tree whole (an engine teardown), which destroys no
  /// node and so queues nothing; the ids go the same release path.
  pub fn drain_snapshot_textures(&mut self) -> Vec<u64> {
    let mut ids = self.take_released_snapshot_textures();
    ids.extend(self.nodes.values().filter_map(|element| element.snapshot_texture_id.take()));
    ids
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
    // The pass may re-solve a resizing node's children: lay them out
    // against the painted box again whatever its size.
    for last in self.resizing.values_mut() {
      *last = None;
    }
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
  /// (Context::take_content_changes is the source), each in full or inside
  /// rectangles of its own pixels. Live texture references need nothing to
  /// show it: the resubmitted display list samples the fresh pixels at the
  /// raster flush. Two things follow from it.
  ///
  /// Window damage, for partial repaint: every texture element showing a
  /// changed id shows new pixels, tree damage or not, on the present-only
  /// reuse path too. A full change damages the node. A change inside
  /// rectangles damages only what the node shows of them
  /// (`content_extent`), and the node where that is not known. Under
  /// Recording boundaries alone the same holds where the node shows now,
  /// composed up the boundaries' frames (`content_reach`), and a full change
  /// damages the node's drawn box there, not the boundary. Under a snapshot
  /// boundary, or under a view whose filter blurs, or where a map on the
  /// way is not known, the node is the outermost boundary or blurring view
  /// above it, damaged whole (`content_reach` says why).
  ///
  /// Baked pixels, the one consumer that goes stale. Two reference shapes,
  /// two costs: a texture element under a snapshot boundary is IN the bake
  /// and re-rasterizes (`invalidate_paint`); a boundary shader sampling the
  /// id as an extra input only needs its pass re-run over the still-valid
  /// snapshot, so it gets the params-write treatment (`mark_shader_dirty` +
  /// Compose damage: ancestor recordings repaint, the bake survives). Bumps
  /// the revision only on such a hit: a frame whose GPU writes touch no
  /// snapshot boundary keeps the present-only reuse path. Returns whether
  /// anything was invalidated.
  pub fn texture_content_changed(&mut self, changes: &DirtySet) -> bool {
    if changes.is_empty() {
      return false;
    }
    let mut bake_hits: Vec<u64> = Vec::new();
    let mut shader_hits: Vec<u64> = Vec::new();
    // Per node showing a changed id: the node whose extent stands for its
    // pixels, with the window extent the change stays inside; None is that
    // whole node.
    let mut content_hits: Vec<(u64, Option<cull::Extent>)> = Vec::new();
    for &id in &self.texture_referencers {
      let Some(element) = self.nodes.get(&id) else { continue };
      match &element.kind {
        ElementKind::Texture(t) => {
          if let Some(tex) = t.texture_id.filter(|t| changes.contains(t)) {
            let reach = self.content_reach(id, t, tex);
            let shown = match (changes.rects(&tex), reach.placed) {
              (Some(rects), Some((shown, map))) => shown.window_extent(&map, rects.rects()),
              (None, Some((shown, map))) if reach.bounded => Some(shown.drawn_extent(&map)),
              _ => None,
            };
            let extent = shown.and_then(|shown| self.content_extent(reach.node, shown));
            content_hits.push((reach.node, extent));
            if reach.baked {
              bake_hits.push(id);
            }
          }
        }
        // The shader only runs on the view's OWN snapshot boundary (declared
        // without one it is ignored with a warning), so ancestor boundaries
        // are irrelevant here.
        ElementKind::View(v) => {
          if matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa)
            && v.shader.as_ref().is_some_and(|s| s.textures.iter().any(|b| changes.contains(&b.id)))
          {
            shader_hits.push(id);
          }
        }
        _ => {}
      }
    }
    for (node, extent) in content_hits {
      match extent {
        Some(extent) => self.damage.note_area(extent),
        None => self.note_damage(node),
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

  // One walk up from texture node `node_id` (`texture`), showing
  // `texture_id`, for the facts a content change needs (`ContentReach`).
  //
  // The node whose extent stands for the texture's pixels in a damage
  // resolve is the outermost node on it or above it that is a repaint
  // boundary or a view whose filter blurs (`blurs_subtree`); the node
  // itself under none. That extent is current: the outermost boundary's
  // own is written by its parent's loop on every walk, where a replayed
  // recording is not walked and the extents under it are those of the
  // walk that recorded it.
  //
  // Where the texture shows now, when that is known: the box frame's map
  // to the window. Under no boundary, the one the last walk built it with
  // (`Shown`). Under Recording boundaries alone, composed up their frames
  // (cull::RecordingFrame): the build's map to the frame of the nearest
  // boundary, each boundary's map to the next one's, then the outermost
  // one's to the window, every boundary frame as the last walk reaching it
  // wrote it. That holds wherever the boundaries moved or scrolled since:
  // a recording is fixed inside its frame, and its replay draws it there.
  // Each link must name the boundary the walk finds next; a link to
  // another (a capture's walk built the node outside the recording that
  // replays, a boundary came or went) breaks the chain, and the outermost
  // node is damaged whole. So does a snapshot boundary on the way (its
  // raster is resampled, maybe shaded, at composite) and a blur (it
  // spreads each changed pixel of its subtree by its reach, and only the
  // filtered view's own extent is grown by it, cull::envelope).
  //
  // Baked is the nearest snapshot boundary's word: boundary MODE, not cache
  // presence (a declared boundary that has not baked yet has nothing
  // stale, the invalidation is a no-op then, and mode is plain element
  // state). A boundary whose own vended texture (`snapshot_texture`) is
  // the changed id does not count: its rasterization IS the change, and
  // invalidating it would re-rasterize every frame.
  fn content_reach(&self, node_id: u64, texture: &Texture, texture_id: u64) -> ContentReach {
    let shown = texture.shown.get();
    let mut reach = ContentReach { node: node_id, baked: false, placed: None, bounded: false };
    let mut snapshot_seen = false;
    // A snapshot or a blur on the way, or a broken chain: the map is not
    // known.
    let mut lost = shown.is_none();
    // The chain so far: the box frame's map to the frame of `within`, the
    // boundary the walk must meet next, and to the window once it is None.
    let mut map = shown.and_then(|s| s.to_frame);
    let mut within = shown.and_then(|s| s.within);
    let frames = self.recording_frames.borrow();
    let mut current = Some(node_id);
    while let Some(id) = current {
      let Some(element) = self.try_node(id) else { break };
      let blurs = blurs_subtree(element);
      if !matches!(element.repaint_boundary, BoundaryMode::None) || blurs {
        reach.node = id;
      }
      match element.repaint_boundary {
        BoundaryMode::Recording => {
          reach.bounded = true;
          match frames.get(&id) {
            Some(frame) if !lost && within == Some(id) => {
              // On to the next boundary's frame, or to the window from
              // the outermost one.
              let next = if frame.within.is_some() { frame.to_within } else { frame.to_window };
              map = map.zip(next).map(|(m, f)| m.then(&f));
              within = frame.within;
            }
            _ => lost = true,
          }
        }
        BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa => {
          lost = true;
          if !snapshot_seen {
            snapshot_seen = true;
            reach.baked = element.snapshot_texture_id.get() != Some(texture_id);
          }
        }
        BoundaryMode::None => {}
      }
      lost |= blurs;
      current = element.parent;
    }
    if !lost && within.is_none() {
      reach.placed = shown.zip(map);
    }
    reach
  }

  // `shown`, the window extent of what texture node `node_id`'s draw shows
  // of a change, cut to the extent of `reach_node`, the node that stands for
  // its pixels (`content_reach`): the node itself, or the outermost
  // Recording boundary above it, whose extent the last walk wrote. None
  // where that extent is not bounded, and the whole node is damaged.
  fn content_extent(&self, reach_node: u64, shown: cull::Extent) -> Option<cull::Extent> {
    let cull::Extent::Bounded(own) = self.try_node(reach_node)?.last_extent.get() else {
      return None;
    };
    match shown {
      cull::Extent::Bounded(r) => Some(r.intersection(&own).map_or(cull::Extent::Empty, cull::Extent::Bounded)),
      extent => Some(extent),
    }
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

// What one walk up from a texture node finds (`RenderTree::content_reach`):
// the node whose extent stands for the texture's pixels, whether a
// snapshot boundary bakes them, where the node shows now when that is
// known (its draw, and its box frame's map to the window), and whether a
// Recording boundary is on the way.
struct ContentReach {
  node: u64,
  baked: bool,
  placed: Option<(Shown, euclid::default::Transform2D<f32>)>,
  bounded: bool,
}

// Whether a view's filter paints past the subtree it filters: a blur with
// a reach. The color operations map each pixel onto itself, so a filter
// of only those keeps every changed pixel where it was.
fn blurs_subtree(element: &Element) -> bool {
  matches!(&element.kind, ElementKind::View(v) if v.active_filter().is_some_and(|f| f.blur_outset() > 0.0))
}
