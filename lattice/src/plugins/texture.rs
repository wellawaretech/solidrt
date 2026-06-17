use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use flux::rquickjs::function::Opt;
use flux::rquickjs::{Ctx, Function, Object, Persistent, TypedArray, Value};

use crate::rendertree::PlatformContext;
use crate::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> flux::rquickjs::Error {
  ctx.throw(flux::rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

// Flatten a JS { name: number } object into the (name, f32) pairs alloy matches
// against the shader's uniforms by name. Non-numeric values are skipped.
fn collect_params(obj: &Object<'_>) -> Vec<(String, f32)> {
  obj.props::<String, f64>().filter_map(|r| r.ok()).map(|(k, v)| (k, v as f32)).collect()
}

// Flatten a JS { samplerName: textureId } object into (name, id) pairs alloy
// binds to the shader's sampler2D uniforms. Non-numeric values are skipped.
fn collect_textures(obj: &Object<'_>) -> Vec<(String, u64)> {
  obj.props::<String, u64>().filter_map(|r| r.ok()).collect()
}

pub fn init(ctx: Ctx<'_>, atx: AlloyContext, platform: Arc<PlatformContext>) {
  let create_atx = atx.clone();
  let create_texture = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'_>, data: TypedArray<'_, u8>, width: u32, height: u32| -> flux::rquickjs::Result<u64> {
      let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createTexture: detached buffer"))?;
      let expected = (width as usize) * (height as usize) * 4;
      if raw.len != expected {
        return Err(throw_str(&ctx, &format!("createTexture: expected {expected} RGBA8 bytes, got {}", raw.len)));
      }
      let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
      let id = create_atx.create_texture_from_pixels(width, height, pixels);
      Ok(id)
    },
  )
  .expect("create createTexture");

  // Mutable textures pin their JS pixel buffer (a GC anchor per texture id) so
  // uploadTexture can re-read it in place instead of marshaling bytes per call.
  let pinned: Rc<RefCell<HashMap<u64, Persistent<Value<'static>>>>> = Rc::new(RefCell::new(HashMap::new()));

  let mutable_atx = atx.clone();
  let pinned_create = pinned.clone();
  let create_mutable_texture = Function::new(
    ctx.clone(),
    move |data: TypedArray<'_, u8>, width: u32, height: u32| -> flux::rquickjs::Result<u64> {
      // Derive the Ctx from the array itself: closure parameters get independent
      // elided lifetimes, but Persistent::save needs ctx and value unified.
      let ctx = data.as_value().ctx().clone();
      let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createMutableTexture: detached buffer"))?;
      let frame_size = (width as usize) * (height as usize) * 4;
      // The buffer may be larger than one frame (uploadTexture takes an offset),
      // so only require that at least the first frame fits.
      if raw.len < frame_size {
        return Err(throw_str(
          &ctx,
          &format!("createMutableTexture: need at least {frame_size} RGBA8 bytes, got {}", raw.len),
        ));
      }
      let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), frame_size) };
      let id = mutable_atx.create_texture_from_pixels(width, height, pixels);
      pinned_create.borrow_mut().insert(id, Persistent::save(&ctx, data.into_value()));
      Ok(id)
    },
  )
  .expect("create createMutableTexture");

  let upload_atx = atx.clone();
  let upload_platform = platform.clone();
  let pinned_destroy = pinned.clone();
  let upload_texture = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'_>, id: u64, offset: Opt<usize>| -> flux::rquickjs::Result<()> {
      let anchor = pinned
        .borrow()
        .get(&id)
        .cloned()
        .ok_or_else(|| throw_str(&ctx, &format!("uploadTexture: texture {id} is not a mutable texture")))?;
      let value = anchor.restore(&ctx)?;
      let data = TypedArray::<u8>::from_value(value).map_err(|e| throw_str(&ctx, &format!("uploadTexture: {e}")))?;
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

  let create_shader_atx = atx.clone();
  let create_shader = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'_>,
          fragment_src: String,
          width: u32,
          height: u32,
          params: Option<Object<'_>>,
          textures: Option<Object<'_>>|
          -> flux::rquickjs::Result<u64> {
      let params = params.as_ref().map(collect_params).unwrap_or_default();
      let textures = textures.as_ref().map(collect_textures).unwrap_or_default();
      create_shader_atx
        .create_shader_texture(width, height, &fragment_src, &params, &textures)
        .map_err(|e| throw_str(&ctx, &format!("createShader: {e}")))
    },
  )
  .expect("create createShader");

  let set_params_atx = atx.clone();
  let set_params_platform = platform.clone();
  let set_shader_params = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'_>, id: u64, params: Object<'_>| -> flux::rquickjs::Result<()> {
      let params = collect_params(&params);
      set_params_atx.update_shader_params(id, &params).map_err(|e| throw_str(&ctx, &format!("setShaderParams: {e}")))?;
      // New shader output changes the screen without any tree mutation.
      set_params_platform.request_frame();
      Ok(())
    },
  )
  .expect("create setShaderParams");

  let destroy_atx = atx.clone();
  let destroy_texture = Function::new(
    ctx.clone(),
    move |id: u64| {
      pinned_destroy.borrow_mut().remove(&id);
      destroy_atx.destroy_texture(id);
    },
  )
  .expect("create destroyTexture");

  let gpu = Object::new(ctx.clone()).expect("create gpu object");
  gpu.set("createTexture", create_texture).expect("set gpu.createTexture");
  gpu.set("createMutableTexture", create_mutable_texture).expect("set gpu.createMutableTexture");
  gpu.set("uploadTexture", upload_texture).expect("set gpu.uploadTexture");
  gpu.set("destroyTexture", destroy_texture).expect("set gpu.destroyTexture");
  gpu.set("createShader", create_shader).expect("set gpu.createShader");
  gpu.set("setShaderParams", set_shader_params).expect("set gpu.setShaderParams");
  ctx.globals().set("gpu", gpu).expect("set gpu global");
}
