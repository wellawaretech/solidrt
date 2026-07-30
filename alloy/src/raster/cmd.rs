//! The raster command protocol: everything the UI thread can ask the raster
//! thread to do, over the one ordered channel (see the module doc). This is
//! the enum every new GPU feature extends first; the calling conventions
//! (fire-and-forget vs blocking RPC) are documented on the module.

use impellers::{DisplayList, Texture};
use std::sync::mpsc;

use crate::gpu::{GpuResources, ParamValue, PipelineDesc, PipelineSpec, ShaderStage, TargetSpec, UniformTable, WindowShader};
use crate::texture::SamplerState;

pub(crate) enum RasterCmd {
  /// Draw and present (interactive) or read back (playback) a frame. In
  /// interactive mode, when several frames are queued only the newest is
  /// drawn (load shedding); in capture mode every frame draws, because
  /// playback's contract is exactly one Captured per submit. `tree_clean`
  /// marks a present-only resubmit of the previous frame's unchanged display
  /// list (see `Context::submit_clean`).
  Frame { dl: DisplayList, tree_clean: bool },
  /// Re-run make-current so the context binds the window's current EGL
  /// surface. Android destroys the surface on background and SDL creates a
  /// fresh one on resume, but this thread's binding still points at the dead
  /// one, so every swap would fail with EGL_BAD_SURFACE. Sent on
  /// return-to-visible, ahead of the resume repaint's Frame on this ordered
  /// channel.
  RebindWindowSurface,
  /// Create (or replace, same id) a sampleable RGBA8 texture and adopt it
  /// into Impeller. Replies with the adopted handle for UI-side registration.
  CreateTexture {
    id: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    sampler: SamplerState,
    reply: mpsc::Sender<Result<Texture, String>>,
  },
  /// Re-upload pixels into an existing texture; `pixels` is exactly one frame
  /// (the UI side slices multi-frame buffers before sending).
  UpdateTexture { id: u64, pixels: Vec<u8> },
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
    textures: Vec<(String, u64)>,
    sampler: SamplerState,
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
  LinkProgram { id: u64, vertex: u64, fragment: u64, reply: mpsc::Sender<Result<UniformTable, String>> },
  /// Delete a compiled stage. Programs linked from it are unaffected (a
  /// linked program keeps its own compiled copies).
  DestroyStage { id: u64 },
  /// Pair a registered program with draw state in the pipeline registry.
  /// Kind errors ("program is a fragment shader") reach JS via the reply.
  CreateRenderPipeline { id: u64, program: u64, desc: PipelineDesc, reply: mpsc::Sender<Result<(), String>> },
  /// Drop a pipeline from the registry. Targets created from it keep it alive
  /// (and keep rendering); GL resources are freed when the last user goes.
  DestroyRenderPipeline { id: u64 },
  /// Create a target over a registered pipeline and adopt it (first render at
  /// the next dirty flush). Many targets may share one pipeline.
  CreateShaderTarget { id: u64, pipeline: u64, spec: TargetSpec, reply: mpsc::Sender<Result<Texture, String>> },
  /// Drop a program from the registry. Pipelines created from it keep it
  /// alive (and keep rendering); the GL program is deleted when the last user
  /// goes.
  DestroyProgram { id: u64 },
  /// Declare (Some) or clear (None) the window shader. Fire-and-forget on
  /// this ordered channel, so it applies exactly between two frames. The
  /// declared program is held by Rc while active; the layer texture is
  /// allocated lazily by the first shaded frame and freed on clear.
  SetWindowShader { shader: Option<WindowShader> },
  /// Fold new params into an existing shader/pipeline target's record and
  /// mark it dirty; it re-renders at the next flush.
  UpdateShaderParams { id: u64, params: Vec<(String, ParamValue)> },
  /// Rebind an existing shader/pipeline target's sampler2D inputs by uniform
  /// name and mark it dirty. Unnamed bindings keep their current source.
  UpdateShaderTextures { id: u64, textures: Vec<(String, u64)> },
  /// Recreate a shader/pipeline target at a new size (same compiled program,
  /// params, and bindings) and adopt the new target; it re-renders at the
  /// next flush. Replies with the adopted handle so the UI side re-registers
  /// it under the same id.
  ResizeShaderTexture { id: u64, width: u32, height: u32, reply: mpsc::Sender<Result<Texture, String>> },
  /// Set a pipeline's vertex draw count and mark it dirty.
  SetDrawCount { id: u64, count: i32 },
  /// Render a manual target once, now (see `TargetSpec::manual`): flush
  /// pending pure-target writes first so the pass samples fresh inputs, run
  /// the pass, and mark the target's output changed so targets sampling it
  /// re-render at the next flush. Fire-and-forget on this ordered channel, so
  /// renders land in call order and a readback issued after one observes it.
  RenderTarget { id: u64 },
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
  /// Create an interleaved vertex buffer from raw bytes.
  CreateBuffer { id: u64, data: Vec<u8>, reply: mpsc::Sender<Result<(), String>> },
  /// Overwrite part of a vertex buffer and mark pipelines drawing from it
  /// dirty.
  WriteBuffer { id: u64, data: Vec<u8>, byte_offset: usize },
  /// Read back part of a vertex buffer.
  ReadBuffer { id: u64, byte_offset: usize, len: usize, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Free a vertex buffer.
  DestroyBuffer { id: u64 },
  /// Rasterize a display list into a new adopted texture (snapshot repaint
  /// boundaries). The handle goes back to the UI thread, which draws it.
  /// `aa: false` skips the multisampled rig (a "snapshot-no-aa" boundary).
  RasterizeDl { dl: DisplayList, width: u32, height: u32, aa: bool, reply: mpsc::Sender<Result<Texture, String>> },
  /// Re-rasterize into an existing adopted texture, reusing its storage
  /// (snapshot boundary whose retained allocation still fits). The texture's
  /// aligned backing must fit `width` x `height`; the UI thread checks this.
  RasterizeDlInto {
    dl: DisplayList,
    texture: Texture,
    width: u32,
    height: u32,
    aa: bool,
    reply: mpsc::Sender<Result<(), String>>,
  },
  /// Rasterize + read back `width` x `height` pixels in one trip (node
  /// captures). The intermediate padded texture never crosses threads.
  RasterizeReadback { dl: DisplayList, width: u32, height: u32, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Read back a texture's RGBA8 pixels by handle.
  ReadTexture { texture: Texture, width: u32, height: u32, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Inventory textures, buffers, and shader/pipeline targets.
  Resources { reply: mpsc::Sender<GpuResources> },
}
