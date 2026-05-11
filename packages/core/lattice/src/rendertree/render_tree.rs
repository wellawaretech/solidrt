use std::collections::HashMap;
use taffy::prelude::*;
use taffy::tree::LayoutInput;
use taffy::{
    compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout,
    compute_leaf_layout, CacheTree, LayoutBlockContainer, LayoutFlexboxContainer,
    LayoutGridContainer, RunMode,
};

use crate::rendertree::{Measurable, Element, ElementKind, PlatformContext};

pub struct RenderTree {
    nodes: HashMap<u64, Element>,
    pub root: Option<u64>,
}

// Taffy's CompactLength stores f32 values as tagged pointers (*const ()),
// which prevents the auto Send impl. RenderTree is only moved once to the
// UI thread and never shared across threads.
unsafe impl Send for RenderTree {}

impl RenderTree {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root: None,
        }
    }

    pub fn add_node(&mut self, id: u64, element: Element) -> u64 {
        if self.nodes.contains_key(&id) {
            panic!("duplicate node id {}", id);
        }
        self.nodes.insert(id, element);
        id
    }

    pub fn insert_node(&mut self, parent_id: u64, node_id: u64, anchor_id: Option<u64>) {
        let child_has_layout = {
            let child = self.node_mut(node_id);
            child.parent = Some(parent_id);
            child.has_layout()
        };

        let parent = self.node_mut(parent_id);
        parent.children.retain(|&id| id != node_id);
        if let Some(layout) = &mut parent.layout {
            layout.layout_children.retain(|&id| id != NodeId::from(node_id));
        }

        match anchor_id {
            Some(anchor) => {
                if let Some(pos) = parent.children.iter().position(|&id| id == anchor) {
                    parent.children.insert(pos, node_id);
                } else {
                    parent.children.push(node_id);
                }
                if child_has_layout {
                    if let Some(layout) = &mut parent.layout {
                        let anchor_nid = NodeId::from(anchor);
                        let node_nid = NodeId::from(node_id);
                        if let Some(pos) = layout.layout_children.iter().position(|&id| id == anchor_nid) {
                            layout.layout_children.insert(pos, node_nid);
                        } else {
                            layout.layout_children.push(node_nid);
                        }
                    }
                }
            }
            None => {
                parent.children.push(node_id);
                if child_has_layout {
                    if let Some(layout) = &mut parent.layout {
                        layout.layout_children.push(NodeId::from(node_id));
                    }
                }
            }
        }

        self.invalidate_cache(parent_id);
    }

    pub fn delete_node(&mut self, parent_id: u64, node_id: u64) {
        let parent = self.node_mut(parent_id);
        parent.children.retain(|&id| id != node_id);
        if let Some(layout) = &mut parent.layout {
            layout.layout_children.retain(|&id| id != NodeId::from(node_id));
        }
        self.delete_recursive(node_id);
        self.invalidate_cache(parent_id);
    }

    pub fn element_mut(&mut self, id: u64) -> &mut Element {
        self.node_mut(id)
    }

    pub(crate) fn node(&self, id: u64) -> &Element {
        self.nodes
            .get(&id)
            .expect(&format!("node {} not found", id))
    }

    pub(crate) fn node_mut(&mut self, id: u64) -> &mut Element {
        self.nodes
            .get_mut(&id)
            .expect(&format!("node {} not found", id))
    }

    fn delete_recursive(&mut self, node_id: u64) {
        let child_ids: Vec<u64> = self.nodes.get(&node_id)
            .map(|e| e.children.clone())
            .unwrap_or_default();
        for child_id in child_ids {
            self.delete_recursive(child_id);
        }
        self.nodes.remove(&node_id);
    }

    fn invalidate_cache(&mut self, node_id: u64) {
        let mut current = Some(node_id);
        while let Some(id) = current {
            let element = self.node_mut(id);
            let Some(layout) = &mut element.layout else {
                current = element.parent;
                continue;
            };
            if layout.cache.is_empty() {
                break;
            }
            layout.cache.clear();
            current = element.parent;
        }
    }
}

// --- Taffy layout integration ---------------------------------------------------

pub struct LayoutContext<'a> {
    pub render_tree: &'a mut RenderTree,
    pub platform: &'a PlatformContext,
}

impl<'a> TraversePartialTree for LayoutContext<'a> {
    type ChildIter<'b> = std::iter::Cloned<std::slice::Iter<'b, NodeId>> where Self: 'b;

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
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<taffy::LayoutOutput> {
        self.render_tree.node(u64::from(node_id)).layout_data().cache.get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: taffy::LayoutOutput,
    ) {
        self.render_tree.node_mut(u64::from(node_id)).layout_data_mut().cache.store(known_dimensions, available_space, run_mode, layout_output)
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.render_tree.node_mut(u64::from(node_id)).layout_data_mut().cache.clear();
    }
}

impl<'a> LayoutPartialTree for LayoutContext<'a> {
    type CustomIdent = String;
    type CoreContainerStyle<'b> = &'b Style where Self: 'b;

    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.render_tree.node(u64::from(node_id)).layout_data().style
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.render_tree.node_mut(u64::from(node_id)).layout_data_mut().computed = *layout;
    }

    fn compute_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: LayoutInput,
    ) -> taffy::LayoutOutput {
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
                    text_el.text = text;
                }
            }

            let element = tree.render_tree.node(id);
            let has_measurement = matches!(&element.kind, ElementKind::Text(_) | ElementKind::Rectangle(_));

            if has_measurement {
                let platform = tree.platform;
                let style = &tree.render_tree.node(id).layout_data().style;
                let kind = &tree.render_tree.node(id).kind;
                compute_leaf_layout(inputs, style, |_, _| 0.0, |known, available| {
                    kind.measure(known, available, platform)
                })
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
    type FlexboxContainerStyle<'b> = &'b Style where Self: 'b;
    type FlexboxItemStyle<'b> = &'b Style where Self: 'b;

    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.render_tree.node(u64::from(node_id)).layout_data().style
    }

    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.render_tree.node(u64::from(child_node_id)).layout_data().style
    }
}

impl<'a> LayoutBlockContainer for LayoutContext<'a> {
    type BlockContainerStyle<'b> = &'b Style where Self: 'b;
    type BlockItemStyle<'b> = &'b Style where Self: 'b;

    fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.render_tree.node(u64::from(node_id)).layout_data().style
    }

    fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
        &self.render_tree.node(u64::from(child_node_id)).layout_data().style
    }
}

impl<'a> LayoutGridContainer for LayoutContext<'a> {
    type GridContainerStyle<'b> = &'b Style where Self: 'b;
    type GridItemStyle<'b> = &'b Style where Self: 'b;

    fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
        &self.render_tree.node(u64::from(node_id)).layout_data().style
    }

    fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
        &self.render_tree.node(u64::from(child_node_id)).layout_data().style
    }
}
