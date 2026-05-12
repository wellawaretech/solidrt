use std::collections::HashMap;
use taffy::NodeId;

use crate::rendertree::Element;

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

  fn invalidate_cache(&mut self, node_id: u64) {
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
