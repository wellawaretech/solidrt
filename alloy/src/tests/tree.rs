use taffy::NodeId;

use crate::rendertree::*;

// A node that participates in layout.
fn attached() -> Element {
  View::default().with_layout()
}

// A detached (no-layout) node.
fn detached() -> Element {
  View::default().no_layout()
}

#[test]
fn create_node_stores_element() {
  let mut tree = RenderTree::new();
  let id = tree.create_node(1, attached());
  assert_eq!(id, 1);
  assert!(tree.try_node(1).is_some());
}

#[test]
#[should_panic(expected = "duplicate node id")]
fn create_node_duplicate_id_panics() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(1, attached());
}

#[test]
fn insert_node_links_parent_and_children() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.insert_node(1, 2, None);

  assert_eq!(tree.node(2).parent, Some(1));
  assert_eq!(tree.node(1).children, vec![2]);
  assert_eq!(tree.node(1).layout_data().layout_children, vec![NodeId::from(2u64)]);
}

#[test]
fn insert_node_with_anchor_inserts_before() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 3, Some(2));

  assert_eq!(tree.node(1).children, vec![3, 2]);
  assert_eq!(tree.node(1).layout_data().layout_children, vec![NodeId::from(3u64), NodeId::from(2u64)]);
}

#[test]
fn insert_node_with_unknown_anchor_appends() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 3, Some(99));

  assert_eq!(tree.node(1).children, vec![2, 3]);
}

#[test]
fn reinserting_child_does_not_duplicate() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 2, None);

  assert_eq!(tree.node(1).children, vec![2]);
  assert_eq!(tree.node(1).layout_data().layout_children, vec![NodeId::from(2u64)]);
}

#[test]
#[should_panic(expected = "detached subtrees must be entirely detached")]
fn attached_under_detached_panics() {
  let mut tree = RenderTree::new();
  tree.create_node(1, detached());
  tree.create_node(2, attached());
  tree.insert_node(1, 2, None);
}

#[test]
fn detached_child_not_in_layout_children() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, detached());
  tree.insert_node(1, 2, None);

  assert_eq!(tree.node(1).children, vec![2]);
  assert!(tree.node(1).layout_data().layout_children.is_empty());
}

#[test]
fn detach_node_keeps_subtree_alive() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);

  tree.detach_node(1, 2);

  // Unlinked from parent, but the subtree survives for re-insertion.
  assert_eq!(tree.node(1).children, Vec::<u64>::new());
  assert!(tree.node(1).layout_data().layout_children.is_empty());
  assert!(tree.node(2).parent.is_none());
  assert!(tree.try_node(2).is_some());
  assert!(tree.try_node(3).is_some());
  assert_eq!(tree.node(2).children, vec![3]);
}

#[test]
fn reinsert_detached_node_moves_it() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 3, None);

  // Move node 3 from under 1 to under 2 without an explicit detach first.
  tree.insert_node(2, 3, None);

  assert_eq!(tree.node(1).children, vec![2]);
  assert_eq!(tree.node(2).children, vec![3]);
  assert_eq!(tree.node(3).parent, Some(2));
  assert!(tree.node(1).layout_data().layout_children.iter().all(|&id| id != taffy::NodeId::from(3u64)));
}

#[test]
fn destroy_node_frees_detached_subtree() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);

  tree.detach_node(1, 2);
  tree.destroy_node(2);

  assert!(tree.try_node(2).is_none());
  assert!(tree.try_node(3).is_none());
}

#[test]
fn delete_node_removes_subtree() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);

  tree.delete_node(1, 2);

  assert_eq!(tree.node(1).children, Vec::<u64>::new());
  assert!(tree.node(1).layout_data().layout_children.is_empty());
  assert!(tree.try_node(2).is_none());
  assert!(tree.try_node(3).is_none());
}

// A text node with laid-out content, for snapshot query matching.
fn text(content: &str) -> Element {
  let mut t = Text::default();
  t.computed_text = content.to_string();
  t.with_layout()
}

#[test]
fn snapshot_from_caps_depth_and_reports_child_count() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);

  let full = tree.snapshot().expect("full snapshot");
  assert_eq!(full.children.len(), 1);
  assert_eq!(full.children[0].children.len(), 1);

  let capped = tree.snapshot_from(None, Some(1)).expect("capped snapshot");
  let child = &capped.children[0];
  assert!(child.children.is_empty());
  assert_eq!(child.child_count, 1);

  let sub = tree.snapshot_from(Some(2), Some(0)).expect("subtree snapshot");
  assert_eq!(sub.id, 2);
  assert!(sub.children.is_empty());
  assert_eq!(sub.child_count, 1);

  assert!(tree.snapshot_from(Some(99), None).is_none());
}

#[test]
fn snapshot_matches_finds_kind_and_text_with_paths() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, text("Alpha Heading"));
  tree.create_node(3, attached());
  tree.create_node(4, text("beta"));
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 3, None);
  tree.insert_node(3, 4, None);
  tree.root = Some(1);

  // Case-insensitive text substring, path from the root down to the match.
  let hits = tree.snapshot_matches(None, "alpha", 100).expect("text matches");
  assert_eq!(hits.len(), 1);
  assert_eq!(hits[0].node.id, 2);
  assert_eq!(hits[0].path, vec![1, 2]);
  assert!(hits[0].node.children.is_empty());

  let kinds = tree.snapshot_matches(None, "text", 100).expect("kind matches");
  assert_eq!(kinds.iter().map(|m| m.node.id).collect::<Vec<_>>(), vec![2, 4]);

  // Scoped search: the path starts at the given root.
  let scoped = tree.snapshot_matches(Some(3), "text", 100).expect("scoped matches");
  assert_eq!(scoped.len(), 1);
  assert_eq!(scoped[0].path, vec![3, 4]);

  let limited = tree.snapshot_matches(None, "text", 1).expect("limited matches");
  assert_eq!(limited.len(), 1);

  assert!(tree.snapshot_matches(Some(99), "text", 100).is_none());
}
