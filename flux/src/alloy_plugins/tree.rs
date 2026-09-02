use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, IntoJs, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use taffy::prelude::*;

use super::AlloyContext;
use crate::alloy_plugins::value::PropValue;
use crate::plugins::marshal::OptArg;
use alloy::rendertree::text::{prepare_units, PreparedRun};
use alloy::rendertree::{
  AnimProp, AnimValue, Commit, Damage, Element, EventInterest, FrameDriver, Measurable, MeasureContext,
  PlatformContext, Rect, RenderTree, Text, Window,
};

thread_local! {
  // setProperty (FFI prop write) calls since the last frame. Bumped in the
  // native setProperty handler below; read and reset each frame by the draw
  // bridge (in lattice) for the debug overlay. Lives on the single JS thread,
  // so no timing call crosses into JS.
  pub static SETPROP_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Marshals a JavaScript value into the engine-independent PropValue that
// rendertree setters consume. This is the FFI boundary: rquickjs types stay on
// this side of it. Errors are real JS exceptions raised while reading the
// value (e.g. a Proxy getter throwing) and propagate to the caller unchanged.
// Shared with the spatial plugin, whose setTransition speaks the same
// transition vocabulary.
pub(crate) fn to_prop_value(value: &Value<'_>) -> rquickjs::Result<PropValue> {
  Ok(if value.is_null() || value.is_undefined() {
    PropValue::Null
  } else if let Some(b) = value.as_bool() {
    PropValue::Bool(b)
  } else if let Some(n) = value.as_number() {
    PropValue::Number(n)
  } else if let Some(s) = value.as_string() {
    match s.to_string() {
      Ok(text) => PropValue::Text(text),
      // A JS string with unpaired surrogates (possible from UTF-16 slicing in
      // text-editing code) is not valid UTF-8; degrade to a lossy copy instead
      // of failing the whole property write over string CONTENT.
      Err(_) => {
        let c = s.clone().to_cstring()?;
        // SAFETY: as_ptr/len delimit the engine-owned byte buffer of this
        // string, alive until `c` drops at the end of this block; the bytes
        // are read once and copied out by from_utf8_lossy.
        let bytes = unsafe { std::slice::from_raw_parts(c.as_ptr() as *const u8, c.len()) };
        PropValue::Text(String::from_utf8_lossy(bytes).into_owned())
      }
    }
  } else if let Some(arr) = value.as_array() {
    let mut items = Vec::with_capacity(arr.len());
    for i in 0..arr.len() {
      items.push(to_prop_value(&arr.get::<Value>(i)?)?);
    }
    PropValue::List(items)
  } else if value.as_function().is_some() {
    // Functions (event handlers) are bound in the JS renderer, not marshalled as
    // data; ignore any that reach here.
    PropValue::Null
  } else if let Some(items) = float_array_items(value) {
    PropValue::List(items)
  } else if let Some(obj) = value.as_object() {
    // Arrays and functions are already handled above, so this is a plain object:
    // marshal its own enumerable keys into a Map, recursing on each value.
    let entries = obj
      .props::<String, Value>()
      .map(|entry| {
        let (k, v) = entry?;
        Ok((k, to_prop_value(&v)?))
      })
      .collect::<rquickjs::Result<Vec<_>>>()?;
    PropValue::Map(entries)
  } else {
    PropValue::Null
  })
}

// A Float32Array/Float64Array marshals as a list of numbers, so the flat
// coordinate props (line `points`) take either. Typed arrays are objects, not
// arrays, so without this they would fall into the Map branch as index-keyed
// entries. The bytes are read through as_bytes (None for a detached buffer,
// which marshals as an empty list) rather than AsRef<[T]>, which panics on
// one. Other typed arrays keep falling through.
fn float_array_items(value: &Value<'_>) -> Option<Vec<PropValue>> {
  let obj = value.as_object()?;
  if let Some(ta) = obj.as_typed_array::<f32>() {
    let bytes = ta.as_bytes().unwrap_or(&[]);
    return Some(
      bytes.chunks_exact(4).map(|c| PropValue::Number(f32::from_ne_bytes([c[0], c[1], c[2], c[3]]) as f64)).collect(),
    );
  }
  if let Some(ta) = obj.as_typed_array::<f64>() {
    let bytes = ta.as_bytes().unwrap_or(&[]);
    return Some(
      bytes
        .chunks_exact(8)
        .map(|c| PropValue::Number(f64::from_ne_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])))
        .collect(),
    );
  }
  None
}

