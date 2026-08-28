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
  tree.insert_node(1, 2, None).expect("insert");

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
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, Some(2)).expect("insert");

  assert_eq!(tree.node(1).children, vec![3, 2]);
  assert_eq!(tree.node(1).layout_data().layout_children, vec![NodeId::from(3u64), NodeId::from(2u64)]);
}

#[test]
fn insert_node_with_unknown_anchor_appends() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, Some(99)).expect("insert");

  assert_eq!(tree.node(1).children, vec![2, 3]);
}

#[test]
fn reinserting_child_does_not_duplicate() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 2, None).expect("insert");

  assert_eq!(tree.node(1).children, vec![2]);
  assert_eq!(tree.node(1).layout_data().layout_children, vec![NodeId::from(2u64)]);
}

#[test]
fn attached_under_detached_is_refused_and_leaves_tree_untouched() {
  let mut tree = RenderTree::new();
  tree.create_node(1, detached());
  tree.create_node(2, attached());
  let err = tree.insert_node(1, 2, None).expect_err("laid-out under detached must be refused");
  assert!(err.starts_with("<view> cannot be a child of <d-view>"), "{err}");
  assert!(tree.node(1).children.is_empty());
  assert_eq!(tree.node(2).parent, None);
}

#[test]
fn detached_child_not_in_layout_children() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, detached());
  tree.insert_node(1, 2, None).expect("insert");

  assert_eq!(tree.node(1).children, vec![2]);
  assert!(tree.node(1).layout_data().layout_children.is_empty());
}

#[test]
fn detach_node_keeps_subtree_alive() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");

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
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, None).expect("insert");

  // Move node 3 from under 1 to under 2 without an explicit detach first.
  tree.insert_node(2, 3, None).expect("insert");

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
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");

  tree.detach_node(1, 2);
  tree.destroy_node(2);

  assert!(tree.try_node(2).is_none());
  assert!(tree.try_node(3).is_none());
}

#[test]
fn mounted_count_excludes_orphans() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.root = Some(1);
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  // A node created but never inserted is live but not mounted.
  tree.create_node(4, attached());
  assert_eq!(tree.node_count(), 4);
  assert_eq!(tree.mounted_count(), 3);

  // A detached subtree stays live but leaves the mounted tree.
  tree.detach_node(1, 2);
  assert_eq!(tree.node_count(), 4);
  assert_eq!(tree.mounted_count(), 1);

  // Destroy reclaims the detached subtree; the orphan gap is what remains.
  tree.destroy_node(2);
  assert_eq!(tree.node_count(), 2);
  assert_eq!(tree.mounted_count(), 1);
}

#[test]
fn delete_node_removes_subtree() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");

  tree.delete_node(1, 2);

  assert_eq!(tree.node(1).children, Vec::<u64>::new());
  assert!(tree.node(1).layout_data().layout_children.is_empty());
  assert!(tree.try_node(2).is_none());
  assert!(tree.try_node(3).is_none());
}

fn runs_of(tree: &RenderTree, id: u64) -> Vec<TextRun> {
  match &tree.node(id).kind {
    ElementKind::Text(t) => t.runs.clone(),
    _ => panic!("not a text"),
  }
}

