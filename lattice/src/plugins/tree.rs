use flux::rquickjs::{function::Opt, Ctx, Function, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;

use alloy::impellers::{BlendMode, Color, DrawStyle, StrokeCap, StrokeJoin};

use crate::rendertree::{ElementKind, Rectangle, RenderTree, Span, Text, View, Window};

#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

pub fn init(ctx: &Ctx<'_>, tree: RenderTree) {
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
      "text" => Text::default().with_layout(),
      "span" => Span::default().no_layout(),
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
  let set_property = Function::new(ctx.clone(), move |node_id: u64, property: String, value: Value<'_>| {
    let mut tree = tree_ref.borrow_mut();
    let invalidate = {
      let element = tree.element_mut(node_id);
      match (&mut element.kind, property.as_str()) {
        (kind, "color") => {
          let rgba = value.get::<f64>().expect("color must be a number") as u32;
          kind.paint_mut().expect("node kind has no paint").color = Color::new_srgba(
            ((rgba >> 24) & 0xFF) as f32 / 255.0,
            ((rgba >> 16) & 0xFF) as f32 / 255.0,
            ((rgba >> 8) & 0xFF) as f32 / 255.0,
            (rgba & 0xFF) as f32 / 255.0,
          );
          false
        }
        (kind, "strokeWidth") => {
          kind.paint_mut().expect("node kind has no paint").stroke_width = value.get::<f64>().expect("strokeWidth must be a number") as f32;
          false
        }
        (kind, "strokeMiter") => {
          kind.paint_mut().expect("node kind has no paint").stroke_miter = value.get::<f64>().expect("strokeMiter must be a number") as f32;
          false
        }
        (kind, "drawStyle") => {
          kind.paint_mut().expect("node kind has no paint").draw_style = match value.get::<String>().expect("drawStyle must be a string").as_str() {
            "fill" => DrawStyle::Fill,
            "stroke" => DrawStyle::Stroke,
            "strokeAndFill" => DrawStyle::StrokeAndFill,
            v => panic!("unknown drawStyle '{v}'"),
          };
          false
        }
        (kind, "strokeCap") => {
          kind.paint_mut().expect("node kind has no paint").stroke_cap = match value.get::<String>().expect("strokeCap must be a string").as_str() {
            "butt" => StrokeCap::Butt,
            "round" => StrokeCap::Round,
            "square" => StrokeCap::Square,
            v => panic!("unknown strokeCap '{v}'"),
          };
          false
        }
        (kind, "strokeJoin") => {
          kind.paint_mut().expect("node kind has no paint").stroke_join = match value.get::<String>().expect("strokeJoin must be a string").as_str() {
            "miter" => StrokeJoin::Miter,
            "round" => StrokeJoin::Round,
            "bevel" => StrokeJoin::Bevel,
            v => panic!("unknown strokeJoin '{v}'"),
          };
          false
        }
        (kind, "blendMode") => {
          kind.paint_mut().expect("node kind has no paint").blend_mode = match value.get::<String>().expect("blendMode must be a string").as_str() {
            "clear" => BlendMode::Clear,
            "source" => BlendMode::Source,
            "destination" => BlendMode::Destination,
            "sourceOver" => BlendMode::SourceOver,
            "destinationOver" => BlendMode::DestinationOver,
            "sourceIn" => BlendMode::SourceIn,
            "destinationIn" => BlendMode::DestinationIn,
            "sourceOut" => BlendMode::SourceOut,
            "destinationOut" => BlendMode::DestinationOut,
            "sourceATop" => BlendMode::SourceATop,
            "destinationATop" => BlendMode::DestinationATop,
            "xor" => BlendMode::Xor,
            "plus" => BlendMode::Plus,
            "modulate" => BlendMode::Modulate,
            "screen" => BlendMode::Screen,
            "overlay" => BlendMode::Overlay,
            "darken" => BlendMode::Darken,
            "lighten" => BlendMode::Lighten,
            "colorDodge" => BlendMode::ColorDodge,
            "colorBurn" => BlendMode::ColorBurn,
            "hardLight" => BlendMode::HardLight,
            "softLight" => BlendMode::SoftLight,
            "difference" => BlendMode::Difference,
            "exclusion" => BlendMode::Exclusion,
            "multiply" => BlendMode::Multiply,
            "hue" => BlendMode::Hue,
            "saturation" => BlendMode::Saturation,
            "color" => BlendMode::Color,
            "luminosity" => BlendMode::Luminosity,
            v => panic!("unknown blendMode '{v}'"),
          };
          false
        }
        (ElementKind::Window(win), prop) => {
          win.set_property(prop, value).unwrap_or_else(|| panic!("unknown property '{property}'"))
        }
        (ElementKind::Rectangle(rect), prop) => {
          rect.set_property(prop, value).unwrap_or_else(|| panic!("unknown property '{property}'"))
        }
        (ElementKind::Text(text), prop) => {
          text.set_property(prop, value).unwrap_or_else(|| panic!("unknown property '{property}'"))
        }
        (ElementKind::Span(span), prop) => {
          span.set_property(prop, value).unwrap_or_else(|| panic!("unknown property '{property}'"))
        }
        _ => panic!("unknown property '{property}'"),
      }
    };
    if invalidate {
      tree.invalidate_cache(node_id);
    }
  })
  .unwrap();

  let ffi = Object::new(ctx.clone()).unwrap();
  ffi.set("createRoot", create_root).unwrap();
  ffi.set("createNode", create_node).unwrap();
  ffi.set("deleteNode", delete_node).unwrap();
  ffi.set("insertNode", insert_node).unwrap();
  ffi.set("setProperty", set_property).unwrap();

  ctx.globals().set("ffi", ffi).unwrap();
}