// The font options measureText and prepareText share, onto a Text, through
// the JSX property decoders (one parser for fontWeight and friends). Throws
// on a value that does not decode.
fn apply_font_options<'js>(ctx: &Ctx<'js>, node: &mut Text, opts: &Object<'js>) -> rquickjs::Result<()> {
  for name in ["fontFamily", "fontSize", "fontStyle", "fontWeight", "lineHeight", "maxLines"] {
    let value: Value<'js> = opts.get(name)?;
    if value.is_undefined() {
      continue;
    }
    super::properties::apply_font_options(node, name, &to_prop_value(&value)?)
      .map_err(|msg| rquickjs::Exception::throw_message(ctx, &msg))?;
  }
  Ok(())
}

// The `runs` option of prepareText: styled ranges in JS (UTF-16) offsets over
// `text`, each object carrying the font options it overrides on `base`.
// Returned in byte offsets for alloy; throws on a range that is out of
// order, overlapping, empty or outside the text.
fn prepared_runs<'js>(
  ctx: &Ctx<'js>,
  text: &str,
  base: &Text,
  list: rquickjs::Array<'js>,
) -> rquickjs::Result<Vec<PreparedRun>> {
  // UTF-16 offset -> byte offset, rounding up to the next char boundary.
  let mut bytes = Vec::with_capacity(text.len() + 1);
  for (at, ch) in text.char_indices() {
    for _ in 0..ch.len_utf16() {
      bytes.push(at);
    }
  }
  bytes.push(text.len());
  let mut runs: Vec<PreparedRun> = Vec::with_capacity(list.len());
  for entry in list.iter::<Object<'js>>() {
    let entry = entry?;
    let start = entry.get::<_, f64>("start").unwrap_or(-1.0);
    let end = entry.get::<_, f64>("end").unwrap_or(-1.0);
    let previous_end = runs.last().map_or(0, |r| r.end);
    let len = (bytes.len() - 1) as f64;
    let in_text = start >= 0.0 && end > start && end <= len;
    let (start, end) = if in_text { (bytes[start as usize], bytes[end as usize]) } else { (0, 0) };
    if !in_text || start >= end || start < previous_end {
      return Err(rquickjs::Exception::throw_message(
        ctx,
        &format!("prepareText: runs must be sorted, disjoint, non-empty ranges within the text (run {})", runs.len()),
      ));
    }
    let mut node = base.clone();
    apply_font_options(ctx, &mut node, &entry)?;
    runs.push(PreparedRun { start, end, style: node.run_style() });
  }
  Ok(runs)
}

struct TextSize {
  width: f32,
  height: f32,
}

impl<'js> IntoJs<'js> for TextSize {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("width", self.width)?;
    obj.set("height", self.height)?;
    Ok(obj.into_value())
  }
}

// Rect lives in alloy (engine-free), so the rquickjs IntoJs conversion cannot
// be a trait impl on it (orphan rule). Wrap it locally for marshalling; the
// nested origin/size flattens to the JS `{x, y, width, height}` shape here.
struct JsBoundingBox(Rect);

impl<'js> IntoJs<'js> for JsBoundingBox {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("x", self.0.origin.x)?;
    obj.set("y", self.0.origin.y)?;
    obj.set("width", self.0.size.width)?;
    obj.set("height", self.0.size.height)?;
    Ok(obj.into_value())
  }
}

// The packed 0xRRGGBBAA form parseColor hands back to JS (the inverse of
// properties::packed_to_color).
fn color_to_packed(c: alloy::impellers::Color) -> u32 {
  let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
  (b(c.red) << 24) | (b(c.green) << 16) | (b(c.blue) << 8) | b(c.alpha)
}

