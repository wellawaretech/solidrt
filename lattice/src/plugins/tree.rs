use alloy::impellers::{FontStyle, FontWeight};
use flux::rquickjs::{function::Opt, Ctx, Function, IntoJs, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use taffy::prelude::*;

use crate::plugins::value::PropValue;
use crate::rendertree::{BoundingBox, Element, Measurable, MeasureContext, PlatformContext, RenderTree, Text, Window};
use crate::AlloyContext;

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
  } else {
    PropValue::Null
  }
}

struct TextSize {
  width: f32,
  height: f32,
}

impl<'js> IntoJs<'js> for TextSize {
  fn into_js(self, ctx: &Ctx<'js>) -> flux::rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("width", self.width)?;
    obj.set("height", self.height)?;
    Ok(obj.into_value())
  }
}

impl<'js> IntoJs<'js> for BoundingBox {
  fn into_js(self, ctx: &Ctx<'js>) -> flux::rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("x", self.x)?;
    obj.set("y", self.y)?;
    obj.set("width", self.width)?;
    obj.set("height", self.height)?;
    Ok(obj.into_value())
  }
}

#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

pub fn init(
  ctx: &Ctx<'_>,
  tree: RenderTree,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
) {
  let shared = SharedRenderTree(Rc::new(RefCell::new(tree)));
  ctx.store_userdata(shared.clone()).expect("store render tree");

  let tree_ref = shared.0.clone();
  let create_root = Function::new(ctx.clone(), move |id: u64| {
    let mut tree = tree_ref.borrow_mut();
    tree.create_node(id, Window::default().with_layout());
    tree.root = Some(id);
  })
  .expect("create createRoot");

  let tree_ref = shared.0.clone();
  let create_node = Function::new(ctx.clone(), move |id: u64, kind: String| {
    tree_ref.borrow_mut().create_node(id, Element::from_kind(&kind));
  })
  .expect("create createNode");

  let tree_ref = shared.0.clone();
  let delete_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64| {
    tree_ref.borrow_mut().delete_node(parent_id, node_id);
  })
  .expect("create deleteNode");

  let tree_ref = shared.0.clone();
  let insert_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64, anchor_id: Opt<u64>| {
    tree_ref.borrow_mut().insert_node(parent_id, node_id, anchor_id.0);
  })
  .expect("create insertNode");

  let tree_ref = shared.0.clone();
  let cmd_tx = alloy_cmd_tx.clone();
  let set_property = Function::new(ctx.clone(), move |node_id: u64, property: String, value: Value<'_>| {
    let value = to_prop_value(&value);
    let mut tree = tree_ref.borrow_mut();
    let invalidate = super::properties::apply_jsx(tree.element_mut(node_id), &property, &value, &cmd_tx);
    if invalidate {
      tree.invalidate_cache(node_id);
    }
  })
  .expect("create setProperty");

  let tree_ref = shared.0.clone();
  let get_bounding_box =
    Function::new(ctx.clone(), move |id: u64| -> Option<BoundingBox> { tree_ref.borrow().bounding_box(id) })
      .expect("create getBoundingBox");

  let cmd_tx = alloy_cmd_tx.clone();
  let set_text_input_active = Function::new(ctx.clone(), move |active: bool| {
    cmd_tx.send(alloy::AlloyCommand::SetTextInputActive(active)).ok();
  })
  .expect("create setTextInputActive");

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
  })
  .expect("create measureText");

  let ffi = Object::new(ctx.clone()).expect("create ffi object");
  ffi.set("createRoot", create_root).expect("set ffi.createRoot");
  ffi.set("createNode", create_node).expect("set ffi.createNode");
  ffi.set("deleteNode", delete_node).expect("set ffi.deleteNode");
  ffi.set("insertNode", insert_node).expect("set ffi.insertNode");
  ffi.set("setProperty", set_property).expect("set ffi.setProperty");
  ffi.set("setTextInputActive", set_text_input_active).expect("set ffi.setTextInputActive");
  ffi.set("measureText", measure_text).expect("set ffi.measureText");
  ffi.set("getBoundingBox", get_bounding_box).expect("set ffi.getBoundingBox");

  ctx.globals().set("ffi", ffi).expect("set ffi global");
}
