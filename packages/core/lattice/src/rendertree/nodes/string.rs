use crate::rendertree::{Node, Primitive};

#[derive(Clone, Debug, Default)]
pub struct StringNode {
    pub text: String,
}

impl From<StringNode> for Node {
    fn from(string: StringNode) -> Node {
        Node::new(Primitive::String(string), None)
    }
}