// Spans nest: a leaf's run carries the overrides layered along its span
// ancestry, computed_text is the concatenation, and a write anywhere in the
// span subtree resyncs the owning text.
#[test]
fn text_collects_styled_runs_from_nested_spans() {
  let mut tree = RenderTree::new();
  tree.create_node(1, Text::default().with_layout());
  tree.create_node(2, Span { text: "Hello, ".into(), ..Default::default() }.no_layout());
  let mut outer = Span::default();
  outer.set_font_weight(Some(crate::impellers::FontWeight::Bold));
  tree.create_node(3, outer.no_layout());
  let mut inner = Span::default();
  inner.set_font_size(Some(30.0));
  tree.create_node(4, inner.no_layout());
  tree.create_node(5, Span { text: "world".into(), ..Default::default() }.no_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, None).expect("insert");
  tree.insert_node(3, 4, None).expect("insert");
  tree.insert_node(4, 5, None).expect("insert");

  let runs = runs_of(&tree, 1);
  assert_eq!(runs.len(), 2);
  assert_eq!(runs[0].text, "Hello, ");
  assert_eq!(runs[0].overrides, RunOverrides::default());
  assert_eq!(runs[1].text, "world");
  assert_eq!(runs[1].overrides.font_weight, Some(crate::impellers::FontWeight::Bold));
  assert_eq!(runs[1].overrides.font_size, Some(30.0));
  match &tree.node(1).kind {
    ElementKind::Text(t) => assert_eq!(t.computed_text, "Hello, world"),
    _ => unreachable!(),
  }

  // A write on the deepest leaf, and one on the outer span, both resync.
  tree.edit(5, |el| match &mut el.kind {
    ElementKind::Span(s) => s.set_text("there".into()),
    _ => unreachable!(),
  });
  assert_eq!(runs_of(&tree, 1)[1].text, "there");
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::Span(s) => s.set_font_size(Some(12.0)),
    _ => unreachable!(),
  });
  // The inner span's own size still wins over the outer's.
  assert_eq!(runs_of(&tree, 1)[1].overrides.font_size, Some(30.0));
  tree.edit(4, |el| match &mut el.kind {
    ElementKind::Span(s) => s.set_font_style(Some(crate::impellers::FontStyle::Italic)),
    _ => unreachable!(),
  });
  let run = &runs_of(&tree, 1)[1];
  assert_eq!(run.overrides.font_style, Some(crate::impellers::FontStyle::Italic));
  assert_eq!(run.overrides.font_weight, Some(crate::impellers::FontWeight::Bold));

  // Removing the outer span drops its runs.
  tree.delete_node(1, 3);
  assert_eq!(runs_of(&tree, 1).len(), 1);
}

// A laid-out element child of a text is an inline atom: one ATOM_CHAR run
// naming the element, whose box the layout pass writes and a resync keeps.
#[test]
fn text_collects_atom_runs_and_keeps_their_size() {
  let mut tree = RenderTree::new();
  tree.create_node(1, Text::default().with_layout());
  tree.create_node(2, Span { text: "Rated ".into(), ..Default::default() }.no_layout());
  tree.create_node(3, View::default().with_layout());
  tree.create_node(4, Span { text: " stars".into(), ..Default::default() }.no_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, None).expect("insert");
  tree.insert_node(1, 4, None).expect("insert");

  let runs = runs_of(&tree, 1);
  assert_eq!(runs.len(), 3);
  assert_eq!(runs[0].node, 2);
  assert_eq!(runs[1].text, ATOM_CHAR);
  assert_eq!(runs[1].node, 3);
  assert_eq!(runs[1].atom, Some(Size::zero()));
  assert_eq!(runs[2].node, 4);
  match &tree.node(1).kind {
    ElementKind::Text(t) => assert_eq!(t.computed_text, format!("Rated {ATOM_CHAR} stars")),
    _ => unreachable!(),
  }

  match &mut tree.node_mut(1).kind {
    ElementKind::Text(t) => {
      assert!(t.set_atom_size(3, Size::new(16.0, 16.0)));
      assert!(!t.set_atom_size(3, Size::new(16.0, 16.0)));
    }
    _ => unreachable!(),
  }
  // A span write resyncs the runs; the atom keeps its measured box.
  tree.edit(4, |el| match &mut el.kind {
    ElementKind::Span(s) => s.set_text(" of five".into()),
    _ => unreachable!(),
  });
  let runs = runs_of(&tree, 1);
  assert_eq!(runs[1].atom, Some(Size::new(16.0, 16.0)));
  assert_eq!(runs[2].text, " of five");
}

// A text node with laid-out content, for snapshot query matching.
fn text(content: &str) -> Element {
  let mut t = Text::default();
  t.set_plain_text(content.to_string());
  t.with_layout()
}

#[test]
fn snapshot_from_caps_depth_and_reports_child_count() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
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
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, None).expect("insert");
  tree.insert_node(3, 4, None).expect("insert");
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

