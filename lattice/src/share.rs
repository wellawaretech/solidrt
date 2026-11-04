// The share session (`ore::share::Session`, reached through
// `flux::share::session`) as the control API and the test verb spell it:
// the requests the app made, encoded to JSON. The words are the web's
// (`ShareData`), files as the `{ path, name }` a `flux:fs` handle carries.

use ore::share::ShareRequest;

/// One request: `{ title, text, url, files: [{ path, name }] }`, an unset
/// field null.
pub fn request_json(request: &ShareRequest) -> serde_json::Value {
  let files: Vec<serde_json::Value> =
    request.files.iter().map(|f| serde_json::json!({"path": f.location, "name": f.name})).collect();
  serde_json::json!({"title": request.title, "text": request.text, "url": request.url, "files": files})
}

/// The shares the app asked for in this engine, oldest first; empty
/// without a share session.
pub fn requested_json(ctx: &flux::rquickjs::Ctx<'_>) -> serde_json::Value {
  let requested = flux::share::session(ctx).map(|session| session.requested()).unwrap_or_default();
  serde_json::Value::Array(requested.iter().map(request_json).collect())
}
