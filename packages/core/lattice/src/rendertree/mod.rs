pub mod composite;
mod kinds;
mod render_tree;

pub use kinds::{Rectangle, Span, Text, View, Window};
pub use render_tree::{LayoutContext, RenderTree};


use alloy::impellers::{DisplayListBuilder, TypographyContext};
use taffy::prelude::*;
use taffy::Cache;

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

/// Trait for element type build behavior
pub trait Buildable {
    fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder);
}

/// Context passed during hit testing.
pub struct HitContext {
    pub size: WH,
}

/// Trait for per-element hit testing behavior.
pub trait Hittable {
    /// Transform a point from parent space into this element's local space.
    /// Default: identity (no transform).
    fn transform_to_local(&self, point: XY, _ctx: &HitContext) -> XY {
        point
    }

    /// Check if a point (already in local coordinates) is within this element's hit shape.
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

pub enum ElementKind {
    Window(Window),
    View(View),
    Rectangle(Rectangle),
    // Oval(Oval),
    // Path(Path),
    Text(Text),
    Span(Span),
    // Texture(Texture),
    // Audio(Audio),
}

impl Buildable for ElementKind {
    fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
        match self {
            ElementKind::Window(n) => n.build(ctx, builder),
            ElementKind::View(n) => n.build(ctx, builder),
            ElementKind::Rectangle(n) => n.build(ctx, builder),
            // ElementKind::Oval(n) => n.build(ctx, builder),
            // ElementKind::Path(n) => n.build(ctx, builder),
            ElementKind::Text(n) => n.build(ctx, builder),
            // ElementKind::Texture(n) => n.build(ctx, builder),
            ElementKind::Span(_) => {}
            // ElementKind::Audio(_) => {}
        }
    }
}

impl Hittable for ElementKind {
    fn transform_to_local(&self, point: XY, ctx: &HitContext) -> XY {
        match self {
            ElementKind::View(n) => n.transform_to_local(point, ctx),
            _ => point,
        }
    }

    fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
        match self {
            ElementKind::Rectangle(n) => n.is_in_bounds(point, ctx),
            // ElementKind::Oval(n) => n.is_in_bounds(point, ctx),
            // ElementKind::Path(n) => n.is_in_bounds(point, ctx),
            // ElementKind::Texture(n) => n.is_in_bounds(point, ctx),
            ElementKind::Span(_) => false,
            // ElementKind::Audio(_) => false,
            _ => point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h,
        }
    }
}

impl Measurable for ElementKind {
    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        typography_ctx: &TypographyContext,
    ) -> Size<f32> {
        match self {
            ElementKind::Text(n) => n.measure(known_dimensions, available_space, typography_ctx),
            // ElementKind::Texture(n) => n.measure(known_dimensions, available_space, typography_ctx),
            // ElementKind::Path(n) => n.measure(known_dimensions, available_space, typography_ctx),
            // ElementKind::Oval(n) => n.measure(known_dimensions, available_space, typography_ctx),
            ElementKind::Rectangle(n) => n.measure(known_dimensions, available_space, typography_ctx),
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

/// Controls whether an element participates in hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    /// Default: element is hit-testable; miss clips children.
    Auto = 0,
    /// Element is transparent to hit testing.
    None = 1,
    /// Element captures all pointer events within bounds, stopping propagation.
    All = 2,
}

pub struct Element {
    pub kind: ElementKind,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
    pub layout: Option<LayoutData>,
    pub pointer_events: PointerEvents,
}

impl Element {
    pub fn with_layout(kind: ElementKind, style: Style) -> Self {
        Self {
            kind,
            children: vec![],
            parent: None,
            layout: Some(LayoutData::new(style)),
            pointer_events: PointerEvents::Auto,
        }
    }

    pub fn no_layout(kind: ElementKind) -> Self {
        Self {
            kind,
            children: vec![],
            parent: None,
            layout: None,
            pointer_events: PointerEvents::Auto,
        }
    }

    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    pub fn layout_data(&self) -> &LayoutData {
        self.layout.as_ref().expect("element has no layout data")
    }

    pub fn layout_data_mut(&mut self) -> &mut LayoutData {
        self.layout.as_mut().expect("element has no layout data")
    }

    pub fn style_mut(&mut self) -> Option<&mut Style> {
        self.layout.as_mut().map(|l| &mut l.style)
    }

    pub fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
        self.kind.build(ctx, builder);
    }
}
