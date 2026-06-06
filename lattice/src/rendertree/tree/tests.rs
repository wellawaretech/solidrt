use super::*;
use crate::rendertree::View;

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
