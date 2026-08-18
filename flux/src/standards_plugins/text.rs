use crate::plugins::marshal::OptArg;
use rquickjs::class::Trace;
use rquickjs::{ArrayBuffer, Class, Ctx, Exception, JsLifetime, Object, TypedArray, Value};
use std::cell::RefCell;

/// `TextEncoder`: encodes a JS string to UTF-8 bytes. Per the WHATWG spec the
/// encoding is always UTF-8, so there are no options.
#[derive(JsLifetime)]
#[rquickjs::class(rename = "TextEncoder")]
pub struct TextEncoder {}

impl<'js> Trace<'js> for TextEncoder {
  fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

#[rquickjs::methods]
impl TextEncoder {
  #[qjs(constructor)]
  pub fn new() -> Self {
    TextEncoder {}
  }

  #[qjs(get)]
  pub fn encoding(&self) -> String {
    "utf-8".to_string()
  }

  /// Encode `input` (default "") to a `Uint8Array` of its UTF-8 bytes. A Rust
  /// `String` is already valid UTF-8, so this is a direct byte copy.
  pub fn encode<'js>(&self, ctx: Ctx<'js>, input: OptArg<String>) -> rquickjs::Result<TypedArray<'js, u8>> {
    TypedArray::new(ctx, input.0.unwrap_or_default().into_bytes())
  }
}

/// Streaming decoder state, carried across `decode(..., { stream: true })` calls.
#[derive(Default)]
struct DecoderState {
  /// Trailing bytes of an incomplete UTF-8 sequence held until the next chunk.
  pending: Vec<u8>,
  /// Whether the leading-BOM decision has been made yet (BOM is only stripped
  /// once, at the very start of the stream).
  bom_handled: bool,
}

/// `TextDecoder`: decodes UTF-8 bytes to a JS string, with incremental
/// (`{ stream: true }`) support that holds a UTF-8 sequence split across chunk
/// boundaries until it completes. UTF-8 only (the only encoding this runtime
/// needs); a non-UTF-8 label throws, matching the spec's RangeError.
#[derive(JsLifetime)]
#[rquickjs::class(rename = "TextDecoder")]
pub struct TextDecoder {
  #[qjs(skip_trace)]
  fatal: bool,
  #[qjs(skip_trace)]
  ignore_bom: bool,
  #[qjs(skip_trace)]
  state: RefCell<DecoderState>,
}

impl<'js> Trace<'js> for TextDecoder {
  fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

#[rquickjs::methods]
impl TextDecoder {
  #[qjs(constructor)]
  pub fn new<'js>(ctx: Ctx<'js>, label: OptArg<String>, options: OptArg<Object<'js>>) -> rquickjs::Result<Self> {
    let label = label.0.unwrap_or_default();
    let normalized = label.trim().to_ascii_lowercase();
    // The set of UTF-8 labels the WHATWG Encoding Standard maps to "utf-8".
    let utf8 = matches!(
      normalized.as_str(),
      "" | "utf-8" | "utf8" | "unicode-1-1-utf-8" | "unicode11utf8" | "unicode20utf8" | "x-unicode20utf8"
    );
    if !utf8 {
      return Err(Exception::throw_range(&ctx, &format!("TextDecoder: unsupported encoding '{label}' (utf-8 only)")));
    }
    let fatal = options.0.as_ref().and_then(|o| o.get::<_, Option<bool>>("fatal").ok().flatten()).unwrap_or(false);
    let ignore_bom =
      options.0.as_ref().and_then(|o| o.get::<_, Option<bool>>("ignoreBOM").ok().flatten()).unwrap_or(false);
    Ok(TextDecoder { fatal, ignore_bom, state: RefCell::new(DecoderState::default()) })
  }

  #[qjs(get)]
  pub fn encoding(&self) -> String {
    "utf-8".to_string()
  }

  #[qjs(get)]
  pub fn fatal(&self) -> bool {
    self.fatal
  }

  #[qjs(get, rename = "ignoreBOM")]
  pub fn ignore_bom(&self) -> bool {
    self.ignore_bom
  }

