//! Inspection surfaces: node and mounted counts, the snapshot tree the dev
//! tooling reads (full snapshots, rooted/depth-limited ones, and text
//! matches with their paths), and what a test reads a node by: `find`,
//! `is_visible` and `hit_path`.

use taffy::style::Overflow;

use super::RenderTree;
use crate::rendertree::hit::{DefaultHitTester, HitTester};
use crate::rendertree::{ElementKind, Point, Rect};

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
  /// for nodes whose kind equals `query` or whose text or label contains
  /// it, all case-insensitive. Matches are childless snapshots paired with the id
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
    let label_hit = node.label.as_ref().is_some_and(|label| label.to_lowercase().contains(needle));
    if kind_hit || text_hit || label_hit {
      if let Some(snapshot) = self.snapshot_node(id, 0) {
        out.push(NodeMatch { path: path.clone(), node: snapshot });
      }
    }
    for &child in &node.children {
      self.collect_matches(child, needle, limit, path, out);
    }
    path.pop();
  }

  /// The text a node shows, for the kinds that are text.
  fn node_text(&self, id: u64) -> Option<&str> {
    match &self.try_node(id)?.kind {
      ElementKind::Text(t) => Some(&t.computed_text),
      ElementKind::Span(s) => Some(&s.text),
      _ => None,
    }
  }

  /// Depth-first search of the subtree under `root` (the tree root when
  /// None, `root` itself included) for the nodes `query` matches: exact and
  /// case-sensitive, unlike the loose `snapshot_matches` a person types
  /// into. A node whose text matches under an ancestor showing the very
  /// same text is left out (a span that is all of its text: one visible
  /// text, one match). Matches are childless snapshots with their id path,
  /// capped at `limit`. None when the tree is empty or `root` is not a
  /// current node.
  pub fn find(&self, root: Option<u64>, query: &NodeQuery, limit: usize) -> Option<Vec<NodeMatch>> {
    let start = root.or(self.root)?;
    self.try_node(start)?;
    let mut matches = Vec::new();
    let mut path = Vec::new();
    self.collect_found(start, query, limit, None, &mut path, &mut matches);
    Some(matches)
  }

  fn collect_found(
    &self,
    id: u64,
    query: &NodeQuery,
    limit: usize,
    text_above: Option<&str>,
    path: &mut Vec<u64>,
    out: &mut Vec<NodeMatch>,
  ) {
    if out.len() >= limit {
      return;
    }
    let Some(node) = self.try_node(id) else { return };
    path.push(id);
    let text = self.node_text(id);
    let repeats = text.is_some() && text == text_above;
    let hit = query.kind.is_none_or(|kind| node.kind.name() == kind)
      && query.text.holds(text)
      && query.label.holds(node.label.as_deref())
      && !(repeats && query.text != Match::Any);
    if hit {
      if let Some(snapshot) = self.snapshot_node(id, 0) {
        out.push(NodeMatch { path: path.clone(), node: snapshot });
      }
    }
    for &child in &node.children {
      self.collect_found(child, query, limit, text.or(text_above), path, out);
    }
    path.pop();
  }

  /// Whether the node is painted where a user could see it: mounted and
  /// not on its way out, its painted box still has area once every
  /// clipping ancestor (an overflow other than visible, per axis) and the
  /// window have cut it, and the opacity multiplied up its ancestor chain
  /// is above 0. It says nothing about what is painted over it: that is a
  /// hit test's answer (`hit_path`). Boxes are the axis-aligned bounds of
  /// the painted quads, so under a rotated clipping ancestor the cut is
  /// the bounds' and not the quad's.
  pub fn is_visible(&self, id: u64) -> bool {
    let Some(root) = self.root else { return false };
    let Some(mut visible) = self.bounding_box_viewport(id) else { return false };
    let mut opacity = 1.0;
    let mut at = id;
    loop {
      let Some(node) = self.try_node(at) else { return false };
      if node.lifecycle.exiting {
        return false;
      }
      if let ElementKind::View(view) = &node.kind {
        opacity *= view.opacity.unwrap_or(1.0);
      }
      // A node does not clip its own box, only what is inside it; the
      // window clips everything.
      if at != id {
        let overflow = node.layout.as_ref().map(|l| (l.style.overflow.x, l.style.overflow.y));
        let (clip_x, clip_y) = match overflow {
          Some((x, y)) => (x != Overflow::Visible || at == root, y != Overflow::Visible || at == root),
          None => (at == root, at == root),
        };
        if clip_x || clip_y {
          let Some(clip) = self.bounding_box_viewport(at) else { return false };
          visible = cut(visible, clip, clip_x, clip_y);
        }
      }
      match node.parent {
        Some(parent) => at = parent,
        // Off the mounted tree: nothing of it is painted.
        None if at != root => return false,
        None => break,
      }
    }
    opacity > 0.0 && visible.size.width > 0.0 && visible.size.height > 0.0
  }

  /// The ids from the root of `id`'s tree down to `id` (inclusive), along
  /// the parent links; None when `id` is not a current node.
  pub fn path_to(&self, id: u64) -> Option<Vec<u64>> {
    self.try_node(id)?;
    let mut path = vec![id];
    while let Some(parent) = self.try_node(path[path.len() - 1]).and_then(|node| node.parent) {
      path.push(parent);
    }
    path.reverse();
    Some(path)
  }

  /// The nodes a pointer at `point` (window coordinates) reaches, root
  /// first, the node it lands on last: what the input router dispatches
  /// along. Empty when nothing is hit.
  pub fn hit_path(&self, point: Point) -> Vec<u64> {
    DefaultHitTester.hit_test(self, point).into_iter().map(|(id, _, _)| id).collect()
  }

  fn snapshot_node(&self, id: u64, depth: usize) -> Option<NodeSnapshot> {
    let node = self.try_node(id)?;
    let bounds = self.bounding_box_viewport(id).unwrap_or(Rect::zero());
    let text = self.node_text(id).map(str::to_string);
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
      label: node.label.clone(),
      child_count: node.children.len(),
      children,
    })
  }
}

