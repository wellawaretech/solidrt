use std::collections::HashMap;
use std::rc::Rc;

use crate::gpu::{BufferIds, DrawBounds, DrawRange, UniformTable};

// UI-side mirror of one shader/pipeline target, seeded by its create reply
// (fused paths, whose program is anonymous) or derived from the pipeline
// mirror (the split path). What update-path validation reads.
pub(super) struct TargetMirror {
  /// The program's active uniforms; Rc-shared with the program and pipeline
  /// mirrors on the split path. Empty (and unused) for a draw target, whose
  /// programs live per entry.
  pub(super) uniforms: Rc<UniformTable>,
  /// The target's current resolved draw range, what set_draw merges partial
  /// updates against. None for a fullscreen fragment pass (no mesh draw) and
  /// for draw targets (whose ranges live per entry).
  pub(super) draw: Option<DrawRange>,
  /// The fetch bounds and range vocabulary for set_draw, captured at create
  /// (see `DrawBounds` for why a captured bound stays correct).
  pub(super) bounds: DrawBounds,
  /// The buffer ids the target's fixed-kind pass reads (vertex, index,
  /// instance), so a buffer write can name the targets whose pixels it
  /// changes (see `note_buffer_content`) and a buffer swap has a current
  /// value to merge into. All zero for draw targets, whose buffers live per
  /// entry.
  pub(super) buffers: BufferIds,
  /// Some = a draw target: the mutable ordered draw list, mirrored per entry
  /// (the flat fields above then describe nothing). None for the fixed
  /// kinds, whose one pass the flat fields describe.
  pub(super) entries: Option<DrawListMirror>,
}

// UI-side mirror of a draw target's entry list: stable id allocation plus
// per-entry validation state. Entry ids are target-scoped and never reused,
// so a stale id from a removed entry errors instead of aliasing.
pub(super) struct DrawListMirror {
  /// Whether the target owns depth storage (the addDraw depth-compatibility
  /// check reads this against the pipeline's declared depth state).
  pub(super) depth: bool,
  /// The registry id of the target's depth texture when it was created
  /// with `DepthStorage::Texture` (what `depth_texture` answers, and what
  /// reclaiming the target takes with it).
  pub(super) depth_texture: Option<u64>,
  pub(super) next_draw: u64,
  pub(super) entries: HashMap<u64, EntryMirror>,
}

// UI-side mirror of one draw entry: what per-entry update validation reads
// (the same shape as TargetMirror's flat half, per entry).
pub(super) struct EntryMirror {
  /// The entry's program's active uniforms, Rc-shared with the pipeline
  /// mirror.
  pub(super) uniforms: Rc<UniformTable>,
  /// The entry's current resolved draw range, what set_draw_range merges
  /// partial updates against.
  pub(super) draw: DrawRange,
  /// The entry's fetch bounds and range vocabulary (see `TargetMirror::bounds`).
  pub(super) bounds: DrawBounds,
  /// The buffer ids the entry reads (see `TargetMirror::buffers`).
  pub(super) buffers: BufferIds,
}

// UI-side mirror of a registered render pipeline: its program's uniforms, the
// record strides of its attribute layouts (vertex and per-instance), and
// whether it declares depth state, for deriving target/entry mirrors and
// validating adds without an RPC.
pub(super) struct PipelineMirror {
  pub(super) uniforms: Rc<UniformTable>,
  pub(super) stride: usize,
  pub(super) instance_strides: [usize; crate::gpu::MAX_INSTANCE_SLOTS],
  pub(super) depth: bool,
}

// The entry mirror for (target, draw), sharing the error spelling of every
// per-entry path.
pub(super) fn entry_mirror(
  targets: &HashMap<u64, TargetMirror>,
  target: u64,
  draw: u64,
) -> Result<&EntryMirror, String> {
  let mirror = targets.get(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
  let Some(list) = mirror.entries.as_ref() else {
    return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
  };
  list.entries.get(&draw).ok_or_else(|| format!("draw {draw} not found on target {target}"))
}