/// Emit one "transitionEnd" engine event per settled track, payload
/// `{ target, property }` (JSX property name). `tick` calls this right after
/// the frame's transition advance; the JS side routes each to the node's
/// onTransitionEnd handler.
pub(crate) fn emit_transition_ends(ctx: &Ctx<'_>, settled: &[(u64, alloy::rendertree::AnimProp)]) {
  for &(node, prop) in settled {
    let obj = Object::new(ctx.clone()).expect("create transitionEnd object");
    obj.set("target", node).expect("set target");
    obj
      .set("property", super::properties::transition::anim_prop_name(prop))
      .expect("set property");
    crate::emit_event(ctx, "transitionEnd", obj);
  }
}

/// The render tree handle the runner's draw bridge (`srt:render`) reads for
/// the frame build (commit, layout, paint, finish, with its JS hooks between
/// the phases). Every other reader goes through the functions below.
#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

/// Stamp the transition animation clock with this frame's app time (the
/// runner, once per frame before the frame's JS runs, beside the spatial
/// arena's stamp): property writes during the flush start their tracks at
/// this time and `tick` advances to it. No-op before the GUI is installed.
pub fn stamp_clock(ctx: &Ctx<'_>, now_ms: f64) {
  if let Some(tree) = ctx.userdata::<SharedRenderTree>() {
    tree.0.borrow_mut().set_transition_now(now_ms);
  }
}

/// Advance every running transition track to the stamped clock and report
/// the settled ones to JS as "transitionEnd" events. Returns whether a track
/// is still running (frame demand for the runner's gate). The tree borrow is
/// released before the handlers run: they call back in.
pub fn tick(ctx: &Ctx<'_>) -> bool {
  let Some(tree) = ctx.userdata::<SharedRenderTree>() else {
    return false;
  };
  let active = tree.0.borrow_mut().advance_transitions();
  let settled = tree.0.borrow_mut().take_settled_transitions();
  if !settled.is_empty() {
    emit_transition_ends(ctx, &settled);
  }
  active
}

/// Mounted and total node counts (the runner's stats query). None before the
/// GUI is installed.
pub fn node_counts(ctx: &Ctx<'_>) -> Option<(usize, usize)> {
  let tree = ctx.userdata::<SharedRenderTree>()?;
  let tree = tree.0.borrow();
  Some((tree.mounted_count(), tree.node_count()))
}

/// Run `f` over the render tree (the runner's tree query). None before the
/// GUI is installed.
pub fn with_tree<R>(ctx: &Ctx<'_>, f: impl FnOnce(&RenderTree) -> R) -> Option<R> {
  let tree = ctx.userdata::<SharedRenderTree>()?;
  let result = f(&tree.0.borrow());
  Some(result)
}

