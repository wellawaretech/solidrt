use crate::gpu::{validate_params, DrawUpdate, ParamValue, TextureFormat};
use crate::raster::RasterCmd;
use crate::spatial::{DrawSink, InstanceRecordSink, Mat4, NodeId, SharedSlotSink, SinkWriter, Spatial, TextureSlotSink};

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

  // An instance-record staging publish. A plain buffer takes the dirty
  // range through the ordinary partial write (bounds-checked there against
  // the buffer's size); an ordered instance buffer publishes the whole
  // record set gathered into draw order instead - a partial range has no
  // stable position once records draw in key order.
  fn write_instances(&mut self, buffer: u64, lo: u32, hi: u32, values: &[f32]) {
    self.wrote = true;
    if self.ctx.buffer_has_order(buffer) {
      Self::dropped(self.ctx.ordered_instance_publish(buffer, values));
      return;
    }
    let range = &values[lo as usize..hi as usize];
    let mut data = Vec::with_capacity(range.len() * 4);
    for v in range {
      data.extend_from_slice(&v.to_ne_bytes());
    }
    Self::dropped(self.ctx.write_gpu_buffer(buffer, &data, lo as usize * 4));
  }

  // A palette publish: the staged rows as one whole-texture upload (the
  // ordinary content-damage path; a palette is a few KB). The upload pads
  // to the full frame, so rows above the bound extent read zero.
  fn write_texture(&mut self, texture: u64, values: &[f32]) {
    self.wrote = true;
    let Some(entry) = self.ctx.textures.get(texture) else {
      Self::dropped(Err(format!("texture {texture} not found")));
      return;
    };
    let frame = entry.format.byte_len(entry.width(), entry.height());
    let mut data = Vec::with_capacity(frame.max(values.len() * 4));
    for v in values {
      data.extend_from_slice(&v.to_ne_bytes());
    }
    if data.len() < frame {
      data.resize(frame, 0);
    }
    Self::dropped(self.ctx.update_texture(texture, &data, 0));
  }
}

impl Context {
  /// The spatial core, for node create/move/destroy. Sink binding and the
  /// flush go through the methods below, which resolve sinks against this
  /// context's draw entries.
  pub fn spatial(&self) -> std::cell::RefMut<'_, Spatial> {
    self.spatial.borrow_mut()
  }

  /// Bind a node's draw sink on the sink's target (replacing the one it
  /// had there). Validated like the entry path: the target/draw must
  /// exist and the entry's program must take `uModel` (and `uNormal` when
  /// `normal`), so a bad binding throws at its call site instead of
  /// failing silently at every flush.
  pub fn spatial_bind(&self, node: NodeId, sink: DrawSink) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let entry = entry_mirror(&targets, sink.target, sink.draw)?;
      let identity = ParamValue::Array(crate::spatial::IDENTITY.to_vec());
      let mut probe = vec![("uModel".to_string(), identity.clone())];
      if sink.normal {
        probe.push(("uNormal".to_string(), identity));
      }
      validate_params(&entry.uniforms, &probe)?;
    }
    self.spatial.borrow_mut().bind_sink(node, sink)
  }

  /// Remove a node's draw sink on `target`, or every draw sink with None.
  pub fn spatial_unbind(&self, node: NodeId, target: Option<u64>) -> Result<(), String> {
    self.spatial.borrow_mut().unbind_sink(node, target)
  }

  /// Bind a node's shared-slot sink on the sink's target; validated
  /// against the target (it must exist and be a draw target, whose shared
  /// params accept any name).
  pub fn spatial_bind_slot(&self, node: NodeId, sink: SharedSlotSink) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&sink.target).ok_or_else(|| format!("shader texture {} not found", sink.target))?;
      if mirror.entries.is_none() {
        return Err(format!("target {} is not a draw target (create it with createDrawTarget)", sink.target));
      }
    }
    self.spatial.borrow_mut().bind_shared_slot(node, sink)
  }

  /// Remove a node's slot sink on `target`, or every slot sink with None.
  pub fn spatial_unbind_slot(&self, node: NodeId, target: Option<u64>) -> Result<(), String> {
    self.spatial.borrow_mut().unbind_shared_slot(node, target)
  }

  /// Bind a node's texture slot; validated here so a bad binding throws at
  /// its call site: the texture must exist as an uploadable rgba32f matrix
  /// palette (4 texels wide, one mat4 per row) with `row` inside it.
  pub fn spatial_bind_texture_slot(
    &self,
    node: NodeId,
    sink: TextureSlotSink,
    anchor: Option<NodeId>,
  ) -> Result<(), String> {
    {
      let entry = self
        .textures
        .get(sink.texture)
        .ok_or_else(|| format!("texture {} not found", sink.texture))?;
      if entry.format != TextureFormat::Rgba32f {
        return Err(format!("texture {} is {}, matrix rows need rgba32f", sink.texture, entry.format.name()));
      }
      // One column-major mat4 per row: exactly four rgba32f texels.
      if entry.width() != 4 {
        return Err(format!("texture {} is {} texels wide, matrix rows need 4", sink.texture, entry.width()));
      }
      if sink.row >= entry.height() {
        return Err(format!("row {} is outside texture {} ({} rows)", sink.row, sink.texture, entry.height()));
      }
      if self.depth_owner(sink.texture).is_some() || self.targets.borrow().contains_key(&sink.texture) {
        return Err(format!("texture {} is render-written, not uploadable", sink.texture));
      }
    }
    self.spatial.borrow_mut().bind_texture_slot(node, sink, anchor)
  }

  /// Remove a node's texture slot on `texture`, or every texture slot with
  /// None.
  pub fn spatial_unbind_texture_slot(&self, node: NodeId, texture: Option<u64>) -> Result<(), String> {
    self.spatial.borrow_mut().unbind_texture_slot(node, texture)
  }

  /// Bind (or with None unbind) a node's instance-record sink. Validated
  /// at bind time like the draw path: the buffer must exist and the slot
  /// must fit its byte size, so a bad binding throws at its call site.
  pub fn spatial_bind_record(&self, node: NodeId, sink: Option<InstanceRecordSink>) -> Result<(), String> {
    if let Some(sink) = &sink {
      let size = self.gpu_buffer_len(sink.buffer)?;
      let stride = sink.projection.floats() as usize;
      let need = (sink.index as usize + 1) * stride * 4;
      if need > size {
        return Err(format!(
          "instance record slot {} ends at byte {need}, buffer {} has {size}",
          sink.index, sink.buffer
        ));
      }
    }
    self.spatial.borrow_mut().set_instance_record(node, sink)
  }

  /// Move every record sink on buffer `old` to buffer `new` (the growth
  /// swap; see `Spatial::retarget_records`). Validated here: `new` must
  /// exist and hold every bound slot. The republish lands at the next
  /// flush, which requests the frame.
  pub fn spatial_retarget_records(&self, old: u64, new: u64) -> Result<(), String> {
    let mut spatial = self.spatial.borrow_mut();
    let Some(extent) = spatial.records_extent(old) else {
      return Err(format!("no instance records are bound to buffer {old}"));
    };
    let size = self.gpu_buffer_len(new)?;
    if extent * 4 > size {
      return Err(format!("instance records need {} bytes, buffer {new} has {size}", extent * 4));
    }
    spatial.retarget_records(old, new)
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
