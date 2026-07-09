use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rquickjs::{Ctx, Exception, Function};

/// `btoa(data)`: base64-encode a "binary string" whose char codes are the raw
/// bytes (each in 0..=255). Per the WHATWG spec a code point above 255 throws an
/// `InvalidCharacterError` - the string is a byte container, not Unicode text, so
/// there is no UTF-8 step here.
fn btoa(ctx: Ctx<'_>, data: String) -> rquickjs::Result<String> {
  let mut bytes = Vec::with_capacity(data.len());
  for ch in data.chars() {
    let code = ch as u32;
    if code > 0xFF {
      return Err(Exception::throw_message(&ctx, "btoa: string contains a character outside of the Latin1 range"));
    }
    bytes.push(code as u8);
  }
  Ok(STANDARD.encode(bytes))
}

/// `atob(data)`: base64-decode to a "binary string" whose char codes are the
/// decoded bytes (each in 0..=255). The bytes are widened to code points one-to-
/// one (not interpreted as UTF-8), so a consumer reading them back with
/// `charCodeAt` recovers the exact bytes.
fn atob(ctx: Ctx<'_>, data: String) -> rquickjs::Result<String> {
  // Whitespace is permitted in the input and ignored, per the spec.
  let stripped: String = data.chars().filter(|c| !c.is_ascii_whitespace()).collect();
  let bytes = STANDARD
    .decode(stripped.as_bytes())
    .map_err(|_| Exception::throw_message(&ctx, "atob: string is not correctly base64-encoded"))?;
  // Widen each byte to a char (U+0000..U+00FF), yielding code points the caller
  // can read back as bytes.
  Ok(bytes.into_iter().map(|b| b as char).collect())
}

pub(crate) fn init_base64(ctx: &Ctx<'_>) {
  let globals = ctx.globals();
  globals.set("btoa", Function::new(ctx.clone(), btoa).expect("create btoa")).expect("set btoa global");
  globals.set("atob", Function::new(ctx.clone(), atob).expect("create atob")).expect("set atob global");
}