#[test]
fn referenced_texture_ids_covers_attached_and_detached() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.root = Some(1);

  // Attached texture element referencing id 10.
  let mut tex = Texture::default();
  tex.texture_id = Some(10);
  tree.create_node(2, tex.with_layout());
  tree.insert_node(1, 2, None).expect("insert");

  // Detached (d-texture) element referencing id 20: still live, still counts.
  let mut dtex = Texture::default();
  dtex.texture_id = Some(20);
  tree.create_node(3, dtex.no_layout());

  // Texture element with no src, and a non-texture node: neither contributes.
  tree.create_node(4, Texture::default().with_layout());
  tree.insert_node(1, 4, None).expect("insert");

  let ids = tree.referenced_texture_ids();
  assert!(ids.contains(&10));
  assert!(ids.contains(&20));
  assert_eq!(ids.len(), 2);

  // Destroying the referencing node drops its id from the set.
  tree.destroy_node(3);
  let ids = tree.referenced_texture_ids();
  assert!(!ids.contains(&20));
  assert_eq!(ids.len(), 1);

  // A boundary shader's extra sampler inputs are references too: the pass
  // samples them every re-run, with no texture element to show for it.
  tree.edit(1, |el| match &mut el.kind {
    ElementKind::View(v) => {
      v.shader = Some(crate::gpu::NodeShader {
        program: 1,
        params: vec![],
        textures: vec![crate::gpu::TextureBinding::new("uLut", 30)],
        outset: 0.0,
        previous: false,
      });
      Damage::None
    }
    _ => unreachable!(),
  });
  let ids = tree.referenced_texture_ids();
  assert!(ids.contains(&30));
  assert_eq!(ids.len(), 2);
}

// --- bounding box -----------------------------------------------------------

// Writes a computed layout directly: unit tests have no GPU/platform context,
// so taffy never runs and placements are set by hand. The cache is seeded with
// one entry because content_fallback treats an empty cache as "not laid out".
fn place(tree: &mut RenderTree, id: u64, x: f32, y: f32, w: f32, h: f32) {
  let l = tree.node_mut(id).layout_data_mut();
  l.computed.location = taffy::Point { x, y };
  l.computed.size = taffy::Size { width: w, height: h };
  let input = taffy::tree::LayoutInput {
    run_mode: taffy::RunMode::PerformLayout,
    sizing_mode: taffy::SizingMode::InherentSize,
    axis: taffy::RequestedAxis::Both,
    known_dimensions: taffy::Size::NONE,
    parent_size: taffy::Size::NONE,
    available_space: taffy::Size {
      width: taffy::AvailableSpace::Definite(w),
      height: taffy::AvailableSpace::Definite(h),
    },
    vertical_margins_are_collapsible: taffy::Line::FALSE,
  };
  l.cache.store(&input, taffy::tree::LayoutOutput::from_outer_size(taffy::Size { width: w, height: h }));
}

fn assert_box(b: Rect, x: f32, y: f32, w: f32, h: f32) {
  let eps = 1e-3;
  assert!(
    (b.origin.x - x).abs() < eps
      && (b.origin.y - y).abs() < eps
      && (b.size.width - w).abs() < eps
      && (b.size.height - h).abs() < eps,
    "expected ({x}, {y}, {w}, {h}), got ({}, {}, {}, {})",
    b.origin.x,
    b.origin.y,
    b.size.width,
    b.size.height
  );
}

#[test]
fn bounding_box_composes_translations() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 10.0, 20.0, 100.0, 100.0);
  place(&mut tree, 3, 5.0, 5.0, 20.0, 20.0);

  let b = tree.bounding_box_viewport(3).expect("laid out");
  assert_box(b, 15.0, 25.0, 20.0, 20.0);
}

#[test]
fn bounding_box_scaled_ancestor() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_scale_x(Some(0.5));
  v.set_scale_y(Some(0.5));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 50.0, 50.0, 100.0, 100.0);
  place(&mut tree, 3, 10.0, 10.0, 20.0, 20.0);

  // Scale 0.5 around the parent's center (50, 50): the child's (10, 10)-(30, 30)
  // box maps to (30, 30)-(40, 40), then the parent location (50, 50) shifts it.
  let b = tree.bounding_box_viewport(3).expect("laid out");
  assert_box(b, 80.0, 80.0, 10.0, 10.0);
}

