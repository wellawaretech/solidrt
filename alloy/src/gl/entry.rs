//! The draw-entry half of a mesh target: the per-entry state (resolved
//! buffers, VAO, draw range, params, bindings), the shared target-level
//! state every entry rides over, VAO construction against a pipeline's
//! declared layout, and the entry-buffer checks and releases.

use glow::HasContext;
use std::collections::HashMap;
use std::rc::Rc;

use super::buffer::{release_buffer, GpuBuffer};
use super::pass::{PassInput, ResolvedDraw};
use super::program::{RenderPipeline, ShaderProgram};
use super::slicing::SliceGauge;
use super::storage::{DepthAttachment, Msaa};
use super::{prev_buffer, prev_vertex_array};
use crate::gpu::vocab::{AttrKind, BufferLayout, DrawRange, IndexFormat, ParamValue, PipelineDesc, TextureBinding};

/// The buffers one draw entry fetches through, resolved from registry ids to
/// live Rc clones (the raster-side counterpart of `DrawSpec`'s id fields).
/// Each buffer rides in the entry for its VAO's lifetime - Rc-held like the
/// pipeline, so destroying the registry entry in either order is safe. The
/// registry ids stay alongside for `reads_buffer` and introspection.
#[derive(Default)]
pub struct EntryBuffers {
  /// One buffer per layout the pipeline declares, in declaration order,
  /// each captured in the VAO at its layout's step (divisor 0 per vertex,
  /// 1 per instance); empty when the pipeline is attributeless.
  pub buffers: Vec<(Rc<GpuBuffer>, u64)>,
  /// Index binding: present = the entry draws indexed; the buffer's
  /// ELEMENT_ARRAY binding is captured in the entry's VAO at build time.
  pub index: Option<(Rc<GpuBuffer>, u64, IndexFormat)>,
}

impl EntryBuffers {
  /// Whether the entry reads registry buffer `id` in any role.
  pub fn reads(&self, id: u64) -> bool {
    self.buffers.iter().any(|(_, bid)| *bid == id) || self.index.as_ref().is_some_and(|(_, iid, _)| *iid == id)
  }

  /// The bound buffers' registry ids in layout order.
  pub fn ids(&self) -> Vec<u64> {
    self.buffers.iter().map(|(_, id)| *id).collect()
  }
}

/// One draw of a mesh target's ordered list: the pipeline it draws with
/// (which owns the draw state), plus everything bound to THIS entry - the
/// VAO built against its concrete buffers, those buffers' registry ids, the
/// draw range, uniform values, and sampler inputs. Entries are addressed
/// by a UI-allocated id that stays stable across add/remove (never an index).
pub(super) struct DrawEntry {
  pub(super) id: u64,
  pub(super) pipeline: Rc<RenderPipeline>,
  /// Registry id of the shared pipeline this entry draws with; None for the
  /// fused create path, whose pipeline is anonymous and dies with the target.
  pub(super) pipeline_id: Option<u64>,
  pub(super) vao: glow::VertexArray,
  /// The entry's resolved buffers (layouts and index): what the VAO
  /// reads, and what buffer writes re-render through (see `reads_buffer`).
  pub(super) buffers: EntryBuffers,
  /// Resolved and bounds-checked UI-side (see `resolve_draw_range`) before
  /// it ever reaches this field.
  pub(super) draw: DrawRange,
  /// This entry's current uniform values, folded in by the params merge (its
  /// only writer) and re-applied at every render - entries sharing a program
  /// overwrite each other's uniforms per pass, so re-application is
  /// mandatory, not redundancy.
  pub(super) params: Vec<(String, ParamValue)>,
  /// sampler2D uniform name -> source texture id. Resolved to a live GL
  /// texture at each render by the owner (which holds the texture registry),
  /// so an input whose contents or registry entry changed is picked up
  /// automatically.
  pub(super) bindings: Vec<TextureBinding>,
  /// The entry's debug name (see `DrawSpec::label`), for the inventory only.
  pub(super) label: Option<String>,
  /// The entry's memory of the submission budget (gl::slicing): the
  /// measured rate its renders are sliced by, and the pending bound fence
  /// of a one-slice render. Fresh with the entry, so a rebuilt entry probes.
  pub(super) gauge: SliceGauge,
}

