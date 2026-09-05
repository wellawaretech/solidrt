//! The raster command protocol: everything the UI thread can ask the raster
//! thread to do, over the one ordered channel (see the module doc). This is
//! the enum every new GPU feature extends first; the calling conventions
//! (fire-and-forget vs blocking RPC) are documented on the module.

use impellers::{DisplayList, Texture};
use std::sync::mpsc;

use crate::gpu::{
  AttributeTable, BufferIds, DepthStorage, DrawRange, DrawSpec, GpuLimits, GpuResources, NodeShader, ParamValue,
  PipelineDesc, PipelineSpec, ShaderStage, TargetSpec, TextureBinding, UniformTable, WindowShader,
};

/// The adopted handles a draw target create or resize replies with: the
/// color output, and the depth texture when the target has one
/// (`DepthStorage::Texture`). The UI side registers each under its own id.
pub struct TargetHandles {
  pub color: Texture,
  pub depth: Option<Texture>,
}
use crate::gpu::{SamplerState, TextureFormat};

// Outset applied when converting logical damage to physical pixels: absorbs
// the logical->physical scale rounding at the patch edges.
const DAMAGE_PAD_PX: i32 = 1;

/// An integer damage rectangle in physical pixels, top-left origin,
/// non-empty by construction (width/height >= 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageRect {
  pub x: i32,
  pub y: i32,
  pub width: i32,
  pub height: i32,
}

impl DamageRect {
  pub fn union(self, other: DamageRect) -> DamageRect {
    let x0 = self.x.min(other.x);
    let y0 = self.y.min(other.y);
    let x1 = (self.x + self.width).max(other.x + other.width);
    let y1 = (self.y + self.height).max(other.y + other.height);
    DamageRect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
  }

  /// Cut to a `width` x `height` surface; None when nothing remains.
  pub fn clamped(self, width: i32, height: i32) -> Option<DamageRect> {
    let x0 = self.x.max(0);
    let y0 = self.y.max(0);
    let x1 = (self.x + self.width).min(width);
    let y1 = (self.y + self.height).min(height);
    if x1 <= x0 || y1 <= y0 {
      return None;
    }
    Some(DamageRect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
  }

  /// True when the rect covers the whole `width` x `height` surface.
  pub fn covers(self, width: i32, height: i32) -> bool {
    self.x <= 0 && self.y <= 0 && self.x + self.width >= width && self.y + self.height >= height
  }
}

/// Screen damage carried with a frame: what its content changed relative to
/// the previous submitted frame, in physical pixels (converted from the
/// rendertree's logical FrameDamage at submit, see okf/plans/
/// partial-repaint.md). `Full` claims nothing and always redraws the whole
/// window; `None` still presents - the union with older frames' damage may
/// have to repair an aged back buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PresentDamage {
  None,
  Rect(DamageRect),
  Full,
}

impl PresentDamage {
  pub fn union(self, other: PresentDamage) -> PresentDamage {
    match (self, other) {
      (PresentDamage::Full, _) | (_, PresentDamage::Full) => PresentDamage::Full,
      (PresentDamage::None, d) | (d, PresentDamage::None) => d,
      (PresentDamage::Rect(a), PresentDamage::Rect(b)) => PresentDamage::Rect(a.union(b)),
    }
  }

  /// Physical-pixel form of the paint walk's logical damage, padded and
  /// rounded out.
  pub fn from_frame(damage: crate::rendertree::FrameDamage, scale: f32) -> PresentDamage {
    match damage {
      crate::rendertree::FrameDamage::None => PresentDamage::None,
      crate::rendertree::FrameDamage::Full => PresentDamage::Full,
      crate::rendertree::FrameDamage::Rect(r) => {
        let x = (r.origin.x * scale).floor() as i32 - DAMAGE_PAD_PX;
        let y = (r.origin.y * scale).floor() as i32 - DAMAGE_PAD_PX;
        let x1 = ((r.origin.x + r.size.width) * scale).ceil() as i32 + DAMAGE_PAD_PX;
        let y1 = ((r.origin.y + r.size.height) * scale).ceil() as i32 + DAMAGE_PAD_PX;
        PresentDamage::Rect(DamageRect { x, y, width: x1 - x, height: y1 - y })
      }
    }
  }
}

