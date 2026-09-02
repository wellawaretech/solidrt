//! Inspection surfaces: node and mounted counts, and the snapshot tree the
//! dev tooling reads (full snapshots, rooted/depth-limited ones, and text
//! matches with their paths).

use super::RenderTree;
use crate::rendertree::{ElementKind, Rect};

impl RenderTree {
  /// Number of live nodes (attached and detached), for the stats counters.
  pub fn node_count(&self) -> usize {
    self.nodes.len()
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
