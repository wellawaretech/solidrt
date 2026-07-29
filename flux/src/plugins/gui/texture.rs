use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object, Persistent, TypedArray};

use super::AlloyContext;
use alloy::rendertree::PlatformContext;
use alloy::CaptureInfo;

// Per-engine texture bookkeeping, held in context userdata so engine teardown
// (which clears userdata while the runtime is still alive) destroys the GPU
// textures this engine created.
#[derive(Clone, JsLifetime)]
struct TextureState(#[qjs(skip_trace)] Rc<TextureInner>);

struct TextureInner {
  atx: AlloyContext,
  // Held so the module's evaluate (which only gets a Ctx) can request frames
  // after an upload / shader-param change, the way the global closures captured
  // it before.
  platform: Arc<PlatformContext>,
  // Every texture id this engine created (immutable, mutable, shader, and
  // captureSnapshot output). The alloy texture registry outlives the engine, so
  // without this a reload leaks the previous app's textures - the app rarely
  // calls destroyTexture itself.
  created: RefCell<HashSet<u64>>,
  // Same bookkeeping for vertex buffers (their own id space in alloy).
  created_buffers: RefCell<HashSet<u64>>,
  // Same bookkeeping for linked programs and compiled raw stages (each their
  // own id space in alloy).
  created_programs: RefCell<HashSet<u64>>,
  created_stages: RefCell<HashSet<u64>>,
  // captureSnapshot is async: alloy services the request on a later paint pass
  // and invokes our completion callback (during `deliver_captures`), which moves
  // the outcome plus the promise sides here. `tick` then drains this and settles
  // each promise with the live `Ctx` it holds (the callback has none). Two hops
  // because a JS promise can only be touched from the JS thread with a `Ctx`.
  capture_settle: RefCell<Vec<CaptureSettle>>,
}

struct CaptureSettle {
  result: Result<CaptureInfo, String>,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

impl Drop for TextureInner {
  fn drop(&mut self) {
    // The window shader first, unconditionally: it is raster-thread state
    // cleared only by an explicit command, and a dying app's declaration is
    // stale by definition. Without this, an app that declared one leaves the
    // next app (or the launcher) rendering through it - the raster thread
    // holds the program by Rc, so even the program destroys below would not
    // stop the pass.
    self.atx.set_window_shader(None).ok();
    // Then textures before buffers: destroying a pipeline before its buffer
    // is the documented order for destroy_gpu_buffer. Programs and stages are
    // order-safe (targets keep their program alive, programs keep their own
    // compiled stage copies), released last for symmetry.
    for id in self.created.borrow_mut().drain() {
      self.atx.destroy_texture(id);
    }
    for id in self.created_buffers.borrow_mut().drain() {
      self.atx.destroy_gpu_buffer(id);
    }
    for id in self.created_programs.borrow_mut().drain() {
      self.atx.destroy_shader_program(id);
    }
    for id in self.created_stages.borrow_mut().drain() {
      self.atx.destroy_shader_stage(id);
    }
  }
}

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

// Flatten a JS { name: number | number[] } object into the (name, value) pairs
// alloy matches against the shader's uniforms by name and dispatches by the
// declared GLSL type (float/int scalar, vec2/3/4, mat4 as 16 numbers).
// Non-numeric values (and arrays with non-numeric elements) are skipped.
fn collect_params(obj: &Object<'_>) -> Vec<(String, alloy::ParamValue)> {
  let mut out = Vec::new();
  for entry in obj.props::<String, rquickjs::Value>() {
    let Ok((name, value)) = entry else { continue };
    if let Some(n) = value.as_number() {
      out.push((name, alloy::ParamValue::Scalar(n as f32)));
    } else if let Some(arr) = value.as_array() {
      let nums: Result<Vec<f32>, _> = arr.iter::<f64>().map(|r| r.map(|n| n as f32)).collect();
      if let Ok(nums) = nums {
        out.push((name, alloy::ParamValue::Array(nums)));
      }
    }
  }
  out
}

// Flatten a JS { samplerName: textureId } object into (name, id) pairs alloy
// binds to the shader's sampler2D uniforms. Non-numeric values are skipped.
fn collect_textures(obj: &Object<'_>) -> Vec<(String, u64)> {
  obj.props::<String, u64>().filter_map(|r| r.ok()).collect()
}

// Decode the { filter?, wrap? } sampling options every create path accepts
// ("linear"/"nearest", "clamp"/"repeat", defaults linear/clamp); an unknown
// value throws at the create call site.
fn collect_sampler(ctx: &Ctx<'_>, opts: &Option<Object<'_>>, api: &str) -> rquickjs::Result<alloy::SamplerState> {
  let (filter, wrap) = match opts {
    Some(o) => (o.get::<_, Option<String>>("filter")?, o.get::<_, Option<String>>("wrap")?),
    None => (None, None),
  };
  alloy::SamplerState::parse(filter.as_deref(), wrap.as_deref()).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))
}

