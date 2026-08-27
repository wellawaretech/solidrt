use taffy::prelude::*;
use taffy::tree::LayoutInput;
use taffy::{
  compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_hidden_layout,
  compute_leaf_layout, CacheTree, CoreStyle, LayoutBlockContainer, LayoutFlexboxContainer, LayoutGridContainer,
  RequestedAxis, ResolveOrZero, RunMode, SizingMode,
};

use super::super::tree::RenderTree;
use super::cache::LayoutCache;
use crate::rendertree::{replaced_size, ElementKind, Measurable, MeasureContext, PlatformContext};

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

  // The border box inset by padding and border, origin included: the box a
  // kind's own content sizes and places against, matching the inset
  // place_atoms applies to a text's inline atoms
  // (okf/done/padding-box-divergence.md). Paint and hit both derive it
  // from here, so they cannot disagree.
  pub fn content_box(&self) -> crate::impellers::Rect {
    let c = &self.computed;
    let left = c.padding.left + c.border.left;
    let top = c.padding.top + c.border.top;
    let right = c.padding.right + c.border.right;
    let bottom = c.padding.bottom + c.border.bottom;
    crate::impellers::Rect::new(
      crate::impellers::Point::new(left, top),
      crate::impellers::Size::new(c.size.width - left - right, c.size.height - top - bottom),
    )
  }
}

pub struct LayoutContext<'a> {
  pub render_tree: &'a mut RenderTree,
  pub platform: &'a PlatformContext,
  pub alloy: &'a crate::Context,
}

impl<'a> LayoutContext<'a> {
  // Padding and border of a node's style, resolved with no percentage basis:
  // the layout box's inset (`Layout::padding`/`border`) that composite reads
  // to derive the content box.
  fn insets(&self, node_id: NodeId) -> (taffy::Rect<f32>, taffy::Rect<f32>) {
    let style = &self.render_tree.node(u64::from(node_id)).layout_data().style;
    let calc = |val, basis| self.resolve_calc_value(val, basis);
    (style.padding().resolve_or_zero(None, calc), style.border().resolve_or_zero(None, calc))
  }

  fn margin(&self, node_id: NodeId) -> taffy::Rect<f32> {
    let style = &self.render_tree.node(u64::from(node_id)).layout_data().style;
    style.margin().resolve_or_zero(None, |val, basis| self.resolve_calc_value(val, basis))
  }

  // Lay out one inline atom of `text` as an independent shrink-to-fit root
  // (like an inline block: max-content in both axes, its own style size and
  // padding honored) and hand its margin box to the text's run: margins are
  // how an atom keeps its distance from the words around it.
  fn measure_atom(&mut self, text: u64, atom: NodeId) {
    let output = self.compute_child_layout(
      atom,
      LayoutInput {
        run_mode: RunMode::PerformLayout,
        sizing_mode: SizingMode::InherentSize,
        axis: RequestedAxis::Both,
        known_dimensions: Size::NONE,
        parent_size: Size::NONE,
        available_space: Size::MAX_CONTENT,
        vertical_margins_are_collapsible: Line::FALSE,
      },
    );
    let margin = self.margin(atom);
    let size = crate::impellers::Size::new(
      output.size.width + margin.horizontal_axis_sum(),
      output.size.height + margin.vertical_axis_sum(),
    );
    if let crate::rendertree::ElementKind::Text(t) = &mut self.render_tree.node_mut(text).kind {
      t.set_atom_size(u64::from(atom), size);
    }
    // Position comes after the text's own layout (place_atoms); the box is
    // final now.
    let (padding, border) = self.insets(atom);
    let mut layout = self.render_tree.node(u64::from(atom)).layout_data().computed;
    layout.size = output.size;
    layout.content_size = output.content_size;
    layout.padding = padding;
    layout.border = border;
    layout.margin = margin;
    self.set_unrounded_layout(atom, &layout);
  }

  // Write each atom's location from the text's line layout at its final
  // (content) width, relative to the text's box.
  fn place_atoms(&mut self, text: u64, size: Size<f32>) {
    let (padding, border) = self.insets(NodeId::from(text));
    let inset = padding + border;
    let width = size.width - inset.horizontal_axis_sum();
    let positions = match &self.render_tree.node(text).kind {
      crate::rendertree::ElementKind::Text(t) => t.atom_positions(self.platform, width),
      _ => return,
    };
    for (atom, point) in positions {
      // `point` is the margin box's top-left; the layout box sits inside it.
      let mut layout = self.render_tree.node(atom).layout_data().computed;
      layout.location =
        taffy::Point { x: inset.left + layout.margin.left + point.x, y: inset.top + layout.margin.top + point.y };
      self.set_unrounded_layout(NodeId::from(atom), &layout);
    }
  }
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

impl<'a> LayoutContext<'a> {
  // taffy's container algorithm for a display.
  fn container_layout(&mut self, node_id: NodeId, display: Display, inputs: LayoutInput) -> taffy::LayoutOutput {
    match display {
      Display::Flex => compute_flexbox_layout(self, node_id, inputs),
      Display::Block => compute_block_layout(self, node_id, inputs, None),
      Display::Grid => compute_grid_layout(self, node_id, inputs),
      Display::None => compute_hidden_layout(self, node_id),
    }
  }