// State the `flux:rendertree` module binds, stashed in userdata by `store_state`
// before any import so the module's `evaluate` can build its exports.
#[derive(Clone, JsLifetime)]
struct RenderTreeState(#[qjs(skip_trace)] Rc<RenderTreeInner>);

struct RenderTreeInner {
  tree: Rc<RefCell<RenderTree>>,
  platform: Arc<PlatformContext>,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  atx: AlloyContext,
  // The direct draw path's frame driver (see `render`): one per context, so
  // consecutive `render()` calls reuse the retained display list.
  render_driver: RefCell<FrameDriver>,
}

/// Create the shared render tree and stash the state the `flux:rendertree`
/// module binds, before any import. Also stores `SharedRenderTree`, which the
/// runner's draw bridge (`srt:render`) reads directly.
pub(crate) fn store_state(
  ctx: &Ctx<'_>,
  tree: RenderTree,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
) {
  let shared = SharedRenderTree(Rc::new(RefCell::new(tree)));
  ctx.store_userdata(shared.clone()).expect("store render tree");
  let inner =
    RenderTreeInner { tree: shared.0, platform, alloy_cmd_tx, atx, render_driver: RefCell::new(FrameDriver::new()) };
  ctx.store_userdata(RenderTreeState(Rc::new(inner))).expect("store rendertree state");
}

fn state(ctx: &Ctx<'_>) -> Rc<RenderTreeInner> {
  ctx.userdata::<RenderTreeState>().expect("rendertree state userdata").0.clone()
}

/// The `flux:rendertree` module: the render-tree bridge the renderer drives to
/// build and mutate the native tree (create/insert/delete nodes, write
/// properties, query layout, measure text). Marshalling only - all domain logic
/// lives in alloy's rendertree. Displaying the built tree is the runner's
/// concern (`srt:render`), not part of this module.
pub struct RenderTreeModule;

impl ModuleDef for RenderTreeModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createRoot")?;
    decl.declare("setRoot")?;
    decl.declare("createNode")?;
    decl.declare("detachNode")?;
    decl.declare("destroyNode")?;
    decl.declare("insertNode")?;
    decl.declare("setProperty")?;
    decl.declare("setEventInterest")?;
    decl.declare("requestFrame")?;
    decl.declare("render")?;
    decl.declare("setTextInputActive")?;
    decl.declare("setPointerLock")?;
    decl.declare("measureText")?;
    decl.declare("prepareText")?;
    decl.declare("getBoundingBox")?;
    decl.declare("getBoundingBoxViewport")?;
    decl.declare("getLayoutBox")?;
    decl.declare("snapshotTexture")?;
    decl.declare("parseColor")?;
    decl.declare("mixColors")?;
    decl.declare("brightness")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("createRoot", Function::new(ctx.clone(), create_root)?)?;
    exports.export("setRoot", Function::new(ctx.clone(), set_root)?)?;
    exports.export("createNode", Function::new(ctx.clone(), create_node)?)?;
    exports.export("detachNode", Function::new(ctx.clone(), detach_node)?)?;
    exports.export("destroyNode", Function::new(ctx.clone(), destroy_node)?)?;
    exports.export("insertNode", Function::new(ctx.clone(), insert_node)?)?;
    exports.export("setProperty", Function::new(ctx.clone(), set_property)?)?;
    exports.export("setEventInterest", Function::new(ctx.clone(), set_event_interest)?)?;
    exports.export("requestFrame", Function::new(ctx.clone(), request_frame)?)?;
    exports.export("render", Function::new(ctx.clone(), render)?)?;
    exports.export("setTextInputActive", Function::new(ctx.clone(), set_text_input_active)?)?;
    exports.export("setPointerLock", Function::new(ctx.clone(), set_pointer_lock)?)?;
    exports.export("measureText", Function::new(ctx.clone(), measure_text)?)?;
    exports.export("prepareText", Function::new(ctx.clone(), prepare_text)?)?;
    exports.export("getBoundingBox", Function::new(ctx.clone(), get_bounding_box)?)?;
    exports.export("getBoundingBoxViewport", Function::new(ctx.clone(), get_bounding_box_viewport)?)?;
    exports.export("getLayoutBox", Function::new(ctx.clone(), get_layout_box)?)?;
    exports.export("snapshotTexture", Function::new(ctx.clone(), snapshot_texture)?)?;
    exports.export("parseColor", Function::new(ctx.clone(), parse_color)?)?;
    exports.export("mixColors", Function::new(ctx.clone(), mix_colors)?)?;
    exports.export("brightness", Function::new(ctx.clone(), brightness)?)?;
    Ok(())
  }
}

fn create_root(ctx: Ctx<'_>, id: u64) {
  let s = state(&ctx);
  let mut tree = s.tree.borrow_mut();
  tree.create_node(id, Window::default().with_layout());
  tree.root = Some(id);
  s.platform.request_frame();
}

fn set_root(ctx: Ctx<'_>, id: u64) {
  let s = state(&ctx);
  s.tree.borrow_mut().set_root(id);
  s.platform.request_frame();
}

fn create_node(ctx: Ctx<'_>, id: u64, kind: String) -> rquickjs::Result<()> {
  let Some(element) = Element::from_kind(&kind) else {
    return Err(rquickjs::Exception::throw_message(&ctx, &format!("Unknown node kind: <{kind}>")));
  };
  let s = state(&ctx);
  s.tree.borrow_mut().create_node(id, element);
  s.platform.request_frame();
  Ok(())
}