// The target-shaped options shared by createPipeline and createShaderTarget:
// params/textures plus the mesh fields (meaningful for pipelines only, left
// at their defaults otherwise).
struct TargetOpts {
  params: Vec<(String, alloy::ParamValue)>,
  textures: Vec<(String, u64)>,
  attributes: Vec<(String, String)>,
  buffer_id: u64,
  topology: String,
  draw_count: i32,
  depth: bool,
  depth_write: bool,
  blend: String,
  clear_color: [f32; 4],
}

// Decode the shared opts object: { params, textures, attributes: [{name,
// format}], buffer, topology, vertexCount, depth, depthWrite, blend,
// clearColor }, everything optional. Marshalling only; alloy validates.
fn collect_target_opts(opts: &Option<Object<'_>>) -> rquickjs::Result<TargetOpts> {
  let get_obj = |name: &str| -> rquickjs::Result<Option<Object<'_>>> {
    match opts {
      Some(o) => o.get::<_, Option<Object>>(name),
      None => Ok(None),
    }
  };
  let params = get_obj("params")?.as_ref().map(collect_params).unwrap_or_default();
  let textures = get_obj("textures")?.as_ref().map(collect_textures).unwrap_or_default();

  let mut attributes: Vec<(String, String)> = Vec::new();
  if let Some(opts) = opts {
    if let Some(arr) = opts.get::<_, Option<Array>>("attributes")? {
      for item in arr.iter::<Object>() {
        let entry = item?;
        attributes.push((entry.get("name")?, entry.get("format")?));
      }
    }
  }

  let buffer_id = match opts {
    Some(o) => o.get::<_, Option<u64>>("buffer")?.unwrap_or(0),
    None => 0,
  };
  let topology = match opts {
    Some(o) => o.get::<_, Option<String>>("topology")?.unwrap_or_else(|| "triangles".to_string()),
    None => "triangles".to_string(),
  };
  let draw_count = match opts {
    Some(o) => o.get::<_, Option<i32>>("vertexCount")?.unwrap_or(-1),
    None => -1,
  };
  let depth = match opts {
    Some(o) => o.get::<_, Option<bool>>("depth")?.unwrap_or(false),
    None => false,
  };
  let depth_write = match opts {
    Some(o) => o.get::<_, Option<bool>>("depthWrite")?.unwrap_or(true),
    None => true,
  };
  let blend = match opts {
    Some(o) => o.get::<_, Option<String>>("blend")?.unwrap_or_else(|| "none".to_string()),
    None => "none".to_string(),
  };
  let mut clear_color = [0f32; 4];
  if let Some(opts) = opts {
    if let Some(arr) = opts.get::<_, Option<Vec<f64>>>("clearColor")? {
      for (slot, v) in clear_color.iter_mut().zip(arr) {
        *slot = v as f32;
      }
    }
  }

  Ok(TargetOpts { params, textures, attributes, buffer_id, topology, draw_count, depth, depth_write, blend, clear_color })
}

