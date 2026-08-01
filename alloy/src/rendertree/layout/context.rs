use taffy::prelude::*;
use taffy::tree::LayoutInput;
use taffy::{
  compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
  CacheTree, LayoutBlockContainer, LayoutFlexboxContainer, LayoutGridContainer,
};

use super::super::tree::RenderTree;
use super::cache::LayoutCache;
use crate::rendertree::{ElementKind, Measurable, MeasureContext, PlatformContext};

pub struct LayoutData {
  pub style: Style,
  pub computed: Layout,
  pub cache: LayoutCache,
  pub layout_children: Vec<NodeId>,
  // True only when JSX explicitly set `position="relative"`. taffy's default
  // position is Relative for every node, so this flag is what distinguishes a
  // deliberately declared positioning context from the implicit default. It is
  // the stop point when resolving a node's container-relative bounding box.
  pub positioning_context: bool,
}

impl LayoutData {
  pub fn new(style: Style) -> Self {
    Self {
      style,
      computed: Layout::new(),
      cache: LayoutCache::new(),
      layout_children: vec![],
      positioning_context: false,
    }
  }

  // The one seam where taffy's computed layout converts into the euclid paint
  // vocabulary. Everything downstream of layout (hit testing, bounding boxes,
  // compositing) reads the box through these instead of `computed` directly.
  pub fn location(&self) -> crate::impellers::Point {
    crate::impellers::Point::new(self.computed.location.x, self.computed.location.y)
  }

  pub fn size(&self) -> crate::impellers::Size {
    crate::impellers::Size::new(self.computed.size.width, self.computed.size.height)
  }
}

pub struct LayoutContext<'a> {
  pub render_tree: &'a mut RenderTree,
  pub platform: &'a PlatformContext,
  pub alloy: &'a crate::Context,
}

impl<'a> TraversePartialTree for LayoutContext<'a> {
  type ChildIter<'b>
    = std::iter::Cloned<std::slice::Iter<'b, NodeId>>
  where
    Self: 'b;

  fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
    self.render_tree.node(u64::from(parent)).layout_data().layout_children.iter().cloned()
  }

  fn child_count(&self, parent: NodeId) -> usize {
    self.render_tree.node(u64::from(parent)).layout_data().layout_children.len()
  }

  fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
    self.render_tree.node(u64::from(parent)).layout_data().layout_children[index]
  }
}

impl<'a> CacheTree for LayoutContext<'a> {
  fn cache_get(&self, node_id: NodeId, input: &LayoutInput) -> Option<taffy::LayoutOutput> {
    let out = self.render_tree.node(u64::from(node_id)).layout_data().cache.get(input);
    crate::rendertree::counters::note_cache_get(out.is_some());
    out
  }

  fn cache_store(&mut self, node_id: NodeId, input: &LayoutInput, layout_output: taffy::LayoutOutput) {
    self.render_tree.node_mut(u64::from(node_id)).layout_data_mut().cache.store(input, layout_output)
  }

  fn cache_clear(&mut self, node_id: NodeId) {
    self.render_tree.node_mut(u64::from(node_id)).layout_data_mut().cache.clear();
  }
}

impl<'a> LayoutPartialTree for LayoutContext<'a> {
  type CustomIdent = String;
  type CoreContainerStyle<'b>
    = &'b Style
  where
    Self: 'b;

  fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
    &self.render_tree.node(u64::from(node_id)).layout_data().style
  }

  fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
    let id = u64::from(node_id);
    let data = self.render_tree.node_mut(id).layout_data_mut();
    // A changed layout moves or resizes painted content without any element
    // mutation (e.g. a sibling grew); retained boundary recordings above this
    // node are stale.
    if data.computed != *layout {
      data.computed = *layout;
      self.render_tree.invalidate_paint(id);
    }
  }

  fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> taffy::LayoutOutput {
    compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
      let id = u64::from(node_id);
      let element = tree.render_tree.node(id);

      // Handle Text: concatenate text from Span children
      if let ElementKind::Text(_) = &element.kind {
        let children = element.children.clone();
        let mut text = String::new();
        for child_id in children {
          if let ElementKind::Span(span) = &tree.render_tree.node(child_id).kind {
            text.push_str(&span.text);
          }
        }
        if let ElementKind::Text(text_el) = &mut tree.render_tree.node_mut(id).kind {
          text_el.computed_text = text;
        }
      }

      let element = tree.render_tree.node(id);
      if element.kind.is_measured_leaf() {
        let platform = tree.platform;
        let alloy = tree.alloy;
        let style = &tree.render_tree.node(id).layout_data().style;
        let kind = &tree.render_tree.node(id).kind;
        compute_leaf_layout(
          inputs,
          style,
          |_, _| 0.0,
          |known, available| {
            let size = kind.measure(&MeasureContext { platform, alloy, known, available });
            Size { width: size.width, height: size.height }
          },
        )
      } else {
        match element.layout_data().style.display {
          Display::Flex => compute_flexbox_layout(tree, node_id, inputs),
          Display::Block => compute_block_layout(tree, node_id, inputs, None),
          Display::Grid => compute_grid_layout(tree, node_id, inputs),
          Display::None => taffy::LayoutOutput::HIDDEN,
        }
      }
    })
  }
}

impl<'a> LayoutFlexboxContainer for LayoutContext<'a> {
  type FlexboxContainerStyle<'b>
    = &'b Style
  where
    Self: 'b;
  type FlexboxItemStyle<'b>
    = &'b Style
  where
    Self: 'b;

  fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
    &self.render_tree.node(u64::from(node_id)).layout_data().style
  }

  fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
    &self.render_tree.node(u64::from(child_node_id)).layout_data().style
  }
}

impl<'a> LayoutBlockContainer for LayoutContext<'a> {
  type BlockContainerStyle<'b>
    = &'b Style
  where
    Self: 'b;
  type BlockItemStyle<'b>
    = &'b Style
  where
    Self: 'b;

  fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
    &self.render_tree.node(u64::from(node_id)).layout_data().style
  }

  fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
    &self.render_tree.node(u64::from(child_node_id)).layout_data().style
  }
}

impl<'a> LayoutGridContainer for LayoutContext<'a> {
  type GridContainerStyle<'b>
    = &'b Style
  where
    Self: 'b;
  type GridItemStyle<'b>
    = &'b Style
  where
    Self: 'b;

  fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
    &self.render_tree.node(u64::from(node_id)).layout_data().style
  }

  fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
    &self.render_tree.node(u64::from(child_node_id)).layout_data().style
  }
}