#[test]
fn bounding_box_scrolled_design_size_ancestor() {
  // Scroll means box pixels (okf/backlog/overflow-viewbox-clip.md): under a
  // fit scale of 0.5, a 10 px scroll removes 20 design units before the fit
  // maps the corners into the box. A raw design-unit subtraction would land
  // the box 5 px to the right.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((200.0, 200.0)));
  v.set_scroll_x(Some(10.0));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 100.0, 100.0, 50.0, 50.0);

  // Child corners in design space (100..150), minus 20 design units of
  // scroll (80..130), through the 0.5 fit: box (40..65, 50..75).
  let b = tree.bounding_box_viewport(3).expect("laid out");
  assert_box(b, 40.0, 50.0, 25.0, 25.0);
}

#[test]
fn bounding_box_rotated_ancestor() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_rotate(Some(std::f32::consts::FRAC_PI_2));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 20.0);

  // A quarter turn around the parent's center (50, 50) stands the 100x20 bar
  // upright along the parent's right edge.
  let b = tree.bounding_box_viewport(3).expect("laid out");
  assert_box(b, 80.0, 0.0, 20.0, 100.0);
}

#[test]
fn bounding_box_own_scale() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_scale_x(Some(2.0));
  v.set_scale_y(Some(2.0));
  tree.create_node(2, v.with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 50.0, 50.0);

  // The node's own transform counts (getBoundingClientRect semantics): scale 2
  // around its center (25, 25) grows the box symmetrically past the origin.
  let b = tree.bounding_box_viewport(2).expect("laid out");
  assert_box(b, -25.0, -25.0, 100.0, 100.0);
}

fn assert_point(p: Point, x: f32, y: f32) {
  let eps = 1e-3;
  assert!((p.x - x).abs() < eps && (p.y - y).abs() < eps, "expected ({x}, {y}), got ({}, {})", p.x, p.y);
}

#[test]
fn painted_quad_untransformed_is_box_corners() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.insert_node(1, 2, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 10.0, 20.0, 100.0, 50.0);

  let q = tree.painted_quad(2).expect("laid out");
  assert_point(q[0], 10.0, 20.0);
  assert_point(q[1], 110.0, 20.0);
  assert_point(q[2], 110.0, 70.0);
  assert_point(q[3], 10.0, 70.0);
}

#[test]
fn painted_quad_rotated_ancestor_carries_corners() {
  // Same setup as bounding_box_rotated_ancestor: the AABB collapses the
  // rotation, the quad preserves it - and rebuilding the AABB from the quad
  // must reproduce bounding_box_viewport exactly (one code path).
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_rotate(Some(std::f32::consts::FRAC_PI_2));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 20.0);

  let q = tree.painted_quad(3).expect("laid out");
  // A quarter turn around (50, 50): pre-transform top-left lands at (100, 0),
  // and the corner order keeps identifying which original corner is which.
  assert_point(q[0], 100.0, 0.0);
  assert_point(q[1], 100.0, 100.0);
  assert_point(q[2], 80.0, 100.0);
  assert_point(q[3], 80.0, 0.0);

  let b = tree.bounding_box_viewport(3).expect("laid out");
  assert_eq!(b, Rect::from_points(q));
  assert_box(b, 80.0, 0.0, 20.0, 100.0);
}

#[test]
fn bounding_box_translate_and_scroll_fast_path() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_x(Some(5.0));
  v.set_scroll_y(Some(10.0));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 20.0, 20.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 10.0, 10.0);

  // No matrix props: translate adds, scroll subtracts, layout position adds.
  let b = tree.bounding_box_viewport(3).expect("laid out");
  assert_box(b, 25.0, 10.0, 10.0, 10.0);
}

// --- texture_content_changed: GPU content writes as snapshot damage ---------

use std::collections::HashSet;

use crate::gpu::NodeShader;