/// Store the texture plugin state (alloy context, platform, and the created-id
/// set for reload cleanup) in userdata, before any module import. The
/// `flux:gpu` surface is registered separately via `module_override`.
pub fn store_state(ctx: &Ctx<'_>, atx: AlloyContext, platform: Arc<PlatformContext>) {
  ctx
    .store_userdata(TextureState(Rc::new(TextureInner {
      atx,
      platform,
      created: RefCell::new(HashSet::new()),
      created_buffers: RefCell::new(HashSet::new()),
      created_programs: RefCell::new(HashSet::new()),
      created_stages: RefCell::new(HashSet::new()),
      capture_settle: RefCell::new(Vec::new()),
    })))
    .expect("store texture state");
}

/// The `flux:gpu` module: texture creation/upload/destruction and fragment
/// shaders. Texture ids are the public token (used as `<texture src>` and shader
/// sampler inputs), so there is no handle to hide here.
pub struct GpuModule;

impl ModuleDef for GpuModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createTexture")?;
    decl.declare("createMutableTexture")?;
    decl.declare("uploadTexture")?;
    decl.declare("resizeTexture")?;
    decl.declare("destroyTexture")?;
    decl.declare("createShader")?;
    decl.declare("compileShader")?;
    decl.declare("linkProgram")?;
    decl.declare("destroyShader")?;
    decl.declare("createShaderTarget")?;
    decl.declare("destroyProgram")?;
    decl.declare("setShaderParams")?;
    decl.declare("setShaderTextures")?;
    decl.declare("setShaderSize")?;
    decl.declare("createPipeline")?;
    decl.declare("createBuffer")?;
    decl.declare("writeBuffer")?;
    decl.declare("destroyBuffer")?;
    decl.declare("setDrawCount")?;
    decl.declare("captureSnapshot")?;
    decl.declare("readTexture")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let state = ctx.userdata::<TextureState>().expect("texture state userdata");
    let atx = state.0.atx.clone();
    let platform = state.0.platform.clone();

    let create_atx = atx.clone();
    let create_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, data: TypedArray<'_, u8>, width: u32, height: u32, opts: Opt<Object<'_>>| -> rquickjs::Result<u64> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createTexture: detached buffer"))?;
        let expected = (width as usize) * (height as usize) * 4;
        if raw.len != expected {
          return Err(throw_str(&ctx, &format!("createTexture: expected {expected} RGBA8 bytes, got {}", raw.len)));
        }
        let sampler = collect_sampler(&ctx, &opts.0, "createTexture")?;
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        let id = create_atx.create_texture_from_pixels(width, height, pixels, sampler);
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createTexture");

    // A mutable texture is created exactly like an immutable one; "mutable" only
    // signals intent to update it later via uploadTexture. The seed buffer may be
    // larger than one frame (uploadTexture takes an offset), so only require that
    // the first frame fits.
    let mutable_atx = atx.clone();
    let create_mutable_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, data: TypedArray<'_, u8>, width: u32, height: u32, opts: Opt<Object<'_>>| -> rquickjs::Result<u64> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createMutableTexture: detached buffer"))?;
        let frame_size = (width as usize) * (height as usize) * 4;
        if raw.len < frame_size {
          return Err(throw_str(
            &ctx,
            &format!("createMutableTexture: need at least {frame_size} RGBA8 bytes, got {}", raw.len),
          ));
        }
        let sampler = collect_sampler(&ctx, &opts.0, "createMutableTexture")?;
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), frame_size) };
        let id = mutable_atx.create_texture_from_pixels(width, height, pixels, sampler);
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createMutableTexture");

    // The caller passes the pixel buffer on every upload (it already holds it),
    // so nothing is pinned on the Rust side. `data` may hold multiple frames;
    // `offset` selects the one to upload. Reading the buffer is zero-copy.
    let upload_atx = atx.clone();
    let upload_platform = platform.clone();
    let upload_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, id: u64, data: TypedArray<'_, u8>, offset: Opt<usize>| -> rquickjs::Result<()> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "uploadTexture: detached buffer"))?;
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        upload_atx
          .update_texture(id, pixels, offset.0.unwrap_or(0))
          .map_err(|e| throw_str(&ctx, &format!("uploadTexture: {e}")))?;
        // New texture content changes the screen without any tree mutation.
        upload_platform.request_frame();
        Ok(())
      },
    )
    .expect("create uploadTexture");

    // An id-stable resize: replaces the texture's storage at the same id, so
    // `<texture src>` references and shader sampler bindings stay valid (the
    // sampling shaders re-render). `data` seeds the new size and, like
    // createMutableTexture, must hold at least one frame. Alloy validates the
    // id (shader targets are rejected there; those resize via setShaderSize).
    let resize_atx = atx.clone();
    let resize_platform = platform.clone();
    let resize_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, id: u64, data: TypedArray<'_, u8>, width: u32, height: u32| -> rquickjs::Result<()> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "resizeTexture: detached buffer"))?;
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        resize_atx
          .resize_texture(id, width, height, pixels)
          .map_err(|e| throw_str(&ctx, &format!("resizeTexture: {e}")))?;
        // The replacement changes the screen without any tree mutation.
        resize_platform.request_frame();
        Ok(())
      },
    )
    .expect("create resizeTexture");

    let create_shader_atx = atx.clone();
    let create_shader = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>,
            fragment_src: String,
            width: u32,
            height: u32,
            params: Option<Object<'_>>,
            textures: Option<Object<'_>>,
            opts: Opt<Object<'_>>|
            -> rquickjs::Result<u64> {
        let params = params.as_ref().map(collect_params).unwrap_or_default();
        let textures = textures.as_ref().map(collect_textures).unwrap_or_default();
        let sampler = collect_sampler(&ctx, &opts.0, "createShader")?;
        let id = create_shader_atx
          .create_shader_texture(width, height, &fragment_src, &params, &textures, sampler)
          .map_err(|e| throw_str(&ctx, &format!("createShader: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createShader");

    let set_params_atx = atx.clone();
    let set_params_platform = platform.clone();
    let set_shader_params =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64, params: Object<'_>| -> rquickjs::Result<()> {
        let params = collect_params(&params);
        set_params_atx
          .update_shader_params(id, &params)
          .map_err(|e| throw_str(&ctx, &format!("setShaderParams: {e}")))?;
        // New shader output changes the screen without any tree mutation.
        set_params_platform.request_frame();
        Ok(())
      })
      .expect("create setShaderParams");

    // Rebind a shader/pipeline target's sampler2D inputs by uniform name -
    // the sampler analog of setShaderParams: mutate, then re-render with the
    // last-applied params. Unnamed bindings keep their current source.
    let set_textures_atx = atx.clone();
    let set_textures_platform = platform.clone();
    let set_shader_textures =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64, textures: Object<'_>| -> rquickjs::Result<()> {
        let textures = collect_textures(&textures);
        set_textures_atx
          .update_shader_textures(id, &textures)
          .map_err(|e| throw_str(&ctx, &format!("setShaderTextures: {e}")))?;
        // New shader output changes the screen without any tree mutation.
        set_textures_platform.request_frame();
        Ok(())
      })
      .expect("create setShaderTextures");

    // Resize a shader/pipeline target in place - the setDrawCount analog for
    // output size: the id, compiled program, last-applied params, and sampler
    // bindings all carry over, and the output re-renders at the new size.
    let shader_size_atx = atx.clone();
    let shader_size_platform = platform.clone();
    let set_shader_size =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64, width: u32, height: u32| -> rquickjs::Result<()> {
        shader_size_atx
          .resize_shader_texture(id, width, height)
          .map_err(|e| throw_str(&ctx, &format!("setShaderSize: {e}")))?;
        // New shader output changes the screen without any tree mutation.
        shader_size_platform.request_frame();
        Ok(())
      })
      .expect("create setShaderSize");

    // createPipeline(vertexSrc, fragmentSrc, width, height, opts?) -> texture id.
    // opts: { params, textures, attributes: [{name, format}], buffer, topology,
    // vertexCount, depth, clearColor }. Marshalling only; alloy validates.
    let create_pipeline_atx = atx.clone();
    let create_pipeline = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>,
            vertex_src: String,
            fragment_src: String,
            width: u32,
            height: u32,
            opts: Opt<Object<'_>>|
            -> rquickjs::Result<u64> {
        let o = collect_target_opts(&opts.0)?;
        let sampler = collect_sampler(&ctx, &opts.0, "createPipeline")?;
        let id = create_pipeline_atx
          .create_pipeline_texture(&alloy::PipelineSpec {
            width,
            height,
            vertex_src: &vertex_src,
            fragment_src: &fragment_src,
            params: &o.params,
            textures: &o.textures,
            attributes: &o.attributes,
            buffer_id: o.buffer_id,
            topology: &o.topology,
            draw_count: o.draw_count,
            depth: o.depth,
            depth_write: o.depth_write,
            blend: &o.blend,
            clear_color: o.clear_color,
            sampler,
          })
          .map_err(|e| throw_str(&ctx, &format!("createPipeline: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createPipeline");

    // Raw stage compile: complete GLSL ES by default, the standard header on
    // explicit request. Compile errors throw here, at a call site the app
    // chose.
    let compile_shader_atx = atx.clone();
    let compile_shader = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, stage: String, source: String, opts: Opt<Object<'_>>| -> rquickjs::Result<u64> {
        let stage = alloy::ShaderStage::parse(&stage).map_err(|e| throw_str(&ctx, &format!("compileShader: {e}")))?;
        let header = match &opts.0 {
          Some(o) => o.get::<_, Option<bool>>("header")?.unwrap_or(false),
          None => false,
        };
        let id = compile_shader_atx
          .compile_shader_stage(stage, &source, header)
          .map_err(|e| throw_str(&ctx, &format!("compileShader: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created_stages.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create compileShader");

    // Link two compiled stages into a program: the handle targets (and later
    // the window effect) are created from. Link errors throw here.
    let link_program_atx = atx.clone();
    let link_program =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, vertex: u64, fragment: u64| -> rquickjs::Result<u64> {
        let id = link_program_atx
          .link_shader_program(vertex, fragment)
          .map_err(|e| throw_str(&ctx, &format!("linkProgram: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created_programs.borrow_mut().insert(id);
        Ok(id)
      })
      .expect("create linkProgram");

    let destroy_shader_atx = atx.clone();
    let destroy_shader = Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64| {
      let state = ctx.userdata::<TextureState>().expect("texture state userdata");
      state.0.created_stages.borrow_mut().remove(&id);
      destroy_shader_atx.destroy_shader_stage(id);
    })
    .expect("create destroyShader");

    // createShaderTarget(program, width, height, opts?) -> texture id: the
    // target half, over an already-compiled program. Same opts shape as
    // createPipeline (the mesh fields apply to pipeline programs only).
    let create_target_atx = atx.clone();
    let create_shader_target = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, program: u64, width: u32, height: u32, opts: Opt<Object<'_>>| -> rquickjs::Result<u64> {
        let o = collect_target_opts(&opts.0)?;
        let sampler = collect_sampler(&ctx, &opts.0, "createShaderTarget")?;
        let id = create_target_atx
          .create_shader_target(
            program,
            &alloy::TargetSpec {
              width,
              height,
              params: &o.params,
              textures: &o.textures,
              attributes: &o.attributes,
              buffer_id: o.buffer_id,
              topology: &o.topology,
              draw_count: o.draw_count,
              depth: o.depth,
              depth_write: o.depth_write,
              blend: &o.blend,
              clear_color: o.clear_color,
              sampler,
            },
          )
          .map_err(|e| throw_str(&ctx, &format!("createShaderTarget: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createShaderTarget");

    let destroy_program_atx = atx.clone();
    let destroy_program = Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64| {
      let state = ctx.userdata::<TextureState>().expect("texture state userdata");
      state.0.created_programs.borrow_mut().remove(&id);
      destroy_program_atx.destroy_shader_program(id);
    })
    .expect("create destroyProgram");

    let create_buffer_atx = atx.clone();
    let create_buffer =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, data: TypedArray<'_, u8>| -> rquickjs::Result<u64> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createBuffer: detached buffer"))?;
        let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        let id = create_buffer_atx.create_gpu_buffer(bytes).map_err(|e| throw_str(&ctx, &format!("createBuffer: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created_buffers.borrow_mut().insert(id);
        Ok(id)
      })
      .expect("create createBuffer");

    // A write re-renders the pipelines drawing from the buffer (alloy does
    // that), so the screen changes without any tree mutation: request a frame.
    let write_buffer_atx = atx.clone();
    let write_buffer_platform = platform.clone();
    let write_buffer = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, id: u64, data: TypedArray<'_, u8>, offset: Opt<usize>| -> rquickjs::Result<()> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "writeBuffer: detached buffer"))?;
        let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        write_buffer_atx
          .write_gpu_buffer(id, bytes, offset.0.unwrap_or(0))
          .map_err(|e| throw_str(&ctx, &format!("writeBuffer: {e}")))?;
        write_buffer_platform.request_frame();
        Ok(())
      },
    )
    .expect("create writeBuffer");

    let destroy_buffer_atx = atx.clone();
    let destroy_buffer = Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64| {
      let state = ctx.userdata::<TextureState>().expect("texture state userdata");
      state.0.created_buffers.borrow_mut().remove(&id);
      destroy_buffer_atx.destroy_gpu_buffer(id);
    })
    .expect("create destroyBuffer");

    let set_draw_count_atx = atx.clone();
    let set_draw_count_platform = platform.clone();
    let set_draw_count =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64, count: i32| -> rquickjs::Result<()> {
        set_draw_count_atx.set_draw_count(id, count).map_err(|e| throw_str(&ctx, &format!("setDrawCount: {e}")))?;
        set_draw_count_platform.request_frame();
        Ok(())
      })
      .expect("create setDrawCount");

    let destroy_atx = atx.clone();
    let destroy_platform = platform.clone();
    let destroy_texture = Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64| {
      let state = ctx.userdata::<TextureState>().expect("texture state userdata");
      state.0.created.borrow_mut().remove(&id);
      destroy_atx.destroy_texture(id);
      // Destruction is deferred to the paint loop's reclamation sweep, which
      // only runs when a frame is produced - request one so a destroy on an
      // otherwise idle app is not stranded.
      destroy_platform.request_frame();
    })
    .expect("create destroyTexture");

    exports.export("createTexture", create_texture)?;
    exports.export("createMutableTexture", create_mutable_texture)?;
    exports.export("uploadTexture", upload_texture)?;
    exports.export("resizeTexture", resize_texture)?;
    exports.export("destroyTexture", destroy_texture)?;
    exports.export("createShader", create_shader)?;
    exports.export("compileShader", compile_shader)?;
    exports.export("linkProgram", link_program)?;
    exports.export("destroyShader", destroy_shader)?;
    exports.export("createShaderTarget", create_shader_target)?;
    exports.export("destroyProgram", destroy_program)?;
    exports.export("setShaderParams", set_shader_params)?;
    exports.export("setShaderTextures", set_shader_textures)?;
    exports.export("setShaderSize", set_shader_size)?;
    exports.export("createPipeline", create_pipeline)?;
    exports.export("createBuffer", create_buffer)?;
    exports.export("writeBuffer", write_buffer)?;
    exports.export("destroyBuffer", destroy_buffer)?;
    exports.export("setDrawCount", set_draw_count)?;
    // Named generic fns, not closures: `captureSnapshot` returns a Promise and
    // `readTexture` an Object, whose 'js lifetime must unify with the Ctx arg -
    // a closure gives them independent invariant lifetimes and will not compile
    // (same reason camera::open is a named fn). They read state from userdata.
    exports.export("captureSnapshot", Function::new(ctx.clone(), capture_snapshot_impl)?)?;
    exports.export("readTexture", Function::new(ctx.clone(), read_texture_impl)?)?;
    Ok(())
  }
}

/// Queue a node capture and return a promise settled from `tick` once alloy
/// renders it on a later paint pass. The completion callback (run on the UI
/// thread with no `Ctx`) only stashes the outcome and the promise sides into
/// `capture_settle`; `tick` does the JS-facing settle (see CaptureSettle).
fn capture_snapshot_impl<'js>(ctx: Ctx<'js>, node_id: u64) -> rquickjs::Result<Promise<'js>> {
  let state = ctx.userdata::<TextureState>().expect("texture state userdata");
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let resolve = Persistent::save(&ctx, resolve);
  let reject = Persistent::save(&ctx, reject);
  // The callback holds an Rc to our state so it can enqueue the outcome. Alloy
  // owns the callback until the capture is serviced, keeping the state alive
  // while the request is in flight (no cycle: alloy is not owned by the state).
  let inner = state.0.clone();
  state.0.atx.request_capture(
    node_id,
    Box::new(move |result| {
      inner.capture_settle.borrow_mut().push(CaptureSettle { result, resolve, reject });
    }),
  );
  // The capture is serviced during a paint; make sure one happens.
  state.0.platform.request_frame();
  Ok(promise)
}

