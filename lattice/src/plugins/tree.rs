use flux::rquickjs::{function::Opt, Ctx, Function, JsLifetime};
use std::cell::RefCell;
use std::rc::Rc;

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
  let set_property = Function::new(ctx.clone(), move |node_id: u64, property: String, value: f64| {
    let mut tree = tree_ref.borrow_mut();
    let element = tree.element_mut(node_id);
    match (&mut element.kind, property.as_str()) {
      (ElementKind::Rectangle(rect), "x") => rect.x = Some(value as f32),
      (ElementKind::Rectangle(rect), "y") => rect.y = Some(value as f32),
      (ElementKind::Rectangle(rect), "w") => rect.w = Some(value as f32),
      (ElementKind::Rectangle(rect), "h") => rect.h = Some(value as f32),
      (ElementKind::Rectangle(rect), "r") => rect.r = Some(value as f32),
      _ => panic!("unknown property '{property}'"),
    }
  })
  .unwrap();

  let globals = ctx.globals();
  globals.set("createRoot", create_root).unwrap();
  globals.set("createNode", create_node).unwrap();
  globals.set("deleteNode", delete_node).unwrap();
  globals.set("insertNode", insert_node).unwrap();
  globals.set("setProperty", set_property).unwrap();
}
