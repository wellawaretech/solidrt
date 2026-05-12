use flux::rquickjs::{function::Opt, Ctx, Function, JsLifetime};
use std::cell::RefCell;
use std::rc::Rc;

use crate::rendertree::{Rectangle, RenderTree, Span, Text, View, Window};

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

  let globals = ctx.globals();
  globals.set("createRoot", create_root).unwrap();
  globals.set("createNode", create_node).unwrap();
  globals.set("deleteNode", delete_node).unwrap();
  globals.set("insertNode", insert_node).unwrap();
}
