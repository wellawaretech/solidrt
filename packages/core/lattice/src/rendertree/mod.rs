pub mod frame;
pub mod layout;
pub mod nodes;
mod render_tree;

pub use nodes::*;
pub use render_tree::{LayoutContext, RenderTree};


use alloy::impellers::{DisplayListBuilder, TypographyContext};
use taffy::prelude::*;
use taffy::Cache;

// use constants::element_type;

#[derive(Clone, Copy, Debug, Default)]
pub struct XY {
    pub x: f32,
    pub y: f32,
}

impl XY {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WH {
    pub w: f32,
    pub h: f32,
}

impl WH {
    pub fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }
}

/// Build context passed during display list tree traversal.
pub struct BuildContext<'a> {
    pub typography_ctx: &'a TypographyContext,
    pub size: WH,
    pub origin: XY,
}

impl<'a> BuildContext<'a> {
    pub fn new(typography_ctx: &'a TypographyContext) -> Self {
        Self {
            typography_ctx,
            size: WH::default(),
            origin: XY::default(),
        }
    }
}

/// Trait for node type build behavior
pub trait Buildable {
    fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder);
}

/// Context passed during hit testing.
pub struct HitContext {
    pub size: WH,
}

/// Trait for per-node hit testing behavior.
pub trait Hittable {
    /// Transform a point from parent space into this node's local space.
    /// Default: identity (no transform).
    fn transform_to_local(&self, point: XY, _ctx: &HitContext) -> XY {
        point
    }

    /// Check if a point (already in local coordinates) is within this node's hit shape.
    fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
        point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h
    }
}

/// Trait for content-based sizing (text, images, etc.)
pub trait Measurable {
    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        typography_ctx: &TypographyContext,
    ) -> Size<f32>;
}

/// Enum wrapping all node types
pub enum Primitive {
    Window(WindowNode),
    View(ViewNode),
    Rect(RectNode),
    // Oval(OvalNode),
    // Path(PathNode),
    Text(TextNode),
    String(StringNode),
    // Texture(TextureNode),
    // Audio(AudioNode),
}

impl Buildable for Primitive {
    fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
        match self {
            Primitive::Window(n) => n.build(ctx, builder),
            Primitive::View(n) => n.build(ctx, builder),
            Primitive::Rect(n) => n.build(ctx, builder),
            // Primitive::Oval(n) => n.build(ctx, builder),
            // Primitive::Path(n) => n.build(ctx, builder),
            Primitive::Text(n) => n.build(ctx, builder),
            // Primitive::Texture(n) => n.build(ctx, builder),
            Primitive::String(_) => {}
            // Primitive::Audio(_) => {}
        }
    }
}

impl Hittable for Primitive {
    fn transform_to_local(&self, point: XY, ctx: &HitContext) -> XY {
        match self {
            Primitive::View(n) => n.transform_to_local(point, ctx),
            _ => point,
        }
    }

    fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
        match self {
            Primitive::Rect(n) => n.is_in_bounds(point, ctx),
            // Primitive::Oval(n) => n.is_in_bounds(point, ctx),
            // Primitive::Path(n) => n.is_in_bounds(point, ctx),
            // Primitive::Texture(n) => n.is_in_bounds(point, ctx),
            Primitive::String(_) => false,
            // Primitive::Audio(_) => false,
            _ => point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h,
        }
    }
}

impl Measurable for Primitive {
    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        typography_ctx: &TypographyContext,
    ) -> Size<f32> {
        match self {
            Primitive::Text(n) => n.measure(known_dimensions, available_space, typography_ctx),
            // Primitive::Texture(n) => n.measure(known_dimensions, available_space, typography_ctx),
            // Primitive::Path(n) => n.measure(known_dimensions, available_space, typography_ctx),
            // Primitive::Oval(n) => n.measure(known_dimensions, available_space, typography_ctx),
            Primitive::Rect(n) => n.measure(known_dimensions, available_space, typography_ctx),
            _ => Size::ZERO,
        }
    }
}

pub struct LayoutData {
    pub style: Style,
    pub computed: Layout,
    pub cache: Cache,
    pub layout_children: Vec<NodeId>,
}

impl LayoutData {
    pub fn new(style: Style) -> Self {
        Self {
            style,
            computed: Layout::new(),
            cache: Cache::new(),
            layout_children: vec![],
        }
    }
}

/// Controls whether a node participates in hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    /// Default: node is hit-testable; miss clips children.
    Auto = 0,
    /// Node is transparent to hit testing.
    None = 1,
    /// Node captures all pointer events within bounds, stopping propagation.
    All = 2,
}

pub struct Node {
    pub node_type: Primitive,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
    pub layout: Option<LayoutData>,
    pub pointer_events: PointerEvents,
}

impl Node {
    pub fn new(node_type: Primitive, style: Option<Style>) -> Self {
        let pointer_events = PointerEvents::Auto;
        Self {
            node_type,
            children: vec![],
            parent: None,
            layout: style.map(LayoutData::new),
            pointer_events,
        }
    }

    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    pub fn layout_data(&self) -> &LayoutData {
        self.layout.as_ref().expect("node has no layout data")
    }

    pub fn layout_data_mut(&mut self) -> &mut LayoutData {
        self.layout.as_mut().expect("node has no layout data")
    }

    pub fn style_mut(&mut self) -> Option<&mut Style> {
        self.layout.as_mut().map(|l| &mut l.style)
    }

    pub fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
        self.node_type.build(ctx, builder);
    }
}

// pub fn create_element_with_id(scene: &mut RenderTree, id: u64, elem_type: u8) -> NodeId {
//     let node = match elem_type {
//         element_type::WINDOW => WindowNode::default().into(),
//         element_type::VIEW => ViewNode::default().into(),
//         element_type::STRING => StringNode::default().into(),
//         element_type::TEXT => TextNode::default().into(),
//         element_type::RECTANGLE => RectNode::default().into(),
//         element_type::RAW_RECTANGLE => Node::new(Primitive::Rect(RectNode::default()), None),
//         // element_type::RAW_OVAL => Node::new(Primitive::Oval(OvalNode::default()), None),
//         // element_type::RAW_PATH => Node::new(Primitive::Path(PathNode::default()), None),
//         // element_type::RAW_TEXTURE => Node::new(Primitive::Texture(TextureNode::default()), None),
//         element_type::RAW_TEXT => Node::new(Primitive::Text(TextNode::default()), None),
//         // element_type::AUDIO => AudioNode::default().into(),
//         // element_type::OVAL => OvalNode::default().into(),
//         // element_type::PATH => PathNode::default().into(),
//         // element_type::TEXTURE => TextureNode::default().into(),
//         _ => panic!("element type {} not recognized", elem_type),
//     };
//     scene.add_node(id, node)
// }