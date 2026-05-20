use taffy::prelude::*;
use taffy::tree::LayoutInput;
use taffy::Cache;
use taffy::{
  compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout,
  compute_leaf_layout, CacheTree, LayoutBlockContainer, LayoutFlexboxContainer,
  LayoutGridContainer, RunMode,
};

use super::super::tree::RenderTree;
use crate::rendertree::{ElementKind, Measurable, PlatformContext};

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

pub struct LayoutContext<'a> {
  pub render_tree: &'a mut RenderTree,
  pub platform: &'a PlatformContext,
}

impl<'a> TraversePartialTree for LayoutContext<'a> {
  type ChildIter<'b>
    = std::iter::Cloned<std::slice::Iter<'b, NodeId>>
  where
    Self: 'b;

  fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
    self
      .render_tree
      .node(u64::from(parent))
      .layout_data()
      .layout_children
      .iter()
      .cloned()
  }

  fn child_count(&self, parent: NodeId) -> usize {
    self
      .render_tree
      .node(u64::from(parent))
      .layout_data()
      .layout_children
      .len()
  }

  fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
    self
      .render_tree
      .node(u64::from(parent))
      .layout_data()
      .layout_children[index]
  }
}

impl<'a> CacheTree for LayoutContext<'a> {
  fn cache_get(
    &self,
    node_id: NodeId,
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    run_mode: RunMode,
  ) -> Option<taffy::LayoutOutput> {
    self
      .render_tree
      .node(u64::from(node_id))
      .layout_data()
      .cache
      .get(known_dimensions, available_space, run_mode)
  }

  fn cache_store(
    &mut self,
    node_id: NodeId,
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    run_mode: RunMode,
    layout_output: taffy::LayoutOutput,
  ) {
    self
      .render_tree
      .node_mut(u64::from(node_id))
      .layout_data_mut()
      .cache
      .store(known_dimensions, available_space, run_mode, layout_output)
  }

  fn cache_clear(&mut self, node_id: NodeId) {
    self
      .render_tree
      .node_mut(u64::from(node_id))
      .layout_data_mut()
      .cache
      .clear();
  }
}

impl<'a> LayoutPartialTree for LayoutContext<'a> {
  type CustomIdent = String;
  type CoreContainerStyle<'b>
    = &'b Style
  where
    Self: 'b;

  fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
    &self
      .render_tree
      .node(u64::from(node_id))
      .layout_data()
      .style
  }

  fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
    self
      .render_tree
      .node_mut(u64::from(node_id))
      .layout_data_mut()
      .computed = *layout;
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
      let has_measurement = matches!(
        &element.kind,
        ElementKind::Text(_) | ElementKind::Rectangle(_) | ElementKind::Path(_)
      );

      if has_measurement {
        let platform = tree.platform;
        let style = &tree.render_tree.node(id).layout_data().style;
        let kind = &tree.render_tree.node(id).kind;
        compute_leaf_layout(
          inputs,
          style,
          |_, _| 0.0,
          |known, available| kind.measure(known, available, platform),
        )
      } else {
        match element.layout_data().style.display {
          Display::Flex => compute_flexbox_layout(tree, node_id, inputs),
          Display::Block => compute_block_layout(tree, node_id, inputs),
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
    &self
      .render_tree
      .node(u64::from(node_id))
      .layout_data()
      .style
  }

  fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
    &self
      .render_tree
      .node(u64::from(child_node_id))
      .layout_data()
      .style
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
    &self
      .render_tree
      .node(u64::from(node_id))
      .layout_data()
      .style
  }

  fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
    &self
      .render_tree
      .node(u64::from(child_node_id))
      .layout_data()
      .style
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
    &self
      .render_tree
      .node(u64::from(node_id))
      .layout_data()
      .style
  }

  fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
    &self
      .render_tree
      .node(u64::from(child_node_id))
      .layout_data()
      .style
  }
}