fn detach_node(ctx: Ctx<'_>, parent_id: u64, node_id: u64) {
  let s = state(&ctx);
  s.tree.borrow_mut().detach_node(parent_id, node_id);
  s.platform.request_frame();
}

fn destroy_node(ctx: Ctx<'_>, node_id: u64) {
  let s = state(&ctx);
  s.tree.borrow_mut().destroy_node(node_id);
  s.platform.request_frame();
}

fn insert_node(ctx: Ctx<'_>, parent_id: u64, node_id: u64, anchor_id: OptArg<u64>) -> rquickjs::Result<()> {
  let s = state(&ctx);
  s.tree
    .borrow_mut()
    .insert_node(parent_id, node_id, anchor_id.0)
    .map_err(|msg| rquickjs::Exception::throw_message(&ctx, &msg))?;
  s.platform.request_frame();
  Ok(())
}

fn set_property(ctx: Ctx<'_>, node_id: u64, property: String, value: Value<'_>) -> rquickjs::Result<()> {
  SETPROP_COUNT.with(|c| c.set(c.get() + 1));
  let s = state(&ctx);
  // Native transitions: a numeric write to an animatable property on
  // an element declaring a transition for it becomes a track target
  // (Rust interpolates from the next frame on) instead of a snap. A
  // false consumes nothing and has cancelled any running track for
  // the pair, so the normal write below is authoritative.
  if let Some(prop) = super::properties::transition::anim_prop(&property) {
    // Colors arrive as raw CSS strings (or packed 0xRRGGBBAA numbers
    // for compatibility); everything else animatable is a plain
    // scalar. Anything else (null, a gradient object, an unparsable
    // string) never animates - the normal write path raises the
    // proper error for the bad string.
    let target = match prop {
      AnimProp::Color => {
        let packed = value.as_number().map(|n| super::properties::packed_to_color(n as u32));
        let parsed = || {
          let css = value.as_string()?.to_string().ok()?;
          alloy::color::parse_css(&css).ok()
        };
        packed.or_else(parsed).map(AnimValue::Color)
      }
      _ => value.as_number().map(|n| AnimValue::Scalar(n as f32)),
    };
    if s.tree.borrow_mut().transition_write(node_id, prop, target) {
      s.platform.request_frame();
      return Ok(());
    }
  }
  let value = to_prop_value(&value)?;
  s.tree
    .borrow_mut()
    .try_edit(node_id, |el| {
      super::properties::apply_jsx(el, &property, &value, &s.alloy_cmd_tx, &|id, params| {
        s.atx.set_target_params(id, params)
      })
    })
    .map_err(|msg| rquickjs::Exception::throw_message(&ctx, &msg))?;
  s.platform.request_frame();
  Ok(())
}

// Pure dispatch metadata (which pointer deliveries the node's handlers
// want; see alloy's EventInterest): no visual change, so no frame request.
fn set_event_interest(ctx: Ctx<'_>, node_id: u64, bits: u32) -> rquickjs::Result<()> {
  if bits & !EventInterest::KNOWN != 0 {
    return Err(rquickjs::Exception::throw_message(
      &ctx,
      &format!("setEventInterest: unknown bits 0x{:x}", bits & !EventInterest::KNOWN),
    ));
  }
  state(&ctx).tree.borrow_mut().edit(node_id, |el| {
    el.set_event_interest(EventInterest(bits));
    Damage::None
  });
  Ok(())
}

fn request_frame(ctx: Ctx<'_>) {
  state(&ctx).platform.request_frame();
}

