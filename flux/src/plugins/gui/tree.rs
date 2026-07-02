use alloy::impellers::{FontStyle, FontWeight};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{function::Opt, Ctx, Function, IntoJs, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use taffy::prelude::*;

use super::AlloyContext;
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::{BoundingBox, Element, Measurable, MeasureContext, PlatformContext, RenderTree, Text, Window};

thread_local! {
  // setProperty (FFI prop write) calls since the last frame. Bumped in the
  // native setProperty handler below; read and reset each frame by the draw
  // bridge (in lattice) for the debug overlay. Lives on the single JS thread,
  // so no timing call crosses into JS.
  pub static SETPROP_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Marshals a JavaScript value into the engine-independent PropValue that
// rendertree setters consume. This is the FFI boundary: rquickjs types stay on
// this side of it.
fn to_prop_value(value: &Value<'_>) -> PropValue {
  if value.is_null() || value.is_undefined() {
    PropValue::Null
  } else if let Some(b) = value.as_bool() {
    PropValue::Bool(b)
  } else if let Some(n) = value.as_number() {
    PropValue::Number(n)
  } else if let Some(s) = value.as_string() {
    PropValue::Text(s.to_string().expect("property string must be valid UTF-8"))
  } else if let Some(arr) = value.as_array() {
    let mut items = Vec::with_capacity(arr.len());
    for i in 0..arr.len() {
      items.push(to_prop_value(&arr.get::<Value>(i).expect("array element must be a value")));
    }
    PropValue::List(items)
  } else if value.as_function().is_some() {
    // Functions (event handlers) are bound in the JS renderer, not marshalled as
    // data; ignore any that reach here.
    PropValue::Null
  } else if let Some(obj) = value.as_object() {
    // Arrays and functions are already handled above, so this is a plain object:
    // marshal its own enumerable keys into a Map, recursing on each value.
    let entries = obj
      .props::<String, Value>()
      .map(|entry| {
        let (k, v) = entry.expect("object property must be a key/value pair");
        (k, to_prop_value(&v))
      })
      .collect();
    PropValue::Map(entries)
  } else {
    PropValue::Null
  }
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

// BoundingBox lives in alloy (engine-free), so the rquickjs IntoJs conversion
// cannot be a trait impl on it (orphan rule). Wrap it locally for marshalling.
struct JsBoundingBox(BoundingBox);

impl<'js> IntoJs<'js> for JsBoundingBox {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("x", self.0.x)?;
    obj.set("y", self.0.y)?;
    obj.set("width", self.0.width)?;
    obj.set("height", self.0.height)?;
    Ok(obj.into_value())
  }
}

#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

// State the `flux:rendertree` module binds, stashed in userdata by `store_state`
// before any import so the module's `evaluate` can build its exports.
#[derive(Clone, JsLifetime)]
struct RenderTreeState(#[qjs(skip_trace)] Rc<RenderTreeInner>);

struct RenderTreeInner {
  tree: Rc<RefCell<RenderTree>>,
  platform: Arc<PlatformContext>,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  atx: AlloyContext,
}

/// Create the shared render tree and stash the state the `flux:rendertree`
/// module binds, before any import. Also stores `SharedRenderTree`, which the
/// runner's draw bridge (`srt:render`) reads directly.
pub fn store_state(
  ctx: &Ctx<'_>,
  tree: RenderTree,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
) {
  let shared = SharedRenderTree(Rc::new(RefCell::new(tree)));
  ctx.store_userdata(shared.clone()).expect("store render tree");
  ctx
    .store_userdata(RenderTreeState(Rc::new(RenderTreeInner { tree: shared.0, platform, alloy_cmd_tx, atx })))
    .expect("store rendertree state");
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
    decl.declare("createNode")?;
    decl.declare("detachNode")?;
    decl.declare("destroyNode")?;
    decl.declare("insertNode")?;
    decl.declare("setProperty")?;
    decl.declare("requestFrame")?;
    decl.declare("render")?;
    decl.declare("setTextInputActive")?;
    decl.declare("measureText")?;
    decl.declare("getBoundingBox")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let state = ctx.userdata::<RenderTreeState>().expect("rendertree state userdata");
    let tree = state.0.tree.clone();
    let platform = state.0.platform.clone();
    let alloy_cmd_tx = state.0.alloy_cmd_tx.clone();
    let atx = state.0.atx.clone();

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let create_root = Function::new(ctx.clone(), move |id: u64| {
      let mut tree = tree_ref.borrow_mut();
      tree.create_node(id, Window::default().with_layout());
      tree.root = Some(id);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let create_node = Function::new(ctx.clone(), move |id: u64, kind: String| {
      tree_ref.borrow_mut().create_node(id, Element::from_kind(&kind));
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let detach_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64| {
      tree_ref.borrow_mut().detach_node(parent_id, node_id);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let destroy_node = Function::new(ctx.clone(), move |node_id: u64| {
      tree_ref.borrow_mut().destroy_node(node_id);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let insert_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64, anchor_id: Opt<u64>| {
      tree_ref.borrow_mut().insert_node(parent_id, node_id, anchor_id.0);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let cmd_tx = alloy_cmd_tx.clone();
    let set_property = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, node_id: u64, property: String, value: Value<'_>| -> rquickjs::Result<()> {
        SETPROP_COUNT.with(|c| c.set(c.get() + 1));
        let value = to_prop_value(&value);
        let mut tree = tree_ref.borrow_mut();
        let invalidate =
          super::properties::apply_jsx(tree.element_mut(node_id), &property, &value, &cmd_tx).map_err(|msg| {
            ctx.throw(rquickjs::String::from_str(ctx.clone(), &msg).expect("create error string").into())
          })?;
        if invalidate {
          tree.invalidate_cache(node_id);
        }
        tree.sync_span_parent(node_id);
        platform_ref.request_frame();
        Ok(())
      },
    )?;

    let platform_ref = platform.clone();
    let request_frame = Function::new(ctx.clone(), move || platform_ref.request_frame())?;

    // The direct draw path: lay out, paint and submit the whole tree now. Lets a
    // flux + alloy app put its tree on screen without the runner's frame loop.
    let tree_ref = tree.clone();
    let render_platform = platform.clone();
    let render_atx = atx.clone();
    let render = Function::new(ctx.clone(), move || {
      alloy::rendertree::composite::render(&mut tree_ref.borrow_mut(), &render_platform, &render_atx);
    })?;

    let tree_ref = tree.clone();
    let get_bounding_box = Function::new(ctx.clone(), move |id: u64| -> Option<JsBoundingBox> {
      tree_ref.borrow().bounding_box(id).map(JsBoundingBox)
    })?;

    let cmd_tx = alloy_cmd_tx.clone();
    let set_text_input_active = Function::new(ctx.clone(), move |active: bool| {
      cmd_tx.send(alloy::AlloyCommand::SetTextInputActive(active)).ok();
    })?;

    let measure_platform = platform.clone();
    let measure_atx = atx.clone();
    let measure_text = Function::new(ctx.clone(), move |text: String, options: Opt<Object<'_>>| -> TextSize {
      let mut node = Text::default();
      node.computed_text = text;

      if let Some(opts) = options.0 {
        if let Ok(v) = opts.get::<_, String>("fontFamily") {
          node.font_family = match v.as_str() {
            "mono" => "Noto Sans Mono".to_string(),
            "sans" => "Noto Sans".to_string(),
            other => other.to_string(),
          };
        }
        if let Ok(v) = opts.get::<_, f64>("fontSize") {
          node.font_size = v as f32;
        }
        if let Ok(v) = opts.get::<_, String>("fontStyle") {
          node.font_style = match v.as_str() {
            "italic" => FontStyle::Italic,
            _ => FontStyle::Normal,
          };
        }
        if let Ok(v) = opts.get::<_, f64>("fontWeight") {
          node.font_weight = match v as u32 {
            100 => FontWeight::Thin,
            200 => FontWeight::ExtraLight,
            300 => FontWeight::Light,
            500 => FontWeight::Medium,
            600 => FontWeight::SemiBold,
            700 => FontWeight::Bold,
            800 => FontWeight::ExtraBold,
            900 => FontWeight::Black,
            _ => FontWeight::Regular,
          };
        }
        if let Ok(v) = opts.get::<_, f64>("maxLines") {
          node.max_lines = v as u32;
        }
      }

      let size = node.measure(&MeasureContext {
        platform: &measure_platform,
        alloy: &*measure_atx,
        known: Size { width: None, height: None },
        available: Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent },
      });
      TextSize { width: size.width, height: size.height }
    })?;

    exports.export("createRoot", create_root)?;
    exports.export("createNode", create_node)?;
    exports.export("detachNode", detach_node)?;
    exports.export("destroyNode", destroy_node)?;
    exports.export("insertNode", insert_node)?;
    exports.export("setProperty", set_property)?;
    exports.export("requestFrame", request_frame)?;
    exports.export("render", render)?;
    exports.export("setTextInputActive", set_text_input_active)?;
    exports.export("measureText", measure_text)?;
    exports.export("getBoundingBox", get_bounding_box)?;
    Ok(())
  }
}