/// A mesh target's ordered draw list, indexed by entry id. List order is
/// draw order; the index finds an entry by id in one lookup, so a flush
/// addressing thousands of entries costs one pass over them, not one scan
/// of the list per entry (the raster thread's share of a whole-scene move
/// was quadratic in the entry count before the index). Every mutation
/// goes through here, which is what keeps the index true: a change of
/// positions re-indexes the entries it moved.
pub(super) struct DrawList {
  entries: Vec<DrawEntry>,
  index: HashMap<u64, usize>,
}

impl DrawList {
  pub(super) fn new() -> Self {
    DrawList { entries: Vec::new(), index: HashMap::new() }
  }

  /// A list over `entries` in their order (the fixed single-entry creates,
  /// and a rebuild's fresh entries).
  pub(super) fn from_entries(entries: Vec<DrawEntry>) -> Self {
    let mut list = DrawList { entries, index: HashMap::new() };
    list.reindex_from(0);
    list
  }

  pub(super) fn len(&self) -> usize {
    self.entries.len()
  }

  pub(super) fn iter(&self) -> std::slice::Iter<'_, DrawEntry> {
    self.entries.iter()
  }

  /// The entry ids in draw order.
  pub(super) fn ids(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
    self.entries.iter().map(|e| e.id)
  }

  /// The first entry: the one entry of the fixed single-draw kinds.
  pub(super) fn first(&self) -> Option<&DrawEntry> {
    self.entries.first()
  }

  pub(super) fn first_mut(&mut self) -> Option<&mut DrawEntry> {
    self.entries.first_mut()
  }

  pub(super) fn get(&self, id: u64) -> Option<&DrawEntry> {
    self.index.get(&id).map(|&i| &self.entries[i])
  }

  pub(super) fn get_mut(&mut self, id: u64) -> Option<&mut DrawEntry> {
    match self.index.get(&id) {
      Some(&i) => Some(&mut self.entries[i]),
      None => None,
    }
  }

  /// Entry `id`'s place in draw order.
  pub(super) fn position(&self, id: u64) -> Option<usize> {
    self.index.get(&id).copied()
  }

  /// Append `entry`: it draws last.
  pub(super) fn push(&mut self, entry: DrawEntry) {
    self.index.insert(entry.id, self.entries.len());
    self.entries.push(entry);
  }

  /// Insert `entry` at `pos` in draw order; the entries behind it move
  /// back one place.
  pub(super) fn insert(&mut self, pos: usize, entry: DrawEntry) {
    self.entries.insert(pos, entry);
    self.reindex_from(pos);
  }

  /// Take entry `id` out of the list; the entries behind it move up one
  /// place. None when the list has no such entry.
  pub(super) fn remove(&mut self, id: u64) -> Option<DrawEntry> {
    let pos = self.index.remove(&id)?;
    let entry = self.entries.remove(pos);
    self.reindex_from(pos);
    Some(entry)
  }

  /// Reorder to `order`, a full permutation of the list's ids (the caller
  /// validated it, see `validate_order`).
  pub(super) fn reorder(&mut self, order: &[u64]) {
    let rank: HashMap<u64, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    self.entries.sort_by_key(|e| rank[&e.id]);
    self.reindex_from(0);
  }

  /// The entries, in draw order, for a target's teardown.
  pub(super) fn into_entries(self) -> Vec<DrawEntry> {
    self.entries
  }

  fn reindex_from(&mut self, pos: usize) {
    for (i, entry) in self.entries.iter().enumerate().skip(pos) {
      self.index.insert(entry.id, i);
    }
  }
}

