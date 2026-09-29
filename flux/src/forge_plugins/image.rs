use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Exception, Function, Object, TypedArray, Value};
#[cfg(feature = "ktx2")]
use std::future::Future;

use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{bytes_of, OptArg};
#[cfg(feature = "ktx2")]
use crate::plugins::marshal::{with_pending, CopyBytes};
use crate::plugins::value::Neutral;
use crate::standards_plugins::body::JsBytes;

// Marshalling for `flux:image`: adapt JS typed arrays and the options object
// to the engine-free `forge::image` codec. Quality is web-style 0..1 on this
// surface and mapped to the encoder's 1..=100. `alpha` names the pixel
// convention on the JS side of each call: "premultiplied" (default, what the
// GPU holds) or "straight" (the file's bytes verbatim).
//
// transcodeTexture / encodeTexture marshal `forge::ktx2` (compressed
// textures): both copy their input and run on the blocking pool, since a
// scene's worth of either is seconds of CPU. On a build without the `ktx2`
// feature both exist and throw, so the module's shape never depends on the
// build (the `ktx2` capability says which it is).

pub struct ImageModule;

impl ModuleDef for ImageModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("decodeImage")?;
    decl.declare("encodeImage")?;
    decl.declare("transcodeTexture")?;
    decl.declare("encodeTexture")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("decodeImage", Function::new(ctx.clone(), decode_image)?)?;
    exports.export("encodeImage", Function::new(ctx.clone(), encode_image)?)?;
    exports.export("transcodeTexture", Function::new(ctx.clone(), transcode_texture)?)?;
    exports.export("encodeTexture", Function::new(ctx.clone(), encode_texture)?)?;
    Ok(())
  }
}

/// `{ alpha?: "premultiplied" | "straight" }` -> true when premultiplied.
fn premultiplied_opt<'js>(ctx: &Ctx<'js>, opts: &OptArg<Object<'js>>, fname: &str) -> rquickjs::Result<bool> {
  let Some(o) = &opts.0 else { return Ok(true) };
  let v: Value = o.get("alpha")?;
  if v.is_undefined() || v.is_null() {
    return Ok(true);
  }
  let Some(s) = v.as_string() else {
    return Err(Exception::throw_message(ctx, &format!("{fname}: alpha must be a string")));
  };
  match s.to_string()?.as_str() {
    "premultiplied" => Ok(true),
    "straight" => Ok(false),
    other => Err(Exception::throw_message(ctx, &format!("{fname}: unknown alpha \"{other}\""))),
  }
}

fn decode_image<'js>(
  ctx: Ctx<'js>,
  bytes: TypedArray<'js, u8>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let premultiply = premultiplied_opt(&ctx, &opts, "decodeImage")?;
  let decoded = forge::image::decode(bytes_of(&ctx, &bytes, "decodeImage")?, premultiply)
    .map_err(|e| Exception::throw_message(&ctx, &format!("decodeImage: {e}")))?;
  let result = Object::new(ctx.clone())?;
  result.set("data", TypedArray::<u8>::new(ctx.clone(), decoded.data)?)?;
  result.set("width", decoded.width)?;
  result.set("height", decoded.height)?;
  Ok(result)
}

fn encode_image<'js>(
  ctx: Ctx<'js>,
  img: Object<'js>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<TypedArray<'js, u8>> {
  let data: TypedArray<'js, u8> =
    img.get("data").map_err(|_| Exception::throw_message(&ctx, "encodeImage: img.data must be a Uint8Array"))?;
  let width: u32 =
    img.get("width").map_err(|_| Exception::throw_message(&ctx, "encodeImage: img.width must be a number"))?;
  let height: u32 =
    img.get("height").map_err(|_| Exception::throw_message(&ctx, "encodeImage: img.height must be a number"))?;

  let unpremultiply = premultiplied_opt(&ctx, &opts, "encodeImage")?;
  let mut format = String::from("png");
  let mut quality = 0.9f64;
  if let Some(o) = opts.0 {
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
  if !(0.0..=1.0).contains(&quality) {
    return Err(Exception::throw_message(&ctx, &format!("encodeImage: quality {quality} out of range 0..1")));
  }

  let pixels = bytes_of(&ctx, &data, "encodeImage")?;
  let out = match format.as_str() {
    "png" => forge::image::encode_png(pixels, width, height, unpremultiply),
    "jpeg" => forge::image::encode_jpeg(pixels, width, height, (quality * 100.0).round() as u8),
    other => return Err(Exception::throw_message(&ctx, &format!("encodeImage: unknown format \"{other}\""))),
  }
  .map_err(|e| Exception::throw_message(&ctx, &format!("encodeImage: {e}")))?;
  TypedArray::<u8>::new(ctx, out)
}

/// The message both texture calls throw on a build without `ktx2`.
#[cfg(not(feature = "ktx2"))]
const NO_KTX2: &str = "this runtime was built without compressed textures (check Flux.capabilities for \"ktx2\")";

/// An optional string option: `None` when absent, a throw when not a string.
#[cfg(feature = "ktx2")]
fn string_opt<'js>(ctx: &Ctx<'js>, opts: &Object<'js>, key: &str, api: &str) -> rquickjs::Result<Option<String>> {
  let v: Value = opts.get(key)?;
  if v.is_undefined() || v.is_null() {
    return Ok(None);
  }
  match v.as_string() {
    Some(s) => Ok(Some(s.to_string()?)),
    None => Err(Exception::throw_message(ctx, &format!("{api}: {key} must be a string"))),
  }
}

