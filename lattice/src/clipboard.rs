// The clipboard session (`ore::clipboard::Session`, reached through
// `flux::clipboard::session`) as the control API spells it, encoded to
// JSON: what the app last wrote to the clipboard (in a test engine, what
// the engine's own clipboard holds), never the OS clipboard's content.
// The words are the web's: a text as `text`, an image as its MIME `type`
// and its bytes, as base64.

use base64::Engine as _;
use ore::clipboard::Content;

/// One content: null for none, `{ text }`, or `{ type, byteLength, base64 }`.
pub fn content_json(content: Option<&Content>) -> serde_json::Value {
  match content {
    None => serde_json::Value::Null,
    Some(Content::Text(text)) => serde_json::json!({"text": text}),
    Some(Content::Image(image)) => serde_json::json!({
      "type": image.format.mime(),
      "byteLength": image.bytes.len(),
      "base64": base64::engine::general_purpose::STANDARD.encode(&image.bytes),
    }),
  }
}

/// What the engine's clipboard session holds; null without a session.
pub fn held_json(ctx: &flux::rquickjs::Ctx<'_>) -> serde_json::Value {
  content_json(flux::clipboard::session(ctx).and_then(|session| session.held()).as_ref())
}