/// The mesh half of a target: the ordered draw list sharing this target's
/// color (and optional depth) storage, rendered as one pass - clear once,
/// then entries in list order.
pub(super) struct MeshState {
  /// The ordered draw list. The single-draw creates hold exactly one entry
  /// (id 0, `fixed`); draw targets start empty and mutate via
  /// `add_entry`/`remove_entry`.
  pub(super) entries: DrawList,
  /// Target-level params every entry shares (a camera's view-projection),
  /// applied at render before each entry's own params so an entry naming the
  /// same uniform overrides the shared value. Target state, not entry state:
  /// it survives entry add/remove/rebuild. Only draw targets write it (via
  /// `merge_shared_params`); the fixed kinds' target-level params ARE entry
  /// 0's params.
  pub(super) shared_params: Vec<(String, ParamValue)>,
  /// Target-level sampler bindings every entry shares (an environment map, a
  /// shadow map, a LUT): sampler2D uniform name -> source texture id, same
  /// shape as an entry's `bindings`. At render each entry gets the shared
  /// names its program declares and its own bindings do not override - so
  /// coverage may be partial and an entry's own binding wins, mirroring
  /// `shared_params`. Target state; only draw targets write it (via
  /// `merge_shared_bindings`).
  pub(super) shared_bindings: Vec<TextureBinding>,
  /// Present when the target owns depth storage: explicit on a draw target
  /// (`create_draw_target`'s depth option), derived from the pipeline on the
  /// single-draw creates. A renderbuffer stays private to the FBO; a depth
  /// texture is registered under its own id by the owner (see
  /// `DepthAttachment`).
  pub(super) depth: Option<DepthAttachment>,
  /// Multisampled storage when the target was created with `samples >= 2`
  /// and the device granted it; None = single-sample. See `Msaa`.
  pub(super) msaa: Option<Msaa>,
  pub(super) clear_color: [f32; 4],
  /// Color load op (see `TargetSpec::load`): true = draw over the previous
  /// contents instead of clearing. Only ever true on manual targets.
  pub(super) load: bool,
  /// The single-draw creates: the entry set is fixed at creation. The
  /// per-target verbs address entry 0; add/remove are rejected (gated
  /// UI-side, backstopped here).
  pub(super) fixed: bool,
}

impl MeshState {
  /// The entry list resolved for a pass, in list order. An entry's inputs
  /// are its own bindings plus the shared ones its program declares and
  /// does not bind itself (entry overrides shared, and an undeclared shared
  /// name must not eat a texture unit on this entry). The resolver gets the
  /// entry's program so a comparison-sampler uniform (sampler2DShadow) picks
  /// the comparison sampler per ENTRY - one shared depth binding serves a
  /// comparing receiver and a raw-reading one in the same pass.
  pub(super) fn resolved_draws(
    &self,
    resolve: &dyn Fn(&[TextureBinding], &ShaderProgram) -> Vec<PassInput>,
  ) -> Vec<ResolvedDraw<'_>> {
    self
      .entries
      .iter()
      .map(|e| {
        let inputs = if self.shared_bindings.is_empty() {
          resolve(&e.bindings, &e.pipeline.program)
        } else {
          let mut combined = e.bindings.clone();
          for b in &self.shared_bindings {
            if e.pipeline.program.is_active(&b.name) && !combined.iter().any(|c| c.name == b.name) {
              combined.push(b.clone());
            }
          }
          resolve(&combined, &e.pipeline.program)
        };
        ResolvedDraw {
          program: &e.pipeline.program,
          desc: &e.pipeline.desc,
          vao: e.vao,
          buffers: &e.buffers,
          range: e.draw,
          index: e.buffers.index.as_ref().map(|(_, _, fmt)| *fmt),
          params: &e.params,
          inputs,
          gauge: &e.gauge,
          label: e.label.as_deref().or(e.pipeline.label()),
        }
      })
      .collect()
  }
}

unsafe fn record_layout(gl: &glow::Context, program: &ShaderProgram, layout: &BufferLayout) {
  let divisor = layout.step.divisor();
  for attr in &layout.attributes {
    // None means the shader does not (actively) use the attribute; that
    // is fine, the bytes are simply skipped over via the stride.
    if let Some(loc) = gl.get_attrib_location(program.program, &attr.name) {
      let fmt = attr.format;
      gl.enable_vertex_attrib_array(loc);
      // An integer format feeds an integer `in` through the integer
      // pointer (no conversion); every other format converts to float.
      if fmt.kind() == AttrKind::Float {
        gl.vertex_attrib_pointer_f32(
          loc,
          fmt.components(),
          fmt.gl_type(),
          fmt.normalized(),
          layout.stride,
          attr.offset,
        );
      } else {
        gl.vertex_attrib_pointer_i32(loc, fmt.components(), fmt.gl_type(), layout.stride, attr.offset);
      }
      if divisor != 0 {
        gl.vertex_attrib_divisor(loc, divisor);
      }
    }
  }
}