/// Read back any registered texture's current RGBA8 pixels (tightly packed,
/// top-to-bottom) as `{ width, height, data }`. Synchronous: the texture was
/// already rendered on this thread's GL context at creation time, so there is
/// nothing to wait for.
fn read_texture_impl<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Object<'js>> {
  let state = ctx.userdata::<TextureState>().expect("texture state userdata");
  let (width, height, pixels) =
    state.0.atx.read_texture_by_id(id).map_err(|e| throw_str(&ctx, &format!("readTexture: {e}")))?;
  let obj = Object::new(ctx.clone())?;
  obj.set("width", width)?;
  obj.set("height", height)?;
  obj.set("data", TypedArray::new(ctx.clone(), pixels)?)?;
  Ok(obj)
}

/// Reject a captureSnapshot promise with an Error carrying `msg`.
fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, msg: &str) {
  let (Ok(func), Ok(error)) = (reject.restore(ctx), Exception::from_message(ctx.clone(), msg)) else {
    return;
  };
  if let Err(e) = func.call::<_, ()>((error,)) {
    log::warn!("[gpu] capture reject call failed: {e}");
  }
}

/// Per-frame hook, called alongside `camera::tick` / `raf::flush`. Drains the
/// capture outcomes the completion callbacks enqueued during the last paint and
/// settles each promise, tracking the new texture id for reload cleanup.
pub fn tick(ctx: &Ctx<'_>) {
  let Some(state) = ctx.userdata::<TextureState>() else {
    return;
  };
  let settles = std::mem::take(&mut *state.0.capture_settle.borrow_mut());
  for CaptureSettle { result, resolve, reject } in settles {
    match result {
      Ok(CaptureInfo { texture_id, width, height }) => {
        // Same reload-cleanup bookkeeping the create* functions do.
        state.0.created.borrow_mut().insert(texture_id);
        let settle = || -> rquickjs::Result<()> {
          let obj = Object::new(ctx.clone())?;
          obj.set("id", texture_id)?;
          obj.set("width", width)?;
          obj.set("height", height)?;
          resolve.restore(ctx)?.call::<_, ()>((obj,))
        };
        if let Err(e) = settle() {
          log::warn!("[gpu] capture resolve failed: {e}");
        }
      }
      Err(error) => reject_with(ctx, reject, &error),
    }
  }
}
