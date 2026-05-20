use flux::rquickjs::{function::Opt, Ctx, Function, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::rendertree::layout::properties;
use crate::rendertree::{ElementKind, Path, Rectangle, RenderTree, Span, Text, View, Window};

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
      "d-rect" => Rectangle::default().no_layout(),
      "path" => Path::default().with_layout(),
      "d-path" => Path::default().no_layout(),
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
      let prop = property.as_str();
      let result = match &mut element.kind {
        ElementKind::Window(win) => win.set_property(prop, value.clone()),
        ElementKind::Rectangle(rect) => rect.set_property(prop, value.clone()),
        ElementKind::Path(path) => path.set_property(prop, value.clone()),
        ElementKind::Text(text) => text.set_property(prop, value.clone()),
        ElementKind::Span(span) => span.set_property(prop, value.clone()),
        ElementKind::View(view) => view.set_property(prop, value.clone()),
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

  let ffi = Object::new(ctx.clone()).unwrap();
  ffi.set("createRoot", create_root).unwrap();
  ffi.set("createNode", create_node).unwrap();
  ffi.set("deleteNode", delete_node).unwrap();
  ffi.set("insertNode", insert_node).unwrap();
  ffi.set("setProperty", set_property).unwrap();

  ctx.globals().set("ffi", ffi).unwrap();
}