// The direct draw path: put the current tree on screen now. Lets a
// flux + alloy app render without the runner's frame loop. Runs alloy's
// frame protocol, so when nothing changed since the last call the retained
// display list is re-presented instead of rebuilt (fresh texture contents
// are still sampled at the raster flush); the call itself is the demand,
// so the driver's gate never skips it.
fn render(ctx: Ctx<'_>) {
  let s = state(&ctx);
  let mut driver = s.render_driver.borrow_mut();
  let Some(frame) = driver.begin(&s.platform, true) else { return };
  let commit = frame.commit(&mut s.tree.borrow_mut(), &s.platform, &s.atx);
  match commit {
    Err(()) => log::warn!("render: render thread unavailable, dropping frame"),
    Ok(Commit::Reused { .. }) => {}
    Ok(Commit::Build(mut b)) => {
      // The paint phase runs layout itself; the direct path has no
      // between-phase hooks to sequence.
      b.paint(&mut s.tree.borrow_mut(), &s.platform, &s.atx);
      if b.finish(&s.tree.borrow(), &s.platform, &s.atx).is_err() {
        log::warn!("render: render thread unavailable, dropping frame");
      }
    }
  }
}

fn set_text_input_active(ctx: Ctx<'_>, active: bool, hints: OptArg<Object<'_>>) {
  let mut options = alloy::TextInputOptions::default();
  if let Some(h) = hints.0 {
    if let Ok(v) = h.get::<_, String>("type") {
      options.input_type = match v.as_str() {
        "text" => Some(alloy::TextInputType::Text),
        "name" => Some(alloy::TextInputType::Name),
        "email" => Some(alloy::TextInputType::Email),
        "username" => Some(alloy::TextInputType::Username),
        "password" => Some(alloy::TextInputType::PasswordHidden),
        "number" => Some(alloy::TextInputType::Number),
        "pin" => Some(alloy::TextInputType::NumberPasswordHidden),
        _ => None,
      };
    }
    if let Ok(v) = h.get::<_, String>("capitalize") {
      options.capitalize = match v.as_str() {
        "none" => Some(alloy::TextCapitalization::None),
        "sentences" => Some(alloy::TextCapitalization::Sentences),
        "words" => Some(alloy::TextCapitalization::Words),
        "letters" => Some(alloy::TextCapitalization::Letters),
        _ => None,
      };
    }
    if let Ok(v) = h.get::<_, bool>("autocorrect") {
      options.autocorrect = Some(v);
    }
    if let Ok(v) = h.get::<_, bool>("multiline") {
      options.multiline = Some(v);
    }
  }
  state(&ctx).alloy_cmd_tx.send(alloy::AlloyCommand::SetTextInputActive(active, options)).ok();
}

fn set_pointer_lock(ctx: Ctx<'_>, locked: bool) {
  state(&ctx).alloy_cmd_tx.send(alloy::AlloyCommand::SetPointerLock(locked)).ok();
}

fn measure_text<'js>(ctx: Ctx<'js>, text: String, options: OptArg<Object<'js>>) -> rquickjs::Result<TextSize> {
  let mut node = Text::default();
  node.set_plain_text(text);
  if let Some(opts) = options.0 {
    apply_font_options(&ctx, &mut node, &opts)?;
  }
  let s = state(&ctx);
  let size = node.measure(&MeasureContext {
    platform: &s.platform,
    alloy: &*s.atx,
    known: Size { width: None, height: None },
    available: Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent },
  });
  Ok(TextSize { width: size.width, height: size.height })
}

