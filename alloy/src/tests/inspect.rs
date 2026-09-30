// What a test or a dev tool reads a node by: the label, `find`, `is_visible`,
// `path_to` and `hit_path` (rendertree/tree/inspect.rs).

use taffy::style::Overflow;

use crate::rendertree::*;

fn attached() -> Element {
  View::default().with_layout()
}

// Writes a computed layout directly: unit tests have no GPU/platform context,
// so taffy never runs and placements are set by hand.
fn place(tree: &mut RenderTree, id: u64, x: f32, y: f32, w: f32, h: f32) {
  let l = tree.node_mut(id).layout_data_mut();
  l.computed.location = taffy::Point { x, y };
  l.computed.size = taffy::Size { width: w, height: h };
  l.laid_out = true;
}

fn view(tree: &mut RenderTree, id: u64, parent: Option<u64>, (x, y, w, h): (f32, f32, f32, f32)) {
  tree.create_node(id, attached());
  if let Some(parent) = parent {
    tree.insert_node(parent, id, None).expect("insert");
  }
  place(tree, id, x, y, w, h);
}

// A 200x200 root with a 100x100 panel at (10, 10) and two labelled boxes
// in it, the second below the panel's bottom edge.
fn panel() -> RenderTree {
  let mut tree = RenderTree::new();
  view(&mut tree, 1, None, (0.0, 0.0, 200.0, 200.0));
  tree.root = Some(1);
  view(&mut tree, 2, Some(1), (10.0, 10.0, 100.0, 100.0));
  view(&mut tree, 3, Some(2), (0.0, 0.0, 50.0, 50.0));
  view(&mut tree, 4, Some(2), (0.0, 150.0, 50.0, 50.0));
  tree.node_mut(3).label = Some("first".to_string());
  tree.node_mut(4).label = Some("second".to_string());
  tree
}

fn found(tree: &RenderTree, root: Option<u64>, query: NodeQuery) -> Vec<u64> {
  tree.find(root, &query, 100).expect("the root exists").iter().map(|m| m.node.id).collect()
}

fn by_label(label: Match<'_>) -> NodeQuery<'_> {
  NodeQuery { kind: None, text: Match::Any, label }
}

#[test]
fn find_matches_a_label_exactly_and_reports_the_path() {
  let tree = panel();
  assert_eq!(found(&tree, None, by_label(Match::Exact("second"))), vec![4]);
  assert_eq!(found(&tree, None, by_label(Match::Exact("Second"))), Vec::<u64>::new());
  assert_eq!(found(&tree, None, by_label(Match::Exact("sec"))), Vec::<u64>::new());
  assert_eq!(found(&tree, None, by_label(Match::Present)), vec![3, 4]);
  let matches = tree.find(None, &by_label(Match::Exact("second")), 100).expect("a tree");
  assert_eq!(matches[0].path, vec![1, 2, 4]);
  assert_eq!(matches[0].node.label.as_deref(), Some("second"));
}

#[test]
fn find_is_scoped_by_its_root_kind_and_limit() {
  let tree = panel();
  let views = NodeQuery { kind: Some("view"), text: Match::Any, label: Match::Any };
  assert_eq!(found(&tree, None, views), vec![1, 2, 3, 4]);
  let views = NodeQuery { kind: Some("view"), text: Match::Any, label: Match::Any };
  assert_eq!(found(&tree, Some(2), views), vec![2, 3, 4]);
  let texts = NodeQuery { kind: Some("text"), text: Match::Any, label: Match::Any };
  assert_eq!(found(&tree, None, texts), Vec::<u64>::new());
  assert_eq!(tree.find(None, &by_label(Match::Present), 1).expect("a tree").len(), 1);
  assert!(tree.find(Some(99), &by_label(Match::Present), 100).is_none());
}

// The loose query a person types matches a label as it matches a text.
#[test]
fn the_loose_query_matches_a_part_of_a_label() {
  let tree = panel();
  let ids: Vec<u64> = tree.snapshot_matches(None, "SEC", 100).expect("a tree").iter().map(|m| m.node.id).collect();
  assert_eq!(ids, vec![4]);
}

#[test]
fn a_node_overflowing_a_visible_parent_is_visible() {
  let tree = panel();
  assert!(tree.is_visible(3));
  // The panel does not clip: the box below its edge is painted, inside
  // the window.
  assert!(tree.is_visible(4));
}

#[test]
fn a_clipping_ancestor_cuts_what_is_outside_its_box() {
  let mut tree = panel();
  tree.node_mut(2).layout_data_mut().style.overflow.y = Overflow::Hidden;
  assert!(tree.is_visible(3));
  assert!(!tree.is_visible(4));
  // The clip is per axis: a horizontal one leaves the box below alone.
  tree.node_mut(2).layout_data_mut().style.overflow.y = Overflow::Visible;
  tree.node_mut(2).layout_data_mut().style.overflow.x = Overflow::Hidden;
  assert!(tree.is_visible(4));
}

#[test]
fn the_window_cuts_what_is_outside_it() {
  let mut tree = panel();
  place(&mut tree, 4, 0.0, 250.0, 50.0, 50.0);
  assert!(!tree.is_visible(4));
  // A part inside is enough.
  place(&mut tree, 4, 0.0, 170.0, 50.0, 50.0);
  assert!(tree.is_visible(4));
}

#[test]
fn opacity_multiplies_up_the_chain() {
  let mut tree = panel();
  let ElementKind::View(panel_view) = &mut tree.node_mut(2).kind else { panic!("a view") };
  panel_view.set_opacity(Some(0.0));
  assert!(!tree.is_visible(3));
  assert!(!tree.is_visible(2));
  assert!(tree.is_visible(1));
}

#[test]
fn a_node_without_area_off_the_tree_or_exiting_is_not_visible() {
  let mut tree = panel();
  place(&mut tree, 3, 0.0, 0.0, 0.0, 50.0);
  assert!(!tree.is_visible(3));
  // Created, never inserted.
  view(&mut tree, 5, None, (0.0, 0.0, 10.0, 10.0));
  assert!(!tree.is_visible(5));
  assert!(!tree.is_visible(99));
  tree.node_mut(2).lifecycle.exiting = true;
  assert!(!tree.is_visible(4));
}

#[test]
fn the_path_and_the_hit_path_run_root_first() {
  let tree = panel();
  assert_eq!(tree.path_to(4), Some(vec![1, 2, 4]));
  assert_eq!(tree.path_to(99), None);
  assert_eq!(tree.hit_path(Point::new(20.0, 20.0)), vec![1, 2, 3]);
  assert_eq!(tree.hit_path(Point::new(500.0, 500.0)), Vec::<u64>::new());
}