/// Record a pipeline's buffer layouts against an entry's concrete buffers
/// in a fresh VAO: layout i over `buffers[i]` at its step's divisor (the
/// divisor is VAO state in ES 3.0, like the attribute pointers). The index
/// buffer, when given, is bound as ELEMENT_ARRAY while the VAO is current:
/// that binding is VAO state too, so it is captured here once and needs no
/// per-draw rebinding (and no explicit save - restoring the previous VAO
/// restores its own element binding). Restores the VAO and array-buffer
/// bindings it touches.
pub(super) fn build_vao(
  gl: &glow::Context,
  program: &ShaderProgram,
  desc: &PipelineDesc,
  buffers: &EntryBuffers,
) -> Result<glow::VertexArray, String> {
  unsafe {
    let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
    let prev_ab = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
    let vao = gl.create_vertex_array().map_err(|e| format!("glGenVertexArrays failed: {e}"))?;
    gl.bind_vertex_array(Some(vao));
    for (layout, (buffer, _)) in desc.buffers.iter().zip(buffers.buffers.iter()) {
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer.vbo));
      record_layout(gl, program, layout);
    }
    if let Some((index, _, _)) = &buffers.index {
      gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index.vbo));
    }
    gl.bind_vertex_array(prev_vertex_array(prev_vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev_ab));
    Ok(vao)
  }
}

/// The buffer-presence contract between a pipeline and an entry: one
/// buffer per declared layout, no more (a buffer without a layout would
/// never be read). Validated at the call site against the UI mirrors; this
/// copy is the raster-side backstop for the create paths.
pub(super) fn check_entry_buffers(desc: &PipelineDesc, buffers: &EntryBuffers) -> Result<(), String> {
  let declared = desc.buffers.len();
  let bound = buffers.buffers.len();
  if bound < declared {
    return Err(format!("pipeline declares {declared} buffer layout(s) but the entry binds {bound} buffer(s)"));
  }
  if bound > declared {
    return Err(format!(
      "the entry binds {bound} buffer(s) but the pipeline declares {declared} layout(s); the rest would never be read"
    ));
  }
  Ok(())
}

/// Drop an entry's uses of its buffers, deleting each GL buffer when the
/// entry held the last reference (see `release_buffer`).
pub(super) fn release_entry_buffers(gl: &glow::Context, buffers: EntryBuffers) {
  for (buffer, _) in buffers.buffers {
    release_buffer(gl, buffer);
  }
  if let Some((buffer, _, _)) = buffers.index {
    release_buffer(gl, buffer);
  }
}

/// Fold a params update into a record by name (new names append, existing
/// names overwrite): the merge rule shared by target-level and per-entry
/// params writes. An existing array value of the same size is overwritten
/// in place, so a per-frame params write allocates nothing on this thread.
pub(super) fn merge_record(record: &mut Vec<(String, ParamValue)>, params: &[(String, ParamValue)]) {
  for (name, value) in params {
    match record.iter_mut().find(|(n, _)| n == name) {
      Some((_, current)) => assign_param(current, value),
      None => record.push((name.clone(), value.clone())),
    }
  }
}

/// `current = value`, reusing the array's allocation when the component
/// counts agree (a uniform's count never changes, so a rewrite is a copy;
/// a count change is a kind change and gets a fresh value).
fn assign_param(current: &mut ParamValue, value: &ParamValue) {
  match (current, value) {
    (ParamValue::Array(dst), ParamValue::Array(src)) if dst.len() == src.len() => dst.copy_from_slice(src),
    (current, value) => *current = value.clone(),
  }
}

/// Set the array param `name` in a record to `values`: in place when the
/// record holds it at that size (the spatial flush's fold - a fixed name,
/// a fixed size, no allocation), appended when the record has no such name.
pub(super) fn set_record_array(record: &mut Vec<(String, ParamValue)>, name: &str, values: &[f32]) {
  match record.iter_mut().find(|(n, _)| n == name) {
    Some((_, ParamValue::Array(dst))) if dst.len() == values.len() => dst.copy_from_slice(values),
    Some((_, current)) => *current = ParamValue::Array(values.to_vec()),
    None => record.push((name.to_string(), ParamValue::Array(values.to_vec()))),
  }
}
