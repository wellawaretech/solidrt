use std::collections::HashMap;

use taffy::{NodeId, Size};

use crate::rendertree::{BoundaryMode, BoundingBox, Damage, Element, ElementKind};

pub struct RenderTree {
  nodes: HashMap<u64, Element>,
  pub root: Option<u64>,
  // Content revision: bumped on structural changes and on element_write (the
  // property-mutation surface). Internal layout writes (node_mut) do not
  // count. Lets a painter detect "tree unchanged since last build" and reuse
  // its display list.
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

  /// Mutable element access for a property write. Bumps the revision but
  /// invalidates nothing: the caller must follow up with `apply_damage`,
  /// passing what the setter reported.
  pub fn element_write(&mut self, id: u64) -> &mut Element {
    self.bump_revision();
    self.node_mut(id)
  }

  /// Completes a property write by invalidating what the setter reported (see
  /// `Damage`). Transform-only writes keep the node's own paint cache - its
  /// matrix is applied at composite time - and clear from the parent up, since
  /// ancestor recordings hold the node at its old placement.
  pub fn apply_damage(&mut self, node_id: u64, damage: Damage) {
    match damage {
      Damage::None => {}
      Damage::Transform => {
        if let Some(parent) = self.try_node(node_id).and_then(|e| e.parent) {
          self.invalidate_paint(parent);
        }
      }
      Damage::Scroll => {
        // Like Transform on a Recording boundary (its cache holds children
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

  /// Clear cached boundary recordings from `node_id` up to the root. A content
  /// or layout change invalidates the enclosing boundary and - because
  /// draw_display_list copies commands into the enclosing recording - every
  /// boundary above it as well.
  pub fn invalidate_paint(&self, node_id: u64) {
    let mut current = Some(node_id);
    while let Some(id) = current {
      let Some(element) = self.try_node(id) else {
        break;
      };
      element.paint_cache.borrow_mut().take();
      current = element.parent;
    }
  }

  pub(crate) fn node(&self, id: u64) -> &Element {
    self.nodes.get(&id).expect(&format!("node {} not found", id))
  }

  pub(crate) fn try_node(&self, id: u64) -> Option<&Element> {
    self.nodes.get(&id)
  }

  /// Bounding box of a node relative to its nearest positioning context: the
  /// closest ancestor whose JSX explicitly set `position="relative"`. Falls
  /// back to the window when there is none. This is the frame an absolutely
  /// positioned sibling overlay is drawn in, so coordinates from here can feed
  /// directly into such an overlay. Detached nodes report the box inherited from
  /// their nearest laid-out ancestor. Returns None before the first layout.
  pub fn bounding_box(&self, id: u64) -> Option<BoundingBox> {
    self.compute_bounding_box(id, true)
  }

  /// Bounding box of a node relative to the window root (CSS getBoundingClientRect
  /// semantics), for callers that want absolute coordinates (e.g. snapshot).
  pub fn bounding_box_viewport(&self, id: u64) -> Option<BoundingBox> {
    self.compute_bounding_box(id, false)
  }

  /// Computed lazily: walks from the node upward each call, so nothing is cached
  /// and only queried nodes cost anything. Call after layout_phase (e.g. from
  /// the postLayout hook) for current-frame values. When `stop_at_context` is
  /// set the ascent stops at (and does not fold in) the first positioning
  /// context ancestor, yielding coordinates in that ancestor's frame; otherwise
  /// it continues to the root.
  ///
  /// Phase 1: only translations compose into the result - each ancestor's
  /// layout position, plus View `pos` (forward) and `scroll` (inverse). A View
  /// `rotate` or `scale` anywhere in the chain is ignored, so the reported x/y
  /// is wrong under rotation/scaling. Size and local offset come from the kind's
  /// `local_bounds`. TODO: compose full transforms by walking the four corners
  /// up through each ancestor, mirroring hit testing.
  fn compute_bounding_box(&self, id: u64, stop_at_context: bool) -> Option<BoundingBox> {
    let node = self.try_node(id)?;
    let local = node.kind.local_bounds(self.content_fallback(id)?);
    let width = local.width;
    let height = local.height;
    let mut x = local.x;
    let mut y = local.y;

    // Detached nodes have no layout placement; they inherit position from the
    // ancestor walk below.
    if let Some(layout) = node.layout.as_ref() {
      x += layout.computed.location.x;
      y += layout.computed.location.y;
    }

    // Ascend, adding each ancestor's layout position and View translate, and
    // removing any scroll the ancestor applies to its children. For the
    // container-relative box, stop before folding in the first positioning
    // context: the result is then expressed in that ancestor's frame. Absolute
    // ancestors are deliberately transparent here - their offset is still
    // accumulated, they just never act as the stop.
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
      if let Some(parent_layout) = parent.layout.as_ref() {
        x += parent_layout.computed.location.x;
        y += parent_layout.computed.location.y;
      }
      if let ElementKind::View(v) = &parent.kind {
        if let Some(p) = v.translate {
          x += p.x;
          y += p.y;
        }
        if let Some(s) = v.scroll {
          x -= s.x;
          y -= s.y;
        }
      }
      cur_id = parent_id;
    }

    Some(BoundingBox { x, y, width, height })
  }

  /// Fallback size for shapes without explicit w/h: the nearest laid-out node's
  /// box (self, or the ancestor a detached subtree hangs from). None before the
  /// first layout has populated the cache.
  fn content_fallback(&self, id: u64) -> Option<Size<f32>> {
    let mut cur = id;
    loop {
      let node = self.try_node(cur)?;
      if let Some(layout) = node.layout.as_ref() {
        if layout.cache.is_empty() {
          return None;
        }
        return Some(layout.computed.size);
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

  /// Rebuild a Text's computed_text from its Span children; no-op for other
  /// kinds. The layout pass aggregates spans for attached text on every pass,
  /// but detached text never enters layout, so structural and span-text
  /// changes sync eagerly here instead.
  fn sync_text(&mut self, text_id: u64) {
    let Some(element) = self.try_node(text_id) else { return };
    if !matches!(element.kind, ElementKind::Text(_)) {
      return;
    }
    let mut text = String::new();
    for &child_id in &element.children {
      if let ElementKind::Span(span) = &self.node(child_id).kind {
        text.push_str(&span.text);
      }
    }
    if let ElementKind::Text(t) = &mut self.node_mut(text_id).kind {
      t.computed_text = text;
    }
  }

  /// If `node_id` is a Span, resync the parent Text. Called after a property
  /// write; no-op for every other kind, so callers need not check.
  pub fn sync_span_parent(&mut self, node_id: u64) {
    let Some(node) = self.try_node(node_id) else { return };
    if !matches!(node.kind, ElementKind::Span(_)) {
      return;
    }
    if let Some(parent_id) = node.parent {
      self.sync_text(parent_id);
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
      current = element.parent;
    }
  }

  /// Plain-data copy of the whole tree for external inspection (debug and dev
  /// tooling). Engine-free: the caller decides how to encode it. Boxes are
  /// window-relative (see bounding_box_viewport) and zero before the first
  /// layout.
  pub fn snapshot(&self) -> Option<NodeSnapshot> {
    self.root.and_then(|id| self.snapshot_node(id))
  }

  fn snapshot_node(&self, id: u64) -> Option<NodeSnapshot> {
    let node = self.try_node(id)?;
    let bounds = self.bounding_box_viewport(id).unwrap_or(BoundingBox { x: 0.0, y: 0.0, width: 0.0, height: 0.0 });
    let text = match &node.kind {
      ElementKind::Text(t) => Some(t.computed_text.clone()),
      ElementKind::Span(s) => Some(s.text.clone()),
      _ => None,
    };
    Some(NodeSnapshot {
      id,
      kind: node.kind.name(),
      detached: !node.has_layout(),
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
      text,
      children: node.children.iter().filter_map(|&child| self.snapshot_node(child)).collect(),
    })
  }
}

/// One node of a RenderTree::snapshot: kind, window-relative box, text content
/// and children.
pub struct NodeSnapshot {
  pub id: u64,
  pub kind: &'static str,
  pub detached: bool,
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
  pub text: Option<String>,
  pub children: Vec<NodeSnapshot>,
}