pub(crate) enum RasterCmd {
  /// Draw and present (interactive) or read back (playback) a frame. In
  /// interactive mode, when several frames are queued only the newest is
  /// drawn (load shedding; a shed frame's `damage` is unioned into the one
  /// that draws); in capture mode every frame draws, because playback's
  /// contract is exactly one Captured per submit. `tree_clean` marks a
  /// present-only resubmit of the previous frame's unchanged display list
  /// (see `Context::submit_clean`).
  Frame { dl: DisplayList, tree_clean: bool, damage: PresentDamage },
  /// Register the UI-side frame-request latch for missed-present (jank)
  /// accounting: the raster thread samples it (never consumes) at present
  /// time to tell a demanded gap from an idle one. Forwarded by the platform
  /// loop from AlloyCommand::SetFrameRequestLatch, once at startup.
  SetDemandLatch { latch: std::sync::Arc<std::sync::atomic::AtomicBool> },
  /// Re-run make-current so the context binds the window's current EGL
  /// surface. Android destroys the surface on background and SDL creates a
  /// fresh one on resume, but this thread's binding still points at the dead
  /// one, so every swap would fail with EGL_BAD_SURFACE. Sent on
  /// return-to-visible, ahead of the resume repaint's Frame on this ordered
  /// channel.
  RebindWindowSurface,
  /// Create (or replace, same id) a sampleable pixel texture (RGBA8 or R8)
  /// and adopt it into Impeller. Replies with the adopted handle for UI-side
  /// registration.
  CreateTexture {
    id: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    sampler: SamplerState,
    format: TextureFormat,
    /// Debug label; None on a replace-at-id (an id-stable resize) keeps the
    /// existing entry's label.
    label: Option<String>,
    reply: mpsc::Sender<Result<Texture, String>>,
  },
  /// Create a cube map at `id` from six `size` x `size` faces in GL order
  /// (+X, -X, +Y, -Y, +Z, -Z), or from an explicit mip chain (the full
  /// chain level-major, `check_cube_faces`). Never adopted into Impeller (a cube name
  /// has no 2D adoption), so the reply is bare: the UI side registers the
  /// entry without a handle.
  CreateCubeTexture {
    id: u64,
    size: u32,
    faces: Vec<Vec<u8>>,
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
    reply: mpsc::Sender<Result<(), String>>,
  },
  /// Re-upload pixels into an existing texture; `pixels` is exactly one frame
  /// (the UI side slices multi-frame buffers before sending).
  UpdateTexture { id: u64, pixels: Vec<u8> },
  /// Upload one packed YUV frame into its plane textures: each (id, byte
  /// offset) plane slices its bytes out of the shared `frame`, which is MOVED
  /// from the caller - one multi-plane frame crosses the channel with no copy
  /// (see `Context::update_yuv`).
  UpdateYuv { planes: Vec<(u64, usize)>, frame: Vec<u8> },
  /// Compile a fragment shader into a new target texture and adopt it; the
  /// first render happens at the next dirty flush. Compile and validation
  /// errors must reach JS, hence the reply, which also carries the program's
  /// reflected uniform table for the UI-side validation mirror (the program
  /// is anonymous, so only this reply can deliver it).
  CreateShaderTexture {
    id: u64,
    width: u32,
    height: u32,
    fragment_src: String,
    params: Vec<(String, ParamValue)>,
    textures: Vec<TextureBinding>,
    sampler: SamplerState,
    label: Option<String>,
    reply: mpsc::Sender<Result<(Texture, UniformTable), String>>,
  },
  /// Compile a vertex+fragment pipeline into a new target texture and adopt
  /// it; first render at the next dirty flush. Like CreateShaderTexture, the
  /// reply carries the anonymous program's uniform table for the UI mirror.
  CreatePipelineTexture { id: u64, spec: PipelineSpec, reply: mpsc::Sender<Result<(Texture, UniformTable), String>> },
  /// Compile a single raw stage into the stage registry: a complete GLSL ES
  /// source, or one that explicitly asked for the standard header. Compile
  /// errors must reach JS, hence the reply.
  CompileStage { id: u64, stage: ShaderStage, source: String, header: bool, reply: mpsc::Sender<Result<(), String>> },
  /// Link two compiled stages into a program in the program registry. The
  /// stages remain usable for further links. Link errors reach JS via the
  /// reply; success carries the reflected uniform table for the UI-side
  /// validation mirror.
  LinkProgram {
    id: u64,
    vertex: u64,
    fragment: u64,
    label: Option<String>,
    reply: mpsc::Sender<Result<(UniformTable, AttributeTable), String>>,
  },
  /// Delete a compiled stage. Programs linked from it are unaffected (a
  /// linked program keeps its own compiled copies).
  DestroyStage { id: u64 },
  /// Pair a registered program with draw state in the pipeline registry.
  /// Kind errors ("program is a fragment shader") reach JS via the reply.
  CreateRenderPipeline {
    id: u64,
    program: u64,
    desc: PipelineDesc,
    label: Option<String>,
    reply: mpsc::Sender<Result<(), String>>,
  },
  /// Drop a pipeline from the registry. Targets created from it keep it alive
  /// (and keep rendering); GL resources are freed when the last user goes.
  DestroyRenderPipeline { id: u64 },
  /// Create a fixed single-entry target over a registered pipeline
  /// (`entry.pipeline`) and adopt it (first render at the next dirty flush).
  /// Many targets may share one pipeline.
  CreateShaderTarget { id: u64, spec: TargetSpec, entry: DrawSpec, reply: mpsc::Sender<Result<Texture, String>> },
  /// Create a draw target - a mesh target with an empty, mutable ordered
  /// draw list and optional target-owned depth storage - and adopt it.
  /// Entries arrive via AddDraw; a render is clear + entries in list order.
  /// `depth_id` is the UI-allocated id for the depth texture, present
  /// exactly when `depth` is `Texture`.
  CreateDrawTarget {
    id: u64,
    depth_id: Option<u64>,
    spec: TargetSpec,
    depth: DepthStorage,
    reply: mpsc::Sender<Result<TargetHandles, String>>,
  },
  /// Create a cube draw target at `id`: a `size` x `size` cube map as the
  /// color of a manual draw target rendered face by face (RenderTarget
  /// with a face), over one shared depth renderbuffer. Never adopted into
  /// Impeller (cube maps are sampler-only), so the reply is bare and the
  /// raster side deletes the name on destroy, as for uploaded cube maps.
  CreateCubeDrawTarget {
    id: u64,
    size: u32,
    spec: TargetSpec,
    depth: DepthStorage,
    /// rgba8 or rgba8-srgb (validated UI-side).
    format: TextureFormat,
    reply: mpsc::Sender<Result<(), String>>,
  },
  /// Create a sub-target: a draw target rendering into the `spec`-sized
  /// rectangle at `(x, y)` (top-left origin) of draw target `parent`'s
  /// storage, registered under `id` in the shader map only (it has no
  /// texture of its own). Validated UI-side (parent kind and mode, tile
  /// options); the reply carries a raster-side failure.
  CreateSubTarget { id: u64, parent: u64, x: i32, y: i32, spec: TargetSpec, reply: mpsc::Sender<Result<(), String>> },
  /// Move and resize sub-target `id`'s rectangle (top-left origin,
  /// validated UI-side). Marks the parent dirty: its next render is a full
  /// one, which is what clears the rectangle the tile left.
  SetTargetRect { id: u64, x: i32, y: i32, width: u32, height: u32 },
  /// Add entry `draw` (a UI-allocated, target-scoped id) to a draw target's
  /// list: appended, or inserted immediately before entry `before` when
  /// given. Validated UI-side against the mirrors, so this is
  /// fire-and-forget like every other write; a raster-side failure warns and
  /// the entry is skipped. Marks the target dirty (manual targets fold only).
  AddDraw { target: u64, draw: u64, entry: DrawSpec, before: Option<u64> },
  /// Remove entry `draw` from a draw target's list, releasing its VAO and
  /// its uses of the pipeline and buffer. Fire-and-forget; marks dirty.
  RemoveDraw { target: u64, draw: u64 },
  /// Fold new params into one draw entry's record and mark the target dirty;
  /// values apply at the next render (flush, or explicit for manual).
  UpdateDrawParams { target: u64, draw: u64, params: Vec<(String, ParamValue)> },
  /// Fold new params into a draw target's shared (target-level) record and
  /// mark the target dirty. Shared params apply per entry before the entry's
  /// own params, so an entry naming the same uniform overrides them.
  UpdateTargetParams { target: u64, params: Vec<(String, ParamValue)> },
  /// Fold sampler rebinds into a draw target's shared (target-level) record
  /// and mark the target dirty. Each entry gets the shared names its program
  /// declares and its own bindings do not override; unnamed shared bindings
  /// keep their current source.
  UpdateTargetTextures { target: u64, textures: Vec<TextureBinding> },
  /// Rebind one draw entry's sampler2D inputs by uniform name and mark the
  /// target dirty. Unnamed bindings keep their current source.
  UpdateDrawTextures { target: u64, draw: u64, textures: Vec<TextureBinding> },
  /// Set one draw entry's range (resolved and validated UI-side) and mark
  /// the target dirty.
  SetDrawRange { target: u64, draw: u64, range: DrawRange },
  /// Swap an entry's buffers for `ids` (validated and bounds-checked
  /// UI-side; `draw` None = the single-draw kinds' entry 0): the VAO is
  /// rebuilt against the new buffers and the replaced ones released. Marks
  /// the target dirty.
  SetDrawBuffers { target: u64, draw: Option<u64>, ids: BufferIds },
  /// Reorder a draw target's list to `order`: a full permutation of the
  /// current entry ids (validated UI-side), list order being draw order.
  /// The sorting verb - opaque front-to-back, transparent back-to-front.
  SetDrawOrder { target: u64, order: Vec<u64> },
  /// Drop a program from the registry. Pipelines created from it keep it
  /// alive (and keep rendering); the GL program is deleted when the last user
  /// goes.
  DestroyProgram { id: u64 },
  /// Declare (Some) or clear (None) the window shader. Fire-and-forget on
  /// this ordered channel, so it applies exactly between two frames. The
  /// declared program is held by Rc while active; the layer texture is
  /// allocated lazily by the first shaded frame and freed on clear.
  SetWindowShader { shader: Option<WindowShader> },
  /// Install (Some) or clear (None) the overlay: a small diagnostics
  /// quad composited over every subsequent frame, after the window shader
  /// pass. The declaration's display list is rasterized into a small
  /// retained layer (an Impeller surface draw always clears its target, so
  /// it can never draw straight onto the finished frame) and blended over
  /// FBO 0 each frame. Retained here because it changes on its own cadence
  /// (once per second), decoupled from frames; fire-and-forget on this
  /// ordered channel, so an update shows from exactly the next frame.
  SetOverlay { overlay: Option<crate::context::Overlay> },
  /// Fold new params into an existing shader/pipeline target's record and
  /// mark it dirty; it re-renders at the next flush.
  UpdateShaderParams { id: u64, params: Vec<(String, ParamValue)> },
  /// Rebind an existing shader/pipeline target's sampler2D inputs by uniform
  /// name and mark it dirty. Unnamed bindings keep their current source.
  UpdateShaderTextures { id: u64, textures: Vec<TextureBinding> },
  /// Recreate a shader/pipeline target at a new size (same compiled program,
  /// params, and bindings) and adopt the new target; it re-renders at the
  /// next flush. Replies with the adopted handle(s) so the UI side
  /// re-registers them under the same ids (the depth texture, when the
  /// target has one, gets a fresh name too).
  ResizeShaderTexture { id: u64, width: u32, height: u32, reply: mpsc::Sender<Result<TargetHandles, String>> },
  /// Set a pipeline target's draw range (resolved and validated UI-side, see
  /// `Context::set_draw`) and mark it dirty.
  SetDraw { id: u64, range: DrawRange },
  /// Render a manual target once, now (see `TargetSpec::manual`): flush
  /// pending pure-target writes first so the pass samples fresh inputs, run
  /// the pass, and mark the target's output changed so targets sampling it
  /// re-render at the next flush. Fire-and-forget on this ordered channel, so
  /// renders land in call order and a readback issued after one observes it.
  /// `face` names the face of a cube draw target (validated UI-side: Some
  /// exactly for cube targets), `level` the mip level of that face to
  /// render into (UI-side: within a mipmapped cube target's chain; None
  /// is level 0 plus the chain regeneration).
  RenderTarget { id: u64, face: Option<u32>, level: Option<u32> },
  /// Overwrite manual target `dst` with the current pixels of texture `src`
  /// (same size, validated UI-side): the GPU-side seed/history write, the
  /// copyTexture analog of uploadTexture. Flushes first (it observes src),
  /// draws src over dst via the shared copy program (a sampling draw, never
  /// a blit), and marks dst's output changed. Fire-and-forget, so copies
  /// land in call order like renders.
  CopyTexture { src: u64, dst: u64 },
  /// Drop raster-side bookkeeping for a texture id (and destroy its shader
  /// program/FBO when the id is a shader target). The GL name itself is owned
  /// by the adopted Impeller Texture and dies with the UI side's last handle.
  DestroyTexture { id: u64 },
  /// Register an already-adopted Impeller texture (a snapshot boundary's
  /// retained rasterization) under a registry id so shader passes can
  /// sample it by id, and mark the id's content changed so targets sampling
  /// it re-render at the next flush. Sent after every rasterization of a
  /// boundary whose texture has been vended (`snapshotTexture`): the name
  /// stays Impeller-owned, so the raster side only mirrors it. Ordered
  /// behind the rasterization that produced it and ahead of the frame.
  AdoptTexture { id: u64, texture: Texture, width: u32, height: u32 },
  /// Create an interleaved vertex buffer from raw bytes.
  CreateBuffer { id: u64, data: Vec<u8>, label: Option<String>, reply: mpsc::Sender<Result<(), String>> },
  /// Overwrite part of a vertex buffer and mark pipelines drawing from it
  /// dirty.
  WriteBuffer { id: u64, data: Vec<u8>, byte_offset: usize },
  /// Overwrite the front of a vertex buffer from a leased staging block: the
  /// block MOVES across the channel (no copy, like UpdateYuv) and returns to
  /// the UI-side pool via `recycle` once the GL write is issued. `len` is
  /// the published prefix; the block itself is always full buffer size.
  WriteBufferLease { id: u64, block: Vec<u8>, len: usize, recycle: mpsc::Sender<(u64, Vec<u8>)> },
  /// Read back part of a vertex buffer.
  ReadBuffer { id: u64, byte_offset: usize, len: usize, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Free a vertex buffer.
  DestroyBuffer { id: u64 },
  /// Rasterize a display list into a new adopted texture (snapshot repaint
  /// boundaries). Storage is exactly `width` x `height`. The handle goes
  /// back to the UI thread, which draws it. `aa: false` skips the
  /// multisampled rig (a "snapshot-no-aa" boundary).
  RasterizeDl { dl: DisplayList, width: u32, height: u32, aa: bool, reply: mpsc::Sender<Result<Texture, String>> },
  /// Re-rasterize into an existing adopted texture, reusing its storage
  /// (snapshot boundary at unchanged dimensions). The texture's backing must
  /// be exactly `width` x `height`; the UI thread only reuses at an exact
  /// match.
  RasterizeDlInto {
    dl: DisplayList,
    texture: Texture,
    width: u32,
    height: u32,
    aa: bool,
    reply: mpsc::Sender<Result<(), String>>,
  },
  /// Rasterize a shaded snapshot boundary and run its node shader pass in
  /// one trip: the display list renders into the source texture, then
  /// `shader.program` draws one fullscreen pass over it into the output,
  /// which the boundary composites in place of the raw snapshot. With
  /// `shader.previous`, `history` binds as `uPrevious` (created transparent
  /// when None). Some(texture) reuses storage (the UI side guarantees an
  /// exact dimension match); None allocates and adopts. Replies with every
  /// handle for the boundary's paint cache - the UI side owns the
  /// source/history role rotation across calls.
  RasterizeDlShaded {
    dl: DisplayList,
    width: u32,
    height: u32,
    aa: bool,
    shader: NodeShader,
    source: Option<Texture>,
    output: Option<Texture>,
    history: Option<Texture>,
    reply: mpsc::Sender<Result<(Texture, Texture, Option<Texture>), String>>,
  },
  /// Re-run a node shader pass in place over an existing source/output pair
  /// (plus the history binding while `previous` is declared): the
  /// declaration changed while the boundary's content stayed valid.
  /// Fire-and-forget on this ordered channel, so the refreshed pixels land
  /// ahead of the frame that composites `output`.
  RerunNodeShader {
    shader: NodeShader,
    source: Texture,
    output: Texture,
    history: Option<Texture>,
    width: u32,
    height: u32,
  },
  /// Rasterize + read back `width` x `height` pixels in one trip (node
  /// captures). The intermediate texture never crosses threads.
  RasterizeReadback { dl: DisplayList, width: u32, height: u32, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Read back a texture's RGBA8 pixels by handle.
  ReadTexture { texture: Texture, width: u32, height: u32, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Inventory textures, buffers, and shader/pipeline targets.
  Resources { reply: mpsc::Sender<GpuResources> },
  /// The device ceilings, queried once at thread startup (see `GpuLimits`).
  /// The Context caches the reply, so this crosses once per process.
  Limits { reply: mpsc::Sender<GpuLimits> },
  /// Fence: answered when the thread reaches it, which (commands execute in
  /// order) is when everything queued before it has executed and nothing is
  /// drawing. See RasterSender::drain.
  Drain { reply: mpsc::Sender<()> },
}

impl RasterCmd {
  /// Whether executing this command can change what a resolved frame samples
  /// (texture uploads, target renders, program changes, ...), invalidating
  /// the clean-tree fast path until the next real resolve. The exemptions:
  /// Frame itself, window shader redeclarations (the very writes the fast
  /// path exists for), the overlay (drawn post-pass into FBO 0, never
  /// sampled by the frame), and the Drain fence, which touches nothing. A
  /// shader *program* change still invalidates through its cleared layer.
  pub(crate) fn invalidates_resolved_content(&self) -> bool {
    !matches!(
      self,
      RasterCmd::Frame { .. }
        | RasterCmd::SetWindowShader { .. }
        | RasterCmd::SetOverlay { .. }
        | RasterCmd::Drain { .. }
    )
  }
}
