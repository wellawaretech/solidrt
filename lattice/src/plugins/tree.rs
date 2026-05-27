use alloy::impellers::{FontStyle, FontWeight};
use flux::rquickjs::{function::Opt, Ctx, Function, IntoJs, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use taffy::prelude::*;

use crate::AlloyContext;
use crate::rendertree::layout::properties;
use crate::rendertree::{ElementKind, Measurable, MeasureContext, Path, PlatformContext, Rectangle, RenderTree, Span, Text, Texture, View, Window};

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

#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

pub fn init(ctx: &Ctx<'_>, tree: RenderTree, alloy_cmd_tx: Sender<alloy::AlloyCommand>, platform: Arc<PlatformContext>, atx: AlloyContext) {
  let shared = SharedRenderTree(Rc::new(RefCell::new(tree)));
  ctx.store_userdata(shared.clone()).unwrap();

  let tree_ref = shared.0.clone();
  let create_root = Function::new(ctx.clone(), move |id: u64| {
    let mut tree = tree_ref.borrow_mut();
    tree.create_node(id, Window::default().with_layout());
    tree.root = Some(id);
  })
  .unwrap();

  let tree_ref = shared.0.clone();
  let create_node = Function::new(ctx.clone(), move |id: u64, kind: String| {
    let element = match kind.as_str() {
      "window" => panic!("use createRoot to create the root Window node"),
      "view" => View::default().with_layout(),
      "rect" => Rectangle::default().with_layout(),
      "d-rect" => Rectangle::default().no_layout(),
      "path" => Path::default().with_layout(),
      "d-path" => Path::default().no_layout(),
      "text" => Text::default().with_layout(),
      "span" => Span::default().no_layout(),
      "texture" => Texture::default().with_layout(),
      "d-texture" => Texture::default().no_layout(),
      _ => panic!("unknown node kind: {kind}"),
    };
    tree_ref.borrow_mut().create_node(id, element);
  })
  .unwrap();

  let tree_ref = shared.0.clone();
  let delete_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64| {
    tree_ref.borrow_mut().delete_node(parent_id, node_id);
  })
  .unwrap();

  let tree_ref = shared.0.clone();
  let insert_node = Function::new(
    ctx.clone(),
    move |parent_id: u64, node_id: u64, anchor_id: Opt<u64>| {
      tree_ref.borrow_mut().insert_node(parent_id, node_id, anchor_id.0);
    },
  )
  .unwrap();

  let tree_ref = shared.0.clone();
  let cmd_tx = alloy_cmd_tx.clone();
  let set_property = Function::new(ctx.clone(), move |node_id: u64, property: String, value: Value<'_>| {
    let mut tree = tree_ref.borrow_mut();
    let invalidate = {
      let element = tree.element_mut(node_id);
      let prop = property.as_str();
      let result = match &mut element.kind {
        ElementKind::Window(win) => win.set_property(prop, value.clone(), &cmd_tx),
        ElementKind::Rectangle(rect) => rect.set_property(prop, value.clone()),
        ElementKind::Path(path) => path.set_property(prop, value.clone()),
        ElementKind::Text(text) => text.set_property(prop, value.clone()),
        ElementKind::Span(span) => span.set_property(prop, value.clone()),
        ElementKind::View(view) => view.set_property(prop, value.clone()),
        ElementKind::Texture(tex) => tex.set_property(prop, value.clone()),
      };
      let result = result
        .or_else(|| element.kind.paint_mut().and_then(|paint| paint.set_property(prop, value.clone())));
      let result = result
        .or_else(|| element.style_mut().and_then(|style| properties::set_property(style, prop, value)));
      result.unwrap_or_else(|| panic!("unknown property '{property}'"))
    };
    if invalidate {
      tree.invalidate_cache(node_id);
    }
  })
  .unwrap();

  let cmd_tx = alloy_cmd_tx.clone();
  let set_text_input_active = Function::new(ctx.clone(), move |active: bool| {
    cmd_tx.send(alloy::AlloyCommand::SetTextInputActive(active)).ok();
  })
  .unwrap();

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
      if let Ok(v) = opts.get::<_, f64>("fontSize") { node.font_size = v as f32; }
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
      if let Ok(v) = opts.get::<_, f64>("maxLines") { node.max_lines = v as u32; }
    }

    let size = node.measure(&MeasureContext {
      platform: &measure_platform,
      alloy: &*measure_atx,
      known: Size { width: None, height: None },
      available: Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent },
    });
    TextSize { width: size.width, height: size.height }
  })
  .unwrap();

  let ffi = Object::new(ctx.clone()).unwrap();
  ffi.set("createRoot", create_root).unwrap();
  ffi.set("createNode", create_node).unwrap();
  ffi.set("deleteNode", delete_node).unwrap();
  ffi.set("insertNode", insert_node).unwrap();
  ffi.set("setProperty", set_property).unwrap();
  ffi.set("setTextInputActive", set_text_input_active).unwrap();
  ffi.set("measureText", measure_text).unwrap();

  ctx.globals().set("ffi", ffi).unwrap();
}
