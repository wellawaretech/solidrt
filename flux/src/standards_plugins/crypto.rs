//! `crypto.subtle.digest`: the one Web Crypto entry point flux provides. An
//! app hashes bytes (content addressing, integrity checks); keys, ciphers and
//! randomness wait for a need. This layer only marshals: the algorithm name
//! and the input buffer go to `forge::crypto::digest`, and the result comes
//! back as a promise so the call site matches the standard, with every
//! failure rejecting rather than throwing (an async binding never throws
//! synchronously).

use rquickjs::{function::MutFn, promise::Promised, ArrayBuffer, Ctx, Function, IntoJs, Object, TypedArray, Value};

use crate::plugins::marshal::with_pending;

/// A digest as an `ArrayBuffer` (what the standard resolves to).
struct DigestBytes(Vec<u8>);

impl<'js> IntoJs<'js> for DigestBytes {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    ArrayBuffer::new(ctx.clone(), self.0).map(|ab| ab.into_value())
  }
}

/// The algorithm name from a `"SHA-256"` string or an `{ name }` object.
fn algorithm_name<'js>(val: &Value<'js>) -> Option<String> {
  if let Some(s) = val.as_string() {
    return s.to_string().ok();
  }
  val.as_object()?.get::<_, String>("name").ok()
}

/// The bytes of a `Uint8Array` or `ArrayBuffer` input; anything else is None.
fn input_bytes<'js>(val: &Value<'js>) -> Option<Vec<u8>> {
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Some(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  ArrayBuffer::from_value(val.clone()).map(|ab| ab.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
}

fn digest(algorithm: &Value<'_>, data: &Value<'_>) -> Result<DigestBytes, String> {
  let name = algorithm_name(algorithm).ok_or("crypto.subtle.digest: algorithm must be a name or { name }")?;
  let bytes = input_bytes(data).ok_or("crypto.subtle.digest: data must be a Uint8Array or ArrayBuffer")?;
  forge::crypto::digest(&name, &bytes).map(DigestBytes).map_err(|e| format!("crypto.subtle.digest: {e}"))
}

/// Install the `crypto` global with its `subtle.digest`.
pub fn init_crypto(ctx: &Ctx<'_>) {
  let digest_fn = Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>, algorithm: Value<'_>, data: Value<'_>| -> rquickjs::Result<Promised<_>> {
      let result = digest(&algorithm, &data);
      Ok(with_pending(&ctx, async move { result }))
    }),
  )
  .expect("failed to create crypto.subtle.digest");

  let subtle = Object::new(ctx.clone()).expect("failed to create crypto.subtle");
  subtle.set("digest", digest_fn).expect("failed to set crypto.subtle.digest");
  let crypto = Object::new(ctx.clone()).expect("failed to create crypto");
  crypto.set("subtle", subtle).expect("failed to set crypto.subtle");
  ctx.globals().set("crypto", crypto).expect("failed to set crypto global");
}