fn ids(list: &[u64]) -> HashSet<u64> {
  list.iter().copied().collect()
}

// A snapshot-boundary view over a texture leaf showing registry id `tex`.
fn snapshot_over_texture(tree: &mut RenderTree, tex: u64) {
  tree.create_node(1, attached());
  tree.create_node(2, Texture::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.edit(1, |el| {
    el.repaint_boundary = BoundaryMode::Snapshot;
    Damage::None
  });
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(tex)),
    _ => unreachable!(),
  });
}

#[test]
fn content_change_under_snapshot_bumps_revision() {
  let mut tree = RenderTree::new();
  snapshot_over_texture(&mut tree, 7);
  let before = tree.revision();
  assert!(tree.texture_content_changed(&ids(&[7])));
  assert_ne!(tree.revision(), before);
}

#[test]
fn own_snapshot_texture_change_does_not_invalidate_boundary() {
  // A boundary showing its own vended snapshot texture: the rasterization
  // IS the content change, so it must not re-invalidate itself (a
  // re-raster every frame). The same id under a different boundary still
  // counts, and the deleted boundary surrenders its id for release.
  let mut tree = RenderTree::new();
  snapshot_over_texture(&mut tree, 7);
  tree.edit(1, |el| {
    el.snapshot_texture_id.set(Some(7));
    Damage::None
  });
  let before = tree.revision();
  assert!(!tree.texture_content_changed(&ids(&[7])));
  assert_eq!(tree.revision(), before);

  tree.create_node(3, attached());
  tree.create_node(4, Texture::default().with_layout());
  tree.insert_node(3, 4, None).expect("insert");
  tree.edit(3, |el| {
    el.repaint_boundary = BoundaryMode::Snapshot;
    Damage::None
  });
  tree.edit(4, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(7)),
    _ => unreachable!(),
  });
  assert!(tree.texture_content_changed(&ids(&[7])));

  tree.delete_node(1, 2);
  assert!(tree.take_released_snapshot_textures().is_empty());
  tree.destroy_node(1);
  assert_eq!(tree.take_released_snapshot_textures(), vec![7]);
}

#[test]
fn content_change_without_snapshot_is_ignored() {
  // The fast-path guarantee: a plain displayed texture (no baked pixels
  // anywhere above it) must not bump the revision, or pure-GPU animation
  // would lose the present-only reuse path.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, Texture::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(7)),
    _ => unreachable!(),
  });
  let before = tree.revision();
  assert!(!tree.texture_content_changed(&ids(&[7])));
  assert_eq!(tree.revision(), before);
}

#[test]
fn unrelated_id_is_ignored() {
  let mut tree = RenderTree::new();
  snapshot_over_texture(&mut tree, 7);
  let before = tree.revision();
  assert!(!tree.texture_content_changed(&ids(&[8])));
  assert_eq!(tree.revision(), before);
}

#[test]
fn empty_set_is_ignored() {
  let mut tree = RenderTree::new();
  snapshot_over_texture(&mut tree, 7);
  assert!(!tree.texture_content_changed(&ids(&[])));
}

#[test]
fn recording_boundary_is_not_a_bake() {
  // A recording boundary holds a display list with a LIVE texture
  // reference, not pixels; the raster flush refreshes it without damage.
  let mut tree = RenderTree::new();
  snapshot_over_texture(&mut tree, 7);
  tree.edit(1, |el| {
    el.repaint_boundary = BoundaryMode::Recording;
    Damage::None
  });
  let before = tree.revision();
  assert!(!tree.texture_content_changed(&ids(&[7])));
  assert_eq!(tree.revision(), before);
}

#[test]
fn snapshot_no_aa_counts_as_bake() {
  let mut tree = RenderTree::new();
  snapshot_over_texture(&mut tree, 7);
  tree.edit(1, |el| {
    el.repaint_boundary = BoundaryMode::SnapshotNoAa;
    Damage::None
  });
  assert!(tree.texture_content_changed(&ids(&[7])));
}

