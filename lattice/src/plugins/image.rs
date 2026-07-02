use flux::rquickjs::{Ctx, Function, Object, TypedArray};

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> flux::rquickjs::Error {
  ctx.throw(flux::rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

// Pure CPU image decode: turn encoded bytes (PNG, JPEG, ...) into tightly-packed
// RGBA8 pixels plus dimensions. No GPU involvement, which is why this lives in
// its own `image` global rather than alongside the texture functions on `gpu`.
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

pub fn init(ctx: Ctx<'_>) {
  let decode_image = Function::new(ctx.clone(), decode_image_impl).expect("create decodeImage");

  let image = Object::new(ctx.clone()).expect("create image object");
  image.set("decodeImage", decode_image).expect("set image.decodeImage");
  ctx.globals().set("image", image).expect("set image global");
}
