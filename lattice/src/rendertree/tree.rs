use std::collections::HashMap;
use std::sync::mpsc::Sender;

use alloy::AlloyCommand;
use taffy::NodeId;

use crate::rendertree::layout::properties;
use crate::rendertree::{BoundingBox, Element, ElementKind, PropValue};

pub struct RenderTree {
  nodes: HashMap<u64, Element>,
  pub root: Option<u64>,
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
    }
  }

  pub fn create_node(&mut self, id: u64, element: Element) -> u64 {
    if self.nodes.contains_key(&id) {
      panic!("duplicate node id {}", id);
    }
    self.nodes.insert(id, element);
    id
  }

  pub fn insert_node(&mut self, parent_id: u64, node_id: u64, anchor_id: Option<u64>) {
    let child_has_layout = {
      let child = self.node_mut(node_id);
      child.parent = Some(parent_id);
      child.has_layout()
    };

    let parent = self.node_mut(parent_id);
    parent.children.retain(|&id| id != node_id);
    if let Some(layout) = &mut parent.layout {
      layout
        .layout_children
        .retain(|&id| id != NodeId::from(node_id));
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
            if let Some(pos) = layout
              .layout_children
              .iter()
              .position(|&id| id == anchor_nid)
            {
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

    self.invalidate_cache(parent_id);
  }

  pub fn delete_node(&mut self, parent_id: u64, node_id: u64) {
    let parent = self.node_mut(parent_id);
    parent.children.retain(|&id| id != node_id);
    if let Some(layout) = &mut parent.layout {
      layout
        .layout_children
        .retain(|&id| id != NodeId::from(node_id));
    }
    self.delete_recursive(node_id);
    self.invalidate_cache(parent_id);
  }

  pub fn element_mut(&mut self, id: u64) -> &mut Element {
    self.node_mut(id)
  }

  pub(crate) fn node(&self, id: u64) -> &Element {
    self
      .nodes
      .get(&id)
      .expect(&format!("node {} not found", id))
  }

  pub(crate) fn try_node(&self, id: u64) -> Option<&Element> {
    self.nodes.get(&id)
  }

  /// Applies a single property to a node, dispatching to the element kind, then
  /// its paint state, then its layout style, and invalidating the layout cache
  /// when the change affects layout. Panics if no layer recognizes the property.
  pub fn set_property(
    &mut self,
    node_id: u64,
    property: &str,
    value: &PropValue,
    cmd_tx: &Sender<AlloyCommand>,
  ) {
    let invalidate = {
      let element = self.node_mut(node_id);
      let result = match &mut element.kind {
        ElementKind::Window(win) => win.set_property(property, value, cmd_tx),
        ElementKind::Rectangle(rect) => rect.set_property(property, value),
        ElementKind::Oval(oval) => oval.set_property(property, value),
        ElementKind::Line(line) => line.set_property(property, value),
        ElementKind::Path(path) => path.set_property(property, value),
        ElementKind::Text(text) => text.set_property(property, value),
        ElementKind::Span(span) => span.set_property(property, value),
        ElementKind::View(view) => view.set_property(property, value),
        ElementKind::Texture(tex) => tex.set_property(property, value),
      };
      let result = result
        .or_else(|| element.kind.paint_mut().and_then(|paint| paint.set_property(property, value)));
      let result = result
        .or_else(|| element.style_mut().and_then(|style| properties::set_property(style, property, value)));
      result.unwrap_or_else(|| panic!("unknown property '{property}'"))
    };
    if invalidate {
      self.invalidate_cache(node_id);
    }
  }

  /// Window-relative bounding box of a node from the most recently computed
  /// layout. Returns None for layout-less nodes (d-rect, span) and before the
  /// first layout has run (cache empty).
  ///
  /// Computed lazily: walks from the node up to the root each call, so nothing
  /// is cached and only queried nodes cost anything. Call after layout_phase
  /// (e.g. from the postLayout hook) for current-frame values.
  ///
  /// Phase 1: only translations compose into the result - each ancestor's
  /// layout position, plus View `pos` (forward) and `scroll` (inverse). A View
  /// `rotate` or `scale` anywhere in the chain is ignored, so the reported x/y
  /// is wrong under rotation/scaling; width/height are always the node's own
  /// computed size. TODO: compose full transforms by walking the four corners
  /// up through each ancestor, mirroring hit testing.
  pub fn bounding_box(&self, id: u64) -> Option<BoundingBox> {
    let node = self.try_node(id)?;
    let layout = node.layout.as_ref()?;
    if layout.cache.is_empty() {
      return None;
    }

    let width = layout.computed.size.width;
    let height = layout.computed.size.height;

    // Own location, plus own View pos (which translates the node itself).
    let mut x = layout.computed.location.x;
    let mut y = layout.computed.location.y;
    if let ElementKind::View(v) = &node.kind {
      if let Some(p) = v.pos {
        x += p.x;
        y += p.y;
      }
    }

    // Ascend to the root, adding each ancestor's layout position and View
    // translate, and removing any scroll the ancestor applies to its children.
    let mut cur_id = id;
    loop {
      let Some(parent_id) = self.try_node(cur_id).and_then(|n| n.parent) else {
        break;
      };
      let Some(parent) = self.try_node(parent_id) else {
        break;
      };
      if let Some(parent_layout) = parent.layout.as_ref() {
        x += parent_layout.computed.location.x;
        y += parent_layout.computed.location.y;
      }
      if let ElementKind::View(v) = &parent.kind {
        if let Some(p) = v.pos {
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

  pub(crate) fn node_mut(&mut self, id: u64) -> &mut Element {
    self
      .nodes
      .get_mut(&id)
      .expect(&format!("node {} not found", id))
  }

  fn delete_recursive(&mut self, node_id: u64) {
    let child_ids: Vec<u64> = self
      .nodes
      .get(&node_id)
      .map(|e| e.children.clone())
      .unwrap_or_default();
    for child_id in child_ids {
      self.delete_recursive(child_id);
    }
    self.nodes.remove(&node_id);
  }

  pub fn invalidate_cache(&mut self, node_id: u64) {
    let mut current = Some(node_id);
    while let Some(id) = current {
      let element = self.node_mut(id);
      let Some(layout) = &mut element.layout else {
        current = element.parent;
        continue;
      };
      if layout.cache.is_empty() {
        break;
      }
      layout.cache.clear();
      current = element.parent;
    }
  }
}