/// `rect` cut to `clip` on the axes named; an axis left alone keeps its
/// extent. A rect cut away entirely comes back with no size on that axis.
fn cut(rect: Rect, clip: Rect, x: bool, y: bool) -> Rect {
  let span = |from: f32, len: f32, clip_from: f32, clip_len: f32| {
    let start = from.max(clip_from);
    let end = (from + len).min(clip_from + clip_len);
    (start, (end - start).max(0.0))
  };
  let mut out = rect;
  if x {
    (out.origin.x, out.size.width) = span(rect.origin.x, rect.size.width, clip.origin.x, clip.size.width);
  }
  if y {
    (out.origin.y, out.size.height) = span(rect.origin.y, rect.size.height, clip.origin.y, clip.size.height);
  }
  out
}

/// How `RenderTree::find` constrains one field of a node.
#[derive(Clone, Copy, PartialEq)]
pub enum Match<'a> {
  /// No constraint.
  Any,
  /// The node has the field, whatever it holds: the candidates for a
  /// pattern the caller matches itself.
  Present,
  /// The field is exactly this.
  Exact(&'a str),
}

impl Match<'_> {
  fn holds(&self, value: Option<&str>) -> bool {
    match self {
      Match::Any => true,
      Match::Present => value.is_some(),
      Match::Exact(wanted) => value == Some(*wanted),
    }
  }
}

/// What `RenderTree::find` looks for; every constraint has to hold.
pub struct NodeQuery<'a> {
  /// The element kind's name ("view", "text", ...), exact.
  pub kind: Option<&'a str>,
  pub text: Match<'a>,
  pub label: Match<'a>,
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
  pub label: Option<String>,
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