fn prepare_text<'js>(ctx: Ctx<'js>, text: String, options: OptArg<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let mut node = Text::default();
  let mut carets = false;
  let mut runs = Vec::new();
  if let Some(opts) = options.0 {
    apply_font_options(&ctx, &mut node, &opts)?;
    carets = opts.get::<_, bool>("carets").unwrap_or(false);
    if let Ok(list) = opts.get::<_, rquickjs::Array<'js>>("runs") {
      runs = prepared_runs(&ctx, &text, &node, list)?;
    }
  }
  let s = state(&ctx);
  let units = prepare_units(&s.platform, &text, &node.run_style(), &runs, carets);
  let array = rquickjs::Array::new(ctx.clone())?;
  // Byte offsets to UTF-16 (JS string) offsets, incrementally: units tile
  // the text in order.
  let (mut byte, mut utf16) = (0usize, 0usize);
  let mut to_utf16 = |at: usize| {
    utf16 += text[byte..at].encode_utf16().count();
    byte = at;
    utf16
  };
  for (i, unit) in units.into_iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    let start = to_utf16(unit.start) as u32;
    obj.set("start", start)?;
    obj.set("end", to_utf16(unit.end) as u32)?;
    if let Some(stops) = &unit.carets {
      let array = rquickjs::Array::new(ctx.clone())?;
      for (j, stop) in stops.iter().enumerate() {
        let o = Object::new(ctx.clone())?;
        o.set("offset", start + stop.offset)?;
        o.set("x", stop.x)?;
        array.set(j, o)?;
      }
      obj.set("carets", array)?;
    }
    obj.set("text", unit.text)?;
    obj.set("advance", unit.metrics.advance)?;
    obj.set("width", unit.metrics.ink_width)?;
    obj.set("ascent", unit.metrics.ascent)?;
    obj.set("descent", unit.metrics.descent)?;
    obj.set("hardBreak", unit.hard_break)?;
    obj.set("glue", unit.glue)?;
    if let Some(run) = unit.run {
      obj.set("run", run as u32)?;
    }
    array.set(i, obj)?;
  }
  let prepared = Object::new(ctx.clone())?;
  prepared.set("text", text)?;
  prepared.set("units", array)?;
  Ok(prepared)
}

fn get_bounding_box(ctx: Ctx<'_>, id: u64) -> Option<JsBoundingBox> {
  state(&ctx).tree.borrow().bounding_box(id).map(JsBoundingBox)
}

fn get_bounding_box_viewport(ctx: Ctx<'_>, id: u64) -> Option<JsBoundingBox> {
  state(&ctx).tree.borrow().bounding_box_viewport(id).map(JsBoundingBox)
}

fn get_layout_box(ctx: Ctx<'_>, id: u64) -> Option<JsBoundingBox> {
  state(&ctx).tree.borrow().layout_box(id).map(JsBoundingBox)
}

fn snapshot_texture(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<u64> {
  let s = state(&ctx);
  let result = s.tree.borrow().snapshot_texture(id, &s.atx);
  result.map_err(|msg| rquickjs::Exception::throw_message(&ctx, &msg))
}

// Color utilities over alloy's color module (one owner for the CSS
// grammar and the perceptual math; okf/backlog/css-colors-in-rust.md).
// parseColor returns the same packed 0xRRGGBBAA number the JS parser
// used to, and throws on an invalid string.
// Returned as f64: a u32 return would marshal through a signed 32-bit
// int, turning any color with red >= 0x80 negative on the JS side.
fn parse_color(ctx: Ctx<'_>, color: String) -> rquickjs::Result<f64> {
  alloy::color::parse_css(&color)
    .map(|c| color_to_packed(c) as f64)
    .map_err(|msg| rquickjs::Exception::throw_message(&ctx, &msg))
}

// Mixes in oklab; `t` is the fraction of `b`. Returns a hex string
// (#rrggbb, with an alpha byte only when the mix is translucent).
fn mix_colors(ctx: Ctx<'_>, a: String, b: String, t: f64) -> rquickjs::Result<String> {
  let err = |msg: String| rquickjs::Exception::throw_message(&ctx, &msg);
  let a = alloy::color::parse_css(&a).map_err(err)?;
  let b = alloy::color::parse_css(&b).map_err(err)?;
  let m = alloy::color::mix(a, b, t as f32);
  let packed = color_to_packed(m);
  let (r, g, bl, al) = (packed >> 24 & 0xFF, packed >> 16 & 0xFF, packed >> 8 & 0xFF, packed & 0xFF);
  Ok(if al == 0xFF { format!("#{r:02x}{g:02x}{bl:02x}") } else { format!("#{r:02x}{g:02x}{bl:02x}{al:02x}") })
}

fn brightness(ctx: Ctx<'_>, color: String) -> rquickjs::Result<f64> {
  alloy::color::parse_css(&color)
    .map(|c| alloy::color::brightness(c) as f64)
    .map_err(|msg| rquickjs::Exception::throw_message(&ctx, &msg))
}
