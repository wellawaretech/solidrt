use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, Object, TypedArray, Value};

// Marshalling for `flux:image`: adapt JS typed arrays and the options object
// to the engine-free `forge::image` codec. Quality is web-style 0..1 on this
// surface and mapped to the encoder's 1..=100.

pub struct ImageModule;

impl ModuleDef for ImageModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("decodeImage")?;
    decl.declare("encodeImage")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("decodeImage", Function::new(ctx.clone(), decode_image)?)?;
    exports.export("encodeImage", Function::new(ctx.clone(), encode_image)?)?;
    Ok(())
  }
}

fn decode_image<'js>(ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> rquickjs::Result<Object<'js>> {
  let raw = bytes.as_raw().ok_or_else(|| Exception::throw_message(&ctx, "decodeImage: detached buffer"))?;
  let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
  let decoded =
    forge::image::decode(bytes).map_err(|e| Exception::throw_message(&ctx, &format!("decodeImage: {e}")))?;
  let result = Object::new(ctx.clone())?;
  result.set("data", TypedArray::<u8>::new(ctx.clone(), decoded.data)?)?;
  result.set("width", decoded.width)?;
  result.set("height", decoded.height)?;
  Ok(result)
}

// `opts` is Opt<Value> rather than Opt<Object> so an explicit `undefined`
// second argument (normal JS for "no options") is accepted like a missing one.
fn encode_image<'js>(
  ctx: Ctx<'js>,
  img: Object<'js>,
  opts: Opt<Value<'js>>,
) -> rquickjs::Result<TypedArray<'js, u8>> {
  let data: TypedArray<'js, u8> = img
    .get("data")
    .map_err(|_| Exception::throw_message(&ctx, "encodeImage: img.data must be a Uint8Array"))?;
  let width: u32 =
    img.get("width").map_err(|_| Exception::throw_message(&ctx, "encodeImage: img.width must be a number"))?;
  let height: u32 =
    img.get("height").map_err(|_| Exception::throw_message(&ctx, "encodeImage: img.height must be a number"))?;

  let mut format = String::from("png");
  let mut quality = 0.9f64;
  if let Some(v) = opts.0 {
    if !v.is_undefined() && !v.is_null() {
      let Some(o) = v.as_object() else {
        return Err(Exception::throw_message(&ctx, "encodeImage: options must be an object"));
      };
      let f: Value = o.get("format")?;
      if !f.is_undefined() && !f.is_null() {
        let Some(s) = f.as_string() else {
          return Err(Exception::throw_message(&ctx, "encodeImage: format must be a string"));
        };
        format = s.to_string()?;
      }
      let q: Value = o.get("quality")?;
      if !q.is_undefined() && !q.is_null() {
        let Some(n) = q.as_number() else {
          return Err(Exception::throw_message(&ctx, "encodeImage: quality must be a number"));
        };
        quality = n;
      }
    }
  }
  if !(0.0..=1.0).contains(&quality) {
    return Err(Exception::throw_message(&ctx, &format!("encodeImage: quality {quality} out of range 0..1")));
  }

  let raw = data.as_raw().ok_or_else(|| Exception::throw_message(&ctx, "encodeImage: detached buffer"))?;
  let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
  let out = match format.as_str() {
    "png" => forge::image::encode_png(pixels, width, height),
    "jpeg" => forge::image::encode_jpeg(pixels, width, height, (quality * 100.0).round() as u8),
    other => return Err(Exception::throw_message(&ctx, &format!("encodeImage: unknown format \"{other}\""))),
  }
  .map_err(|e| Exception::throw_message(&ctx, &format!("encodeImage: {e}")))?;
  TypedArray::<u8>::new(ctx, out)
}