#[test]
fn boundary_above_the_boundary_still_hits() {
  // The texture sits two levels below the snapshot boundary; the inclusive
  // ancestor probe must still find it.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, Texture::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.edit(1, |el| {
    el.repaint_boundary = BoundaryMode::Snapshot;
    Damage::None
  });
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(7)),
    _ => unreachable!(),
  });
  assert!(tree.texture_content_changed(&ids(&[7])));
}

// A snapshot-boundary view whose shader samples `tex` as an extra input.
fn shaded_boundary(tree: &mut RenderTree, id: u64, tex: u64) {
  tree.create_node(id, attached());
  tree.edit(id, |el| {
    el.repaint_boundary = BoundaryMode::Snapshot;
    match &mut el.kind {
      ElementKind::View(v) => {
        v.shader = Some(NodeShader {
          program: 1,
          params: vec![],
          textures: vec![crate::gpu::TextureBinding::new("uLut", tex)],
          outset: 0.0,
          previous: false,
        });
      }
      _ => unreachable!(),
    }
    Damage::None
  });
}

#[test]
fn boundary_shader_input_counts_as_reference() {
  // The boundary shader samples id 9 as an extra input (not via any texture
  // element); its pass output goes stale when 9's content changes.
  let mut tree = RenderTree::new();
  shaded_boundary(&mut tree, 1, 9);
  let before = tree.revision();
  assert!(tree.texture_content_changed(&ids(&[9])));
  assert_ne!(tree.revision(), before);
  // The narrow path: the pass is flagged to re-run (the field write in setup
  // did not go through set_shader, so the flag can only come from the hit).
  match &tree.node(1).kind {
    ElementKind::View(v) => assert!(v.take_shader_dirty()),
    _ => unreachable!(),
  }
}

#[test]
fn boundary_shader_input_hit_keeps_the_bake() {
  // A shader-INPUT content change needs only the pass rerun: the boundary's
  // own cache must survive (invalidate_paint would take it), while the
  // parent's recording repaints (Compose - it holds the old composited
  // quad). The planted Recording on the boundary stands in for the real
  // Snapshot cache, which needs a GPU texture a unit test cannot allocate.
  let mut tree = RenderTree::new();
  tree.create_node(0, attached());
  shaded_boundary(&mut tree, 1, 9);
  tree.insert_node(0, 1, None).expect("insert");
  let dl = || crate::impellers::DisplayListBuilder::new(None).build().expect("build empty display list");
  *tree.node(0).paint_cache.borrow_mut() = Some(PaintCache::Recording(dl()));
  *tree.node(1).paint_cache.borrow_mut() = Some(PaintCache::Recording(dl()));
  assert!(tree.texture_content_changed(&ids(&[9])));
  assert!(tree.node(1).paint_cache.borrow().is_some(), "the boundary's bake must survive");
  assert!(tree.node(0).paint_cache.borrow().is_none(), "the parent recording must repaint");
}

#[test]
fn detached_texture_without_boundary_is_ignored() {
  // A never-inserted texture node has no ancestors, so no bake references it.
  let mut tree = RenderTree::new();
  tree.create_node(5, Texture::default().no_layout());
  tree.edit(5, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(7)),
    _ => unreachable!(),
  });
  assert!(!tree.texture_content_changed(&ids(&[7])));
}

#[test]
fn content_hit_invalidates_the_boundary_cache_path() {
  // invalidate_paint clears Recording caches from the node up; plant one on
  // the boundary's parent and confirm a content hit clears it (the snapshot
  // itself needs a GPU texture, which a unit test cannot allocate).
  let mut tree = RenderTree::new();
  tree.create_node(0, attached());
  snapshot_over_texture(&mut tree, 7);
  tree.insert_node(0, 1, None).expect("insert");
  tree.edit(0, |el| {
    el.repaint_boundary = BoundaryMode::Recording;
    Damage::None
  });
  *tree.node(0).paint_cache.borrow_mut() = Some(PaintCache::Recording(
    crate::impellers::DisplayListBuilder::new(None).build().expect("build empty display list"),
  ));
  assert!(tree.texture_content_changed(&ids(&[7])));
  assert!(tree.node(0).paint_cache.borrow().is_none());
}