  /// Decode `input` (a Uint8Array or ArrayBuffer; default empty) to a string. With
  /// `{ stream: true }` an incomplete trailing UTF-8 sequence is held for the next
  /// call; otherwise the decoder flushes and resets (a held tail decodes to one
  /// replacement char, or throws when `fatal`).
  pub fn decode<'js>(
    &self,
    ctx: Ctx<'js>,
    input: OptArg<Value<'js>>,
    options: OptArg<Object<'js>>,
  ) -> rquickjs::Result<String> {
    let stream = options.0.as_ref().and_then(|o| o.get::<_, Option<bool>>("stream").ok().flatten()).unwrap_or(false);
    let bytes = match input.0 {
      Some(v) => input_bytes(&ctx, &v)?,
      None => Vec::new(),
    };

    let mut state = self.state.borrow_mut();
    let mut buf = std::mem::take(&mut state.pending);
    buf.extend_from_slice(&bytes);

    let (mut out, rest) = decode_utf8(&ctx, &buf, stream, self.fatal)?;

    // Strip a leading BOM once at the start of the stream (unless ignoreBOM).
    // Done on decoded output so a BOM split across chunks is still handled.
    if !self.ignore_bom && !state.bom_handled && !out.is_empty() {
      state.bom_handled = true;
      if let Some(stripped) = out.strip_prefix('\u{FEFF}') {
        out = stripped.to_string();
      }
    }

    if stream {
      state.pending = rest;
    } else {
      // A non-streaming decode flushes and resets the decoder for reuse.
      *state = DecoderState::default();
    }
    Ok(out)
  }
}

/// Extract raw bytes from a decode input: a Uint8Array or an ArrayBuffer.
/// null/undefined yield no bytes. Other views (e.g. Uint16Array) are unsupported.
fn input_bytes<'js>(ctx: &Ctx<'js>, val: &Value<'js>) -> rquickjs::Result<Vec<u8>> {
  if val.is_null() || val.is_undefined() {
    return Ok(Vec::new());
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  if let Some(ab) = ArrayBuffer::from_value(val.clone()) {
    return Ok(ab.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  Err(Exception::throw_type(ctx, "TextDecoder.decode: input must be a Uint8Array or ArrayBuffer"))
}

/// Decode `buf` as UTF-8. Returns the decoded string plus any trailing bytes that
/// form an incomplete sequence (only when `stream`, so they can be carried over).
/// Non-`stream`: an incomplete tail becomes one replacement char. Invalid bytes
/// mid-buffer become a replacement char unless `fatal`, which throws a TypeError.
fn decode_utf8<'js>(ctx: &Ctx<'js>, buf: &[u8], stream: bool, fatal: bool) -> rquickjs::Result<(String, Vec<u8>)> {
  let mut out = String::new();
  let mut i = 0;
  loop {
    match std::str::from_utf8(&buf[i..]) {
      Ok(valid) => {
        out.push_str(valid);
        return Ok((out, Vec::new()));
      }
      Err(e) => {
        let valid_up_to = e.valid_up_to();
        out.push_str(std::str::from_utf8(&buf[i..i + valid_up_to]).expect("prefix is valid utf-8"));
        i += valid_up_to;
        match e.error_len() {
          // No error length: the tail is an incomplete sequence cut off at the end.
          None => {
            if stream {
              return Ok((out, buf[i..].to_vec()));
            }
            if fatal {
              return Err(decode_error(ctx));
            }
            out.push('\u{FFFD}');
            return Ok((out, Vec::new()));
          }
          // Genuinely invalid bytes mid-buffer: one replacement, then continue.
          Some(n) => {
            if fatal {
              return Err(decode_error(ctx));
            }
            out.push('\u{FFFD}');
            i += n;
          }
        }
      }
    }
  }
}

fn decode_error(ctx: &Ctx<'_>) -> rquickjs::Error {
  Exception::throw_type(ctx, "TextDecoder.decode: invalid UTF-8 (fatal)")
}

pub(crate) fn init_text(ctx: &Ctx<'_>) {
  Class::<TextEncoder>::define(&ctx.globals()).expect("define TextEncoder class");
  Class::<TextDecoder>::define(&ctx.globals()).expect("define TextDecoder class");
}
