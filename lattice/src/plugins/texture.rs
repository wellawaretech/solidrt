use flux::rquickjs::{Ctx, Function, Object, TypedArray};

use crate::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> flux::rquickjs::Error {
  ctx.throw(flux::rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

fn decode_image_impl<'js>(ctx: Ctx<'js>, data: TypedArray<'js, u8>) -> flux::rquickjs::Result<Object<'js>> {
  let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "decodeImage: detached buffer"))?;
  let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
  let img = image::load_from_memory(bytes).map_err(|e| throw_str(&ctx, &format!("decodeImage: {e}")))?;
  let rgba = img.to_rgba8();
  let width = rgba.width();
  let height = rgba.height();
  let pixels = rgba.into_raw();
  let ta = TypedArray::<u8>::new(ctx.clone(), pixels)?;
  let result = Object::new(ctx.clone())?;
  result.set("data", ta)?;
  result.set("width", width)?;
  result.set("height", height)?;
  Ok(result)
}

pub fn init(ctx: Ctx<'_>, atx: AlloyContext) {
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

  let decode_image = Function::new(ctx.clone(), decode_image_impl).expect("create decodeImage");

  let gpu = Object::new(ctx.clone()).expect("create gpu object");
  gpu.set("createTexture", create_texture).expect("set gpu.createTexture");
  gpu.set("decodeImage", decode_image).expect("set gpu.decodeImage");
  ctx.globals().set("gpu", gpu).expect("set gpu global");
}
