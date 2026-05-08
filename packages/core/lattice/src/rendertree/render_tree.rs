use std::collections::HashMap;
use taffy::prelude::*;
use taffy::tree::LayoutInput;
use taffy::{
    compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout,
    compute_leaf_layout, CacheTree, LayoutBlockContainer, LayoutFlexboxContainer,
    LayoutGridContainer, RunMode,
};

use crate::rendertree::{Measurable, Node, Primitive};
use alloy::impellers::TypographyContext;

pub struct RenderTree {
    nodes: HashMap<NodeId, Node>,
    pub root: Option<NodeId>,
    pub typography_ctx: TypographyContext,
}

impl RenderTree {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root: None,
            typography_ctx: TypographyContext::default(),
        }
    }

    pub fn add_node(&mut self, id: u64, node: Node) -> NodeId {
        let node_id: NodeId = NodeId::from(id);
        if self.nodes.contains_key(&node_id) {
            panic!("duplicate node id {}", id);
        }
        self.nodes.insert(node_id, node);
        node_id
    }

    pub fn node(&self, id: NodeId) -> &Node {
        self.nodes
            .get(&id)
            .expect(&format!("node {:?} not found", id))
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes
            .get_mut(&id)
            .expect(&format!("node {:?} not found", id))
    }

    pub fn insert_node(
        &mut self,
        parent_id: NodeId,
        node_id: NodeId,
        anchor_id: Option<NodeId>,
    ) {
        let child_has_layout = {
            let child = self.node_mut(node_id);
            child.parent = Some(parent_id);
            child.has_layout()
        };

        let parent = self.node_mut(parent_id);
        parent.children.retain(|&id| id != node_id);
        if let Some(layout) = &mut parent.layout {
            layout.layout_children.retain(|&id| id != node_id);
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
                        if let Some(pos) = layout.layout_children.iter().position(|&id| id == anchor) {
                            layout.layout_children.insert(pos, node_id);
                        } else {
                            layout.layout_children.push(node_id);
                        }
                    }
                }
            }
            None => {
                parent.children.push(node_id);
                if child_has_layout {
                    if let Some(layout) = &mut parent.layout {
                        layout.layout_children.push(node_id);
                    }
                }
            }
        }

        self.invalidate_cache(parent_id);
    }

    pub fn delete_node(&mut self, parent_id: NodeId, node_id: NodeId) {
        let parent = self.node_mut(parent_id);
        parent.children.retain(|&id| id != node_id);
        if let Some(layout) = &mut parent.layout {
            layout.layout_children.retain(|&id| id != node_id);
        }
        self.delete_recursive(node_id);
        self.invalidate_cache(parent_id);
    }

    fn delete_recursive(&mut self, node_id: NodeId) {
        let child_ids: Vec<NodeId> = self.nodes.get(&node_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for child_id in child_ids {
            self.delete_recursive(child_id);
        }
        self.nodes.remove(&node_id);
    }

    pub fn invalidate_cache(&mut self, node_id: NodeId) {
        let mut current = Some(node_id);
        while let Some(id) = current {
            let node = self.node_mut(id);
            let Some(layout) = &mut node.layout else {
                current = node.parent;
                continue;
            };
            if layout.cache.is_empty() {
                break;
            }
            layout.cache.clear();
            current = node.parent;
        }
    }
}

// --- Taffy layout integration ---------------------------------------------------

pub struct LayoutContext<'a> {
    pub render_tree: &'a mut RenderTree,
}

impl<'a> TraversePartialTree for LayoutContext<'a> {
    type ChildIter<'b> = std::iter::Cloned<std::slice::Iter<'b, NodeId>> where Self: 'b;

    fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
        self.render_tree.node(parent).layout_data().layout_children.iter().cloned()
    }

    fn child_count(&self, parent: NodeId) -> usize {
        self.render_tree.node(parent).layout_data().layout_children.len()
    }

    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        self.render_tree.node(parent).layout_data().layout_children[index]
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
        self.render_tree.node(node_id).layout_data().cache.get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: taffy::LayoutOutput,
    ) {
        self.render_tree.node_mut(node_id).layout_data_mut().cache.store(known_dimensions, available_space, run_mode, layout_output)
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.render_tree.node_mut(node_id).layout_data_mut().cache.clear();
    }
}

impl<'a> LayoutPartialTree for LayoutContext<'a> {
    type CustomIdent = String;
    type CoreContainerStyle<'b> = &'b Style where Self: 'b;

    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.render_tree.node(node_id).layout_data().style
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.render_tree.node_mut(node_id).layout_data_mut().computed = *layout;
    }

    fn compute_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: LayoutInput,
    ) -> taffy::LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let node = tree.render_tree.node(node_id);

            // Handle TextNode: concatenate strings from StringNode children
            if let Primitive::Text(_) = &node.node_type {
                let children = node.children.clone();
                let mut text = String::new();
                for child_id in children {
                    if let Primitive::String(string_node) = &tree.render_tree.node(child_id).node_type {
                        text.push_str(&string_node.text);
                    }
                }
                if let Primitive::Text(text_node) = &mut tree.render_tree.node_mut(node_id).node_type {
                    text_node.text = text;
                }
            }

            let node = tree.render_tree.node(node_id);
            let has_measurement = matches!(&node.node_type, Primitive::Text(_) | Primitive::Rect(_));

            if has_measurement {
                let tc = &tree.render_tree.typography_ctx;
                let style = &tree.render_tree.node(node_id).layout_data().style;
                let node_type = &tree.render_tree.node(node_id).node_type;
                compute_leaf_layout(inputs, style, |_, _| 0.0, |known, available| {
                    node_type.measure(known, available, tc)
                })
            } else {
                match node.layout_data().style.display {
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
        &self.render_tree.node(node_id).layout_data().style
    }

    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.render_tree.node(child_node_id).layout_data().style
    }
}

impl<'a> LayoutBlockContainer for LayoutContext<'a> {
    type BlockContainerStyle<'b> = &'b Style where Self: 'b;
    type BlockItemStyle<'b> = &'b Style where Self: 'b;

    fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.render_tree.node(node_id).layout_data().style
    }

    fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
        &self.render_tree.node(child_node_id).layout_data().style
    }
}

impl<'a> LayoutGridContainer for LayoutContext<'a> {
    type GridContainerStyle<'b> = &'b Style where Self: 'b;
    type GridItemStyle<'b> = &'b Style where Self: 'b;

    fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
        &self.render_tree.node(node_id).layout_data().style
    }

    fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
        &self.render_tree.node(child_node_id).layout_data().style
    }
}