// The record of a render-tree node, as JSON: what the control API's `/tree`
// answers with and what a test reads a node as, shaped in one place so the
// two never drift (okf/plans/test-harness.md, D8 and D32). The tree copy
// itself is the rendertree's (`NodeSnapshot`); this adds what needs the
// live tree and the JSX names (`props`, the painted quad, exits, slides).

/// Coordinates and values in a record are rounded to hundredths: enough
/// for any assertion, and free of float noise.
fn round2(v: f32) -> f64 {
  (v as f64 * 100.0).round() / 100.0
}

/// One node of the `/tree` control endpoint's record. What each field
/// means is documented once, in packages/cli/agents/debugging.md ("The
/// control API without MCP", the `/tree` entry); a field added here is
/// added there. `props`, when set, is the live tree, and the fields that
/// need it (`props`, `quad`, `exiting`/`exit`, `slide`) come from it.
pub fn node_record(
  node: &alloy::rendertree::NodeSnapshot,
  props: Option<&alloy::rendertree::RenderTree>,
) -> serde_json::Value {
  let mut obj = serde_json::json!({
    "id": node.id,
    "kind": node.kind,
    "x": round2(node.x),
    "y": round2(node.y),
    "width": round2(node.width),
    "height": round2(node.height),
  });
  let map = obj.as_object_mut().expect("a node record is an object");
  if node.detached {
    map.insert("detached".into(), true.into());
  }
  if let Some(text) = &node.text {
    map.insert("text".into(), text.clone().into());
  }
  if let Some(label) = &node.label {
    map.insert("label".into(), label.clone().into());
  }
  if let Some(tree) = props {
    if let Some(element) = tree.try_node(node.id) {
      let values = super::read_jsx(element);
      if !values.is_empty() {
        let mut props_map = serde_json::Map::with_capacity(values.len());
        for (name, value) in values {
          props_map.insert(name.into(), read_value_json(value));
        }
        map.insert("props".into(), props_map.into());
      }
      // An exit in flight: the root carries `exiting`, and every node in
      // the cascade names the motion in force per exiting property, so a
      // per-direction curve is a one-line read rather than a curve fit on
      // stepped values.
      if element.lifecycle.exiting {
        map.insert("exiting".into(), true.into());
      }
      let exits: serde_json::Map<String, serde_json::Value> = tree
        .exit_motions(node.id)
        .into_iter()
        .map(|(prop, exit)| {
          let mut motion = exit.spec.to_string();
          if exit.delay_ms > 0.0 {
            motion.push_str(&format!(" delay {}ms", exit.delay_ms));
          }
          (super::anim_prop_name(prop).to_string(), motion.into())
        })
        .collect();
      if !exits.is_empty() {
        map.insert("exit".into(), exits.into());
      }
      // A layout slide in flight: the box above is the one the node is
      // painted at, `slide` what it has still to cover to its solved box
      // (offset and growth), so a slide and a jump read apart frame by
      // frame.
      if let Some(remaining) = tree.slide_remaining(node.id) {
        let (offset, growth) = (remaining.offset, remaining.growth);
        map.insert(
          "slide".into(),
          serde_json::json!({ "x": round2(offset.x), "y": round2(offset.y), "w": round2(growth.x), "h": round2(growth.y) }),
        );
      }
    }
    if let Some(quad) = tree.painted_quad(node.id) {
      if !quad_is_aabb(&quad) {
        let flat: Vec<serde_json::Value> = quad.iter().flat_map(|p| [round2(p.x).into(), round2(p.y).into()]).collect();
        map.insert("quad".into(), flat.into());
      }
    }
  }
  if !node.children.is_empty() {
    map.insert("children".into(), node.children.iter().map(|c| node_record(c, props)).collect::<Vec<_>>().into());
  }
  // A depth cap cut this node's children off: surface how many exist so a
  // reader knows to descend with root=<id>.
  if node.children.len() < node.child_count {
    map.insert("childCount".into(), node.child_count.into());
  }
  obj
}

/// True when the painted quad is still the axis-aligned box the snapshot
/// already reports (top-left, top-right, bottom-right, bottom-left of its own
/// AABB) - the untransformed common case, where emitting it would be noise.
fn quad_is_aabb(quad: &[alloy::rendertree::Point; 4]) -> bool {
  const EPS: f32 = 0.01;
  let eq = |a: f32, b: f32| (a - b).abs() < EPS;
  let (min_x, max_x) =
    (quad.iter().map(|p| p.x).fold(f32::MAX, f32::min), quad.iter().map(|p| p.x).fold(f32::MIN, f32::max));
  let (min_y, max_y) =
    (quad.iter().map(|p| p.y).fold(f32::MAX, f32::min), quad.iter().map(|p| p.y).fold(f32::MIN, f32::max));
  eq(quad[0].x, min_x)
    && eq(quad[0].y, min_y)
    && eq(quad[1].x, max_x)
    && eq(quad[1].y, min_y)
    && eq(quad[2].x, max_x)
    && eq(quad[2].y, max_y)
    && eq(quad[3].x, min_x)
    && eq(quad[3].y, max_y)
}

fn read_value_json(value: super::ReadValue) -> serde_json::Value {
  match value {
    super::ReadValue::Num(n) => round2(n as f32).into(),
    super::ReadValue::Int(n) => n.into(),
    super::ReadValue::Bool(b) => b.into(),
    super::ReadValue::Str(s) => s.into(),
    super::ReadValue::Nums(list) => list.into_iter().map(|n| round2(n as f32)).collect::<Vec<_>>().into(),
  }
}
