use crate::gpu::{validate_params, DrawUpdate, ParamValue};
use crate::raster::RasterCmd;
use crate::spatial::{DrawSink, Mat4, NodeId, SharedSlotSink, SinkWriter, Spatial};

use super::mirror::entry_mirror;
use super::Context;

/// Context's `SinkWriter`: each write resolves against the target/draw
/// mirrors and goes down the raster channel. A write that no longer lands
/// (its entry or target removed since bind time) drops with a warning -
/// bind-time validation covered everything else. `wrote` reports whether
/// anything went out, so the caller knows to request a frame.
struct Writer<'a> {
  ctx: &'a Context,
  wrote: bool,
}

impl Writer<'_> {
  fn dropped(result: Result<(), String>) {
    if let Err(e) = result {
      log::warn!("[spatial] sink write dropped: {e}");
    }
  }
}

impl SinkWriter for Writer<'_> {
  fn write_params(&mut self, target: u64, draw: u64, model: &Mat4, normal: Option<&Mat4>) {
    self.wrote = true;
    let mut params = vec![("uModel".to_string(), ParamValue::Array(model.to_vec()))];
    if let Some(n) = normal {
      params.push(("uNormal".to_string(), ParamValue::Array(n.to_vec())));
    }
    let known = entry_mirror(&self.ctx.targets.borrow(), target, draw).map(|_| ());
    Self::dropped(known.map(|_| {
      self.ctx.send(RasterCmd::UpdateDrawParams { target, draw, params });
      self.ctx.note_target_content(target);
    }));
  }

  fn write_count(&mut self, target: u64, draw: u64, count: u32) {
    self.wrote = true;
    let update = DrawUpdate { instance_count: Some(count as i32), ..DrawUpdate::default() };
    Self::dropped(self.ctx.update_draw(target, Some(draw), update));
  }

  // A shared-slot group's array, whole, through the ordinary shared
  // channel (draw targets store unknown names until a declaring
  // material arrives, so this validates like any setTargetParams).
  fn write_shared(&mut self, target: u64, name: &str, values: &[f32]) {
    self.wrote = true;
    Self::dropped(self.ctx.set_target_params(target, &[(name.to_string(), ParamValue::Array(values.to_vec()))]));
  }
}

impl Context {
  /// The spatial core, for node create/move/destroy. Sink binding and the
  /// flush go through the methods below, which resolve sinks against this
  /// context's draw entries.
  pub fn spatial(&self) -> std::cell::RefMut<'_, Spatial> {
    self.spatial.borrow_mut()
  }

  /// Bind (or with None unbind) a node's draw sink. Validated like the
  /// entry path: the target/draw must exist and the entry's program must
  /// take `uModel` (and `uNormal` when `normal`), so a bad binding throws
  /// at its call site instead of failing silently at every flush.
  pub fn spatial_bind(&self, node: NodeId, sink: Option<DrawSink>) -> Result<(), String> {
    if let Some(sink) = sink {
      let targets = self.targets.borrow();
      let entry = entry_mirror(&targets, sink.target, sink.draw)?;
      let identity = ParamValue::Array(crate::spatial::IDENTITY.to_vec());
      let mut probe = vec![("uModel".to_string(), identity.clone())];
      if sink.normal {
        probe.push(("uNormal".to_string(), identity));
      }
      validate_params(&entry.uniforms, &probe)?;
    }
    self.spatial.borrow_mut().set_sink(node, sink)
  }

  /// Bind (or with None unbind) a node's shared-slot sink; validated
  /// against the target (it must exist and be a draw target, whose shared
  /// params accept any name).
  pub fn spatial_bind_slot(&self, node: NodeId, sink: Option<SharedSlotSink>) -> Result<(), String> {
    if let Some(sink) = &sink {
      let targets = self.targets.borrow();
      let mirror = targets.get(&sink.target).ok_or_else(|| format!("shader texture {} not found", sink.target))?;
      if mirror.entries.is_none() {
        return Err(format!("target {} is not a draw target (create it with createDrawTarget)", sink.target));
      }
    }
    self.spatial.borrow_mut().set_shared_slot(node, sink)
  }

  /// Change a sink's "on" instance count; written at once if the entry is
  /// on. The caller must request a frame when this returns true.
  pub fn spatial_set_count(&self, node: NodeId, count: u32) -> Result<bool, String> {
    let mut writer = Writer { ctx: self, wrote: false };
    self.spatial.borrow_mut().set_sink_count(node, count, &mut writer)
  }

  /// Recompute every changed subtree and write the sinks. Returns whether
  /// anything was written (the caller requests a frame if so).
  pub fn spatial_flush(&self) -> bool {
    let mut writer = Writer { ctx: self, wrote: false };
    self.spatial.borrow_mut().flush(&mut writer);
    writer.wrote
  }
}
