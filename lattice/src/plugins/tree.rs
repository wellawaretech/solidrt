use flux::rquickjs::{function::Opt, Ctx, Function, JsLifetime};
use std::cell::RefCell;
use std::rc::Rc;

use crate::rendertree::{Element, ElementKind, Rectangle, RenderTree, Span, Text, View, Window};

#[repr(u32)]
pub enum NodeKind {
  Window = 0,
  View = 1,
  Rectangle = 2,
  Text = 3,
  Span = 4,
}

impl TryFrom<u32> for NodeKind {
  type Error = u32;
  fn try_from(n: u32) -> Result<Self, u32> {
    match n {
      0 => Ok(Self::Window),
      1 => Ok(Self::View),
      2 => Ok(Self::Rectangle),
      3 => Ok(Self::Text),
      4 => Ok(Self::Span),
      _ => Err(n),
    }
  }
}

impl From<NodeKind> for ElementKind {
  fn from(kind: NodeKind) -> Self {
    match kind {
      NodeKind::Window => panic!("use createRoot to create the root Window node"),
      NodeKind::View => ElementKind::View(View::default()),
      NodeKind::Rectangle => ElementKind::Rectangle(Rectangle::default()),
      NodeKind::Text => ElementKind::Text(Text::default()),
      NodeKind::Span => ElementKind::Span(Span::default()),
    }
  }
}

#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

pub fn init(ctx: &Ctx<'_>, tree: RenderTree) {
  let shared = SharedRenderTree(Rc::new(RefCell::new(tree)));
  ctx.store_userdata(shared.clone()).unwrap();

  let tree_ref = shared.0.clone();
  let create_node = Function::new(ctx.clone(), move |id: u64, kind: u32| {
    let kind = NodeKind::try_from(kind).expect("unknown NodeKind discriminant");
    tree_ref
      .borrow_mut()
      .create_node(id, Element::no_layout(kind.into()));
  })
  .unwrap();

  let tree_ref = shared.0.clone();
  let create_root = Function::new(ctx.clone(), move |id: u64| {
    let mut tree = tree_ref.borrow_mut();
    tree.create_node(id, Window::default().with_layout());
    tree.root = Some(id);
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
  globals.set("createNode", create_node).unwrap();
  globals.set("createRoot", create_root).unwrap();
  globals.set("insertNode", insert_node).unwrap();
}