  // A design-size view lays out on both sides of its fit
  // (okf/done/viewbox-layout-space.md).
  //
  // Outside, it is a replaced element with the design size as intrinsic size
  // (the texture's <img> rules), except that it compresses: a min-content
  // query gets zero, since a design has no size it cannot scale below - a
  // `flex={1}` design-size view fits a window smaller than its design instead of
  // overflowing it the way a texture would.
  //
  // Inside, the children are their own root at the design size, whatever box
  // the outside settled on: the space paint, hit testing, culling and
  // bounding boxes already hand them, which the fit maps onto the box. The
  // view's own size styles belong to the outer box, so the inner pass runs in
  // ContentSize mode, where only the known dimensions count. Its output (the
  // design size) is not what the parent sees; the children's placements are
  // the point. The inner input is constant, so a resize re-solves nothing
  // below the view: the children's caches answer.
  fn design_size_layout(
    &mut self,
    node_id: NodeId,
    inputs: LayoutInput,
    design: crate::impellers::Size,
    display: Display,
  ) -> taffy::LayoutOutput {
    let style = &self.render_tree.node(u64::from(node_id)).layout_data().style;
    let output = compute_leaf_layout(
      inputs,
      style,
      |_, _| 0.0,
      |known, available| {
        let min_content = |a: AvailableSpace| matches!(a, AvailableSpace::MinContent);
        if min_content(available.width) || min_content(available.height) {
          return Size::ZERO;
        }
        let size = replaced_size(known, design);
        Size { width: size.width, height: size.height }
      },
    );
    if inputs.run_mode == RunMode::PerformLayout {
      let known = Size { width: Some(design.width), height: Some(design.height) };
      let inner = LayoutInput {
        run_mode: RunMode::PerformLayout,
        sizing_mode: SizingMode::ContentSize,
        axis: RequestedAxis::Both,
        known_dimensions: known,
        parent_size: known,
        available_space: Size {
          width: AvailableSpace::Definite(design.width),
          height: AvailableSpace::Definite(design.height),
        },
        vertical_margins_are_collapsible: Line::FALSE,
      };
      self.container_layout(node_id, display, inner);
    }
    output
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
    // An ancestor is display none: the hidden pass zeroes this node and
    // recurses into its children whatever its own display says. Ahead of the
    // leaf branch as well, since compute_leaf_layout has no hidden arm.
    if inputs.run_mode == RunMode::PerformHiddenLayout {
      return compute_hidden_layout(self, node_id);
    }
    compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
      let id = u64::from(node_id);
      // A Text's computed_text and runs are kept current eagerly by
      // RenderTree::sync_text on every span or structural change, so the
      // layout pass reads them as-is.
      let element = tree.render_tree.node(id);
      if element.kind.is_measured_leaf() {
        // A text's laid-out children are its inline atoms: each is measured
        // as its own shrink-to-fit root and its box handed to the text before
        // the text measures itself.
        let atoms = match element.kind {
          crate::rendertree::ElementKind::Text(_) => element.layout_data().layout_children.clone(),
          _ => Vec::new(),
        };
        for &atom in &atoms {
          tree.measure_atom(id, atom);
        }
        let platform = tree.platform;
        let alloy = tree.alloy;
        let (padding, border) = tree.insets(node_id);
        let inset = padding + border;
        let style = &tree.render_tree.node(id).layout_data().style;
        let kind = &tree.render_tree.node(id).kind;
        let output = compute_leaf_layout(
          inputs,
          style,
          |_, _| 0.0,
          |known, available| {
            // taffy's known dimensions are border-box; measure sees the
            // content box, matching the content-box available space
            // compute_leaf_layout itself passes, so a padded text wraps at
            // the width it will paint at
            // (okf/done/padding-box-divergence.md).
            let known = taffy::Size {
              width: known.width.map(|w| (w - inset.horizontal_axis_sum()).max(0.0)),
              height: known.height.map(|h| (h - inset.vertical_axis_sum()).max(0.0)),
            };
            let size = kind.measure(&MeasureContext { platform, alloy, known, available });
            Size { width: size.width, height: size.height }
          },
        );
        if !atoms.is_empty() && inputs.run_mode == RunMode::PerformLayout {
          tree.place_atoms(id, output.size);
        }
        output
      } else {
        let display = element.layout_data().style.display;
        // A design-size view is a layout boundary (design_size_layout); a hidden one
        // is hidden first.
        let design = match &element.kind {
          ElementKind::View(v) if display != Display::None => v.design_space(),
          _ => None,
        };
        match design {
          Some(design) => tree.design_size_layout(node_id, inputs, design, display),
          None => tree.container_layout(node_id, display, inputs),
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
