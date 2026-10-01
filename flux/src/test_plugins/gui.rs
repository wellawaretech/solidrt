// The `flux:test/gui` module: the node verbs of a gui test, which the app
// layer (`@solidrt/core/test`) builds locators and readers on. Thin FFI over
// the rendertree's own inspection (find, visibility, the hit path) and the
// shared node record (alloy_plugins/inspect.rs), so a test reads a node as
// the control API's `/tree` reports it, and the GPU inventory as `/gpu`
// does. Records cross as JSON text: the
// record is shaped as JSON once, for both readers.
//
// Compiled where flux has both the `test` and the `gui` feature, which is
// the dev client; the `flux` binary has no tree to read.

use alloy::rendertree::{Match, NodeQuery, Point};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, Object, Value};

use crate::alloy_plugins::inspect::{gpu_record, node_record};
use crate::alloy_plugins::tree::with_tree;

pub const MODULE_NAME: &str = "flux:test/gui";

fn no_tree(ctx: &Ctx<'_>, verb: &str) -> rquickjs::Error {
  Exception::throw_message(ctx, &format!("flux:test/gui {verb}: no render tree in this runtime"))
}

/// One field of a `find` query as JS passes it: a string is an exact match,
/// `true` asks for the nodes that have the field at all (the caller matches
/// a pattern itself), absent is no constraint.
enum Field {
  Any,
  Present,
  Exact(String),
}

impl Field {
  fn read(ctx: &Ctx<'_>, query: &Object<'_>, name: &str) -> rquickjs::Result<Self> {
    let value: Value = query.get(name)?;
    if value.is_undefined() || value.is_null() {
      return Ok(Field::Any);
    }
    if value.as_bool() == Some(true) {
      return Ok(Field::Present);
    }
    match value.as_string() {
      Some(text) => Ok(Field::Exact(text.to_string()?)),
      None => Err(Exception::throw_type(ctx, &format!("flux:test/gui find: {name} must be a string or true"))),
    }
  }

  fn as_match(&self) -> Match<'_> {
    match self {
      Field::Any => Match::Any,
      Field::Present => Match::Present,
      Field::Exact(text) => Match::Exact(text),
    }
  }
}

/// `find(root, query, limit)`: the nodes under `root` (the tree root when
/// null) that `query` (`{ kind?, text?, label? }`) matches, as a JSON array
/// of childless records with their `path`. Null when `root` is no node.
fn find<'js>(ctx: Ctx<'js>, root: Option<u64>, query: Object<'js>, limit: usize) -> rquickjs::Result<Option<String>> {
  let kind: Option<String> = query.get("kind")?;
  let text = Field::read(&ctx, &query, "text")?;
  let label = Field::read(&ctx, &query, "label")?;
  let query = NodeQuery { kind: kind.as_deref(), text: text.as_match(), label: label.as_match() };
  with_tree(&ctx, |tree| {
    tree.find(root, &query, limit).map(|matches| {
      let records: Vec<_> = matches
        .iter()
        .map(|found| {
          let mut record = node_record(&found.node, Some(tree));
          record["path"] = found.path.clone().into();
          record
        })
        .collect();
      serde_json::Value::from(records).to_string()
    })
  })
  .ok_or_else(|| no_tree(&ctx, "find"))
}

/// `node(id, depth)`: the record of one node with `depth` levels of
/// children (0: the node alone), as JSON; null when `id` is no node.
fn node(ctx: Ctx<'_>, id: u64, depth: usize) -> rquickjs::Result<Option<String>> {
  with_tree(&ctx, |tree| {
    tree.snapshot_from(Some(id), Some(depth)).map(|node| node_record(&node, Some(tree)).to_string())
  })
  .ok_or_else(|| no_tree(&ctx, "node"))
}

/// `visible(id)`: whether the node is painted where a user could see it
/// (see `RenderTree::is_visible`).
fn visible(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<bool> {
  with_tree(&ctx, |tree| tree.is_visible(id)).ok_or_else(|| no_tree(&ctx, "visible"))
}

/// `path(id)`: the ids from the root of the node's tree down to it; null
/// when `id` is no node.
fn path(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<Option<Vec<u64>>> {
  with_tree(&ctx, |tree| tree.path_to(id)).ok_or_else(|| no_tree(&ctx, "path"))
}

/// `hit(x, y)`: the ids of the nodes a pointer at that window point
/// reaches, root first; empty when nothing is hit.
fn hit(ctx: Ctx<'_>, x: f32, y: f32) -> rquickjs::Result<Vec<u64>> {
  with_tree(&ctx, |tree| tree.hit_path(Point::new(x, y))).ok_or_else(|| no_tree(&ctx, "hit"))
}

/// `gpu(label, draw)`: the GPU resource inventory as the control API's
/// `/gpu` reports it, as JSON. `label` keeps the resources created with
/// that label, `draw` names the draw entry reported in full.
fn gpu(ctx: Ctx<'_>, label: Option<String>, draw: Option<u64>) -> rquickjs::Result<String> {
  gpu_record(&ctx, label.as_deref(), draw)
    .map(|record| record.to_string())
    .map_err(|e| Exception::throw_message(&ctx, &format!("flux:test/gui gpu: {e}")))
}

pub struct GuiTestModule;

impl ModuleDef for GuiTestModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("find")?;
    decl.declare("node")?;
    decl.declare("visible")?;
    decl.declare("path")?;
    decl.declare("hit")?;
    decl.declare("gpu")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("find", Function::new(ctx.clone(), find)?)?;
    exports.export("node", Function::new(ctx.clone(), node)?)?;
    exports.export("visible", Function::new(ctx.clone(), visible)?)?;
    exports.export("path", Function::new(ctx.clone(), path)?)?;
    exports.export("hit", Function::new(ctx.clone(), hit)?)?;
    exports.export("gpu", Function::new(ctx.clone(), gpu)?)?;
    Ok(())
  }
}