/// The transcode target when the call names none: the block format this
/// device samples natively - BC7 where the GPU reports it (desktops), ETC2
/// (GLES 3.0 core) everywhere else. Only a runtime with a GPU can answer.
#[cfg(feature = "ktx2")]
fn device_target(ctx: &Ctx<'_>) -> rquickjs::Result<forge::ktx2::Target> {
  #[cfg(feature = "gui")]
  if let Some(gui) = crate::alloy_plugins::try_gui(ctx) {
    return Ok(if gui.alloy.gpu_limits().bc7_textures {
      forge::ktx2::Target::Bc7Rgba8
    } else {
      forge::ktx2::Target::Etc2Rgba8
    });
  }
  Err(Exception::throw_message(
    ctx,
    "transcodeTexture: this runtime has no GPU to pick a target for; pass target (\"etc2-rgba8\", \"bc7-rgba8\" or \"rgba8\")",
  ))
}

// `transcodeTexture(ktx2, { target? })`: a KTX2 file to the payload of a
// texture create, as `{ data, width, height, format, mipmap }`.
#[cfg(feature = "ktx2")]
fn transcode_texture<'js>(
  ctx: Ctx<'js>,
  bytes: TypedArray<'js, u8>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Neutral>>>> {
  let named = match &opts.0 {
    Some(o) => string_opt(&ctx, o, "target", "transcodeTexture")?,
    None => None,
  };
  let target = match named {
    Some(name) => forge::ktx2::Target::parse(&name)
      .map_err(|e| Exception::throw_message(&ctx, &format!("transcodeTexture: {e}")))?,
    None => device_target(&ctx)?,
  };
  let file = bytes.copy_bytes();
  Ok(with_pending(&ctx, async move {
    tokio::task::spawn_blocking(move || forge::ktx2::transcode(&file, target))
      .await
      .map_err(|e| format!("transcodeTexture: {e}"))?
      .map(|t| Neutral(t.into()))
      .map_err(|e| format!("transcodeTexture: {e}"))
  }))
}

#[cfg(not(feature = "ktx2"))]
fn transcode_texture<'js>(
  ctx: Ctx<'js>,
  _bytes: TypedArray<'js, u8>,
  _opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<std::future::Ready<JsResult<Neutral>>>> {
  Err(Exception::throw_message(&ctx, &format!("transcodeTexture: {NO_KTX2}")))
}

// `encodeTexture(img, { codec, srgb?, mipmap?, wrap?, quality? })`: RGBA8
// pixels to a KTX2 file.
#[cfg(feature = "ktx2")]
fn encode_texture<'js>(
  ctx: Ctx<'js>,
  img: Object<'js>,
  opts: Object<'js>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<JsBytes>>>> {
  let api = "encodeTexture";
  let data: TypedArray<'js, u8> =
    img.get("data").map_err(|_| Exception::throw_message(&ctx, "encodeTexture: img.data must be a Uint8Array"))?;
  let width: u32 =
    img.get("width").map_err(|_| Exception::throw_message(&ctx, "encodeTexture: img.width must be a number"))?;
  let height: u32 =
    img.get("height").map_err(|_| Exception::throw_message(&ctx, "encodeTexture: img.height must be a number"))?;
  let Some(codec) = string_opt(&ctx, &opts, "codec", api)? else {
    return Err(Exception::throw_message(&ctx, "encodeTexture: codec is required (\"etc1s\" or \"uastc\")"));
  };
  let codec =
    forge::ktx2::Codec::parse(&codec).map_err(|e| Exception::throw_message(&ctx, &format!("{api}: {e}")))?;
  let wrap = match string_opt(&ctx, &opts, "wrap", api)? {
    Some(name) => {
      forge::ktx2::MipWrap::parse(&name).map_err(|e| Exception::throw_message(&ctx, &format!("{api}: {e}")))?
    }
    None => forge::ktx2::MipWrap::default(),
  };
  let options = forge::ktx2::EncodeOptions {
    codec,
    srgb: opts.get::<_, Option<bool>>("srgb")?.unwrap_or(false),
    mipmap: opts.get::<_, Option<bool>>("mipmap")?.unwrap_or(false),
    wrap,
    quality: opts.get::<_, Option<f64>>("quality")?.map(|q| q as f32).unwrap_or(forge::ktx2::DEFAULT_QUALITY),
  };
  let pixels = data.copy_bytes();
  Ok(with_pending(&ctx, async move {
    tokio::task::spawn_blocking(move || forge::ktx2::encode(&pixels, width, height, &options))
      .await
      .map_err(|e| format!("encodeTexture: {e}"))?
      .map(JsBytes)
      .map_err(|e| format!("encodeTexture: {e}"))
  }))
}

#[cfg(not(feature = "ktx2"))]
fn encode_texture<'js>(
  ctx: Ctx<'js>,
  _img: Object<'js>,
  _opts: Object<'js>,
) -> rquickjs::Result<Promised<std::future::Ready<JsResult<JsBytes>>>> {
  Err(Exception::throw_message(&ctx, &format!("encodeTexture: {NO_KTX2}")))
}
