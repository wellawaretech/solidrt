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

/// The subtree as an outline: one node per line indented by depth, with its
/// kind, label, text and painted box (`view [save] 20,50 120x40`), and with
/// `props` the off-default props as JSON after it. No ids, so two runs print
/// the same text and a test can pin a layout against a string. A span that
/// is all of its parent's text is that text (as `find` sees it): one line,
/// not two. What `locator.outline()` reads and what a failed app test
/// prints.
pub fn outline(node: &alloy::rendertree::NodeSnapshot, tree: Option<&alloy::rendertree::RenderTree>, props: bool) -> String {
  let mut lines = Vec::new();
  outline_lines(node, tree, props, 0, &mut lines);
  lines.join("\n")
}

fn outline_lines(
  node: &alloy::rendertree::NodeSnapshot,
  tree: Option<&alloy::rendertree::RenderTree>,
  props: bool,
  depth: usize,
  out: &mut Vec<String>,
) {
  let mut line = format!("{}{}", "  ".repeat(depth), node.kind);
  if let Some(label) = &node.label {
    line.push_str(&format!(" [{label}]"));
  }
  if let Some(text) = &node.text {
    line.push_str(&format!(" {}", serde_json::Value::from(text.as_str())));
  }
  line.push_str(&format!(" {},{} {}x{}", round2(node.x), round2(node.y), round2(node.width), round2(node.height)));
  if props {
    if let Some(values) = tree.and_then(|tree| tree.try_node(node.id)).map(super::read_jsx).filter(|v| !v.is_empty()) {
      let map: serde_json::Map<String, serde_json::Value> =
        values.into_iter().map(|(name, value)| (name.to_string(), read_value_json(value))).collect();
      line.push_str(&format!(" {}", serde_json::Value::Object(map)));
    }
  }
  out.push(line);
  for child in &node.children {
    if child.text.is_some() && child.text == node.text {
      continue;
    }
    outline_lines(child, tree, props, depth + 1, out);
  }
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

// -- The GPU inventory --

// One sampler binding in the /gpu inventory: the bare source id, or
// `{ id, filter?, wrap? }` when the binding carries a sampling override.
fn binding_json(b: &alloy::TextureBinding) -> (String, serde_json::Value) {
  let value = if b.sampler.is_empty() {
    serde_json::json!(b.id)
  } else {
    let mut o = serde_json::Map::new();
    o.insert("id".into(), serde_json::json!(b.id));
    if let Some(f) = b.sampler.filter {
      o.insert("filter".into(), serde_json::json!(f.name()));
    }
    if let Some(w) = b.sampler.wrap {
      o.insert("wrap".into(), serde_json::json!(w.name()));
    }
    serde_json::Value::Object(o)
  };
  (b.name.clone(), value)
}

/// A texture's declared sampling in the create option's own vocabulary
/// (`{ filter, wrap, mipmap, anisotropy }`), the one place the JSON spelling
/// of a SamplerState lives: every inventory site that reports one calls
/// this, so a new sampling axis is added here once.
fn sampler_json(s: &alloy::SamplerState) -> serde_json::Value {
  serde_json::json!({
    "filter": s.filter.name(),
    "wrap": s.wrap.name(),
    "mipmap": s.mipmap,
    "anisotropy": s.anisotropy,
  })
}

/// A pipeline's buffer layouts as the createRenderPipeline `buffers` shape,
/// each field reported only off its default: `stepMode` absent means
/// "vertex", `arrayStride` always (it is what a subset layout is for), an
/// attribute's `offset` always.
fn buffer_layouts_json(layouts: &[alloy::GpuBufferLayoutInfo]) -> Vec<serde_json::Value> {
  layouts
    .iter()
    .map(|layout| {
      let attributes: Vec<serde_json::Value> = layout
        .attributes
        .iter()
        .map(|(name, format, offset)| serde_json::json!({"name": name, "format": format, "offset": offset}))
        .collect();
      let mut obj = serde_json::json!({"arrayStride": layout.stride, "attributes": attributes});
      if layout.step != "vertex" {
        obj.as_object_mut().expect("layout json is an object").insert("stepMode".into(), layout.step.into());
      }
      obj
    })
    .collect()
}

/// Add the create's debug label to a resource object, when one was given
/// (absent otherwise, like the other off-default keys).
pub fn insert_label(obj: &mut serde_json::Value, label: &Option<String>) {
  if let Some(label) = label {
    obj.as_object_mut().expect("resource json is an object").insert("label".into(), label.clone().into());
  }
}

/// Inventory alloy's GPU bookkeeping (textures, buffers, pipelines, programs)
/// as the record the control API's `/gpu` answers with and a test reads
/// (`flux:test/gui` `gpu`): shaped here once, like the node record above.
/// `label`, when given, keeps only the resources created with exactly that
/// debug label (every list; the window shader has none and is dropped).
/// Uniform values with more components than this are elided from a draw
/// entry's `params` (reported as their length, "[16]"), so a model's hundred
/// entries do not each carry two matrices; `draw` names the one entry
/// reported in full. Vectors up to a vec4 (colors, offsets) stay inline.
const PARAM_INLINE_COMPONENTS: usize = 4;

/// One uniform value for the gpu reply: the numbers, or with `elide` the
/// length marker for anything wider than `PARAM_INLINE_COMPONENTS`.
fn param_json(v: &alloy::ParamValue, elide: bool) -> serde_json::Value {
  match v {
    alloy::ParamValue::Scalar(n) => serde_json::json!(n),
    alloy::ParamValue::Array(a) if elide && a.len() > PARAM_INLINE_COMPONENTS => {
      serde_json::json!(format!("[{}]", a.len()))
    }
    alloy::ParamValue::Array(a) => serde_json::json!(a),
  }
}

/// `label` keeps only the resources created with that label; `draw` is the
/// draw entry id (target-scoped, so pair it with `label` to pin one target)
/// whose params are reported in full instead of elided.
pub fn gpu_record(ctx: &rquickjs::Ctx<'_>, label: Option<&str>, draw: Option<u64>) -> Result<serde_json::Value, String> {
  let Some(atx) = super::alloy_context(ctx) else {
    return Err("no alloy context".to_string());
  };
  let res = atx.gpu_resources();

  let textures: Vec<serde_json::Value> = res
    .textures
    .iter()
    .map(|t| {
      let mut obj = serde_json::json!({
        "id": t.id,
        "width": t.width,
        "height": t.height,
        "target": t.target,
        "format": t.format,
        "shape": t.shape,
        "sampler": sampler_json(&t.sampler),
        "byteLength": t.byte_length,
      });
      insert_label(&mut obj, &t.label);
      obj
    })
    .collect();
  let buffers: Vec<serde_json::Value> = res
    .buffers
    .iter()
    .map(|b| {
      let mut obj = serde_json::json!({"id": b.id, "byteLength": b.byte_length});
      insert_label(&mut obj, &b.label);
      obj
    })
    .collect();
  let pipelines: Vec<serde_json::Value> = res
    .pipelines
    .iter()
    .map(|p| {
      let mut obj = serde_json::json!({
        "textureId": p.texture_id,
        "kind": p.kind,
        // Manual targets render only on renderTarget, never by the flush.
        "manual": p.manual,
        "loadOp": if p.load { "load" } else { "clear" },
        // Cumulative like the get_stats aggregates (diff two queries for a
        // rate); issueMs is raster-thread occupancy, execMs is GPU-side
        // duration from timer queries (0 on a context without them).
        "passes": p.passes,
        "issueMs": p.pass_issue_micros / 1000,
        "execMs": p.pass_exec_micros / 1000,
        "textures": p.textures.iter().map(binding_json).collect::<serde_json::Map<_, _>>(),
        // Target-level params are one set per target (the camera, the
        // shadow matrices): always in full.
        "params": p.params.iter().map(|(name, v)| (name.clone(), param_json(v, false))).collect::<serde_json::Map<_, _>>(),
      });
      insert_label(&mut obj, &p.label);
      let map = obj.as_object_mut().expect("pipeline json is an object");
      if let Some(program_id) = p.program_id {
        map.insert("programId".into(), program_id.into());
      }
      if let Some(pipeline_id) = p.pipeline_id {
        map.insert("pipelineId".into(), pipeline_id.into());
      }
      if !p.buffer_ids.is_empty() {
        map.insert("buffers".into(), p.buffer_ids.clone().into());
      }
      // An index binding is itself off-default; with one present the range
      // keys switch to the index spellings (the numbers count indices).
      let indexed = p.index_buffer_id.is_some();
      if let Some(index_buffer_id) = p.index_buffer_id {
        map.insert("indexBuffer".into(), index_buffer_id.into());
      }
      if let Some(index_format) = p.index_format {
        map.insert("indexFormat".into(), index_format.into());
      }
      if let Some(topology) = p.topology {
        map.insert("topology".into(), topology.into());
      }
      if let Some(draw_count) = p.draw_count {
        map.insert(if indexed { "indexCount".into() } else { "drawCount".into() }, draw_count.into());
      }
      // Reported only off their defaults, like depthWrite below: absent
      // means the plain draw from the buffer's start.
      if let Some(first_vertex) = p.first_vertex.filter(|v| *v != 0) {
        map.insert(if indexed { "firstIndex".into() } else { "firstVertex".into() }, first_vertex.into());
      }
      if let Some(instance_count) = p.instance_count.filter(|v| *v != 1) {
        map.insert("instanceCount".into(), instance_count.into());
      }
      if p.depth {
        map.insert("depth".into(), true.into());
      }
      if p.samples > 1 {
        map.insert("samples".into(), p.samples.into());
      }
      // A sub-target: where it renders (its parent) and its rectangle
      // there, top-left origin like the texture leaf's srcX/srcY.
      if let Some(r) = &p.region {
        map.insert("into".into(), r.parent.into());
        map.insert("x".into(), r.x.into());
        map.insert("y".into(), r.y.into());
        map.insert("width".into(), r.width.into());
        map.insert("height".into(), r.height.into());
      }
      // Reported only off their defaults, like depth: absent means the
      // ordinary opaque draw.
      if p.depth_write == Some(false) {
        map.insert("depthWrite".into(), false.into());
      }
      if let Some(blend) = p.blend.filter(|b| *b != "none") {
        map.insert("blend".into(), blend.into());
      }
      if let Some(cull) = p.cull.filter(|c| *c != "none") {
        map.insert("cull".into(), cull.into());
      }
      if !p.buffers.is_empty() {
        map.insert("buffers".into(), buffer_layouts_json(&p.buffers).into());
      }
      // A draw target (kind "draws") reports its entries in list order; each
      // entry follows the flat fields' off-default conventions.
      if p.kind == "draws" {
        let draws: Vec<serde_json::Value> = p
          .draws
          .iter()
          .map(|d| {
            let elide = draw != Some(d.id);
            let mut entry = serde_json::json!({
              "id": d.id,
              "textures": d.textures.iter().map(binding_json).collect::<serde_json::Map<_, _>>(),
              "params": d.params.iter().map(|(name, v)| (name.clone(), param_json(v, elide))).collect::<serde_json::Map<_, _>>(),
            });
            let map = entry.as_object_mut().expect("draw json is an object");
            if let Some(pipeline_id) = d.pipeline_id {
              map.insert("pipelineId".into(), pipeline_id.into());
            }
            if !d.buffer_ids.is_empty() {
              map.insert("buffers".into(), d.buffer_ids.clone().into());
            }
            // An index binding is itself off-default; with one present the
            // range keys switch to the index spellings (indices, not
            // vertices).
            let indexed = d.index_buffer_id.is_some();
            if let Some(index_buffer_id) = d.index_buffer_id {
              map.insert("indexBuffer".into(), index_buffer_id.into());
            }
            if let Some(index_format) = d.index_format {
              map.insert("indexFormat".into(), index_format.into());
            }
            if let Some(label) = &d.label {
              map.insert("label".into(), label.clone().into());
            }
            map.insert("topology".into(), d.topology.into());
            map.insert(if indexed { "indexCount".into() } else { "vertexCount".into() }, d.vertex_count.into());
            if d.first_vertex != 0 {
              map.insert(if indexed { "firstIndex".into() } else { "firstVertex".into() }, d.first_vertex.into());
            }
            if d.instance_count != 1 {
              map.insert("instanceCount".into(), d.instance_count.into());
            }
            if !d.depth_write {
              map.insert("depthWrite".into(), false.into());
            }
            if d.blend != "none" {
              map.insert("blend".into(), d.blend.into());
            }
            if d.cull != "none" {
              map.insert("cull".into(), d.cull.into());
            }
            entry
          })
          .collect();
        map.insert("draws".into(), draws.into());
      }
      obj
    })
    .collect();

  let render_pipelines: Vec<serde_json::Value> = res
    .render_pipelines
    .iter()
    .map(|p| {
      let mut obj = serde_json::json!({"id": p.id, "programId": p.program_id});
      insert_label(&mut obj, &p.label);
      let map = obj.as_object_mut().expect("render pipeline json is an object");
      // Draw state reported only off its defaults, like the per-target infos.
      if p.topology != "triangles" {
        map.insert("topology".into(), p.topology.into());
      }
      if p.blend != "none" {
        map.insert("blend".into(), p.blend.into());
      }
      if p.cull != "none" {
        map.insert("cull".into(), p.cull.into());
      }
      if p.depth {
        map.insert("depth".into(), true.into());
      }
      if !p.depth_write {
        map.insert("depthWrite".into(), false.into());
      }
      if !p.buffers.is_empty() {
        map.insert("buffers".into(), buffer_layouts_json(&p.buffers).into());
      }
      obj
    })
    .collect();

  let programs: Vec<serde_json::Value> = res
    .programs
    .iter()
    .map(|p| {
      let mut obj = serde_json::json!({"id": p.id});
      insert_label(&mut obj, &p.label);
      obj
    })
    .collect();

  let keep = |list: Vec<serde_json::Value>| -> Vec<serde_json::Value> {
    match label {
      None => list,
      Some(label) => list.into_iter().filter(|obj| obj.get("label").and_then(|l| l.as_str()) == Some(label)).collect(),
    }
  };
  let mut data = serde_json::json!({
    "textures": keep(textures), "buffers": keep(buffers), "pipelines": keep(pipelines),
    "renderPipelines": keep(render_pipelines), "programs": keep(programs),
  });
  if let (Some(ws), None) = (&res.window_shader, label) {
    data["windowShader"] = serde_json::json!({
      "programId": ws.program_id, "layerWidth": ws.width, "layerHeight": ws.height,
      "previous": ws.previous, "passOnlyFrames": ws.pass_only_frames,
    });
  }

  Ok(data)
}
