// The file dialog session (`alloy::Dialogs`, reached through
// `flux::gui::dialogs`) as the control API and the test verb spell it: an
// answer parsed from JSON, a request and the session's state encoded to
// JSON. The words are the web pickers' and the harness's (`DialogRequest`
// and `DialogAnswer` in `@solidrt/test`).

use alloy::{DialogAnswer, DialogKind, DialogRequest, DialogStatus, Picked};

/// Parse an answer as the harness and the control API spell it: `{ files:
/// [path, ...] }` (a string, or `{ path, name }` to name it), `{ directory:
/// path }`, or `{ cancel: true }`. A name not given is the path's file name.
pub fn parse_answer(json: &serde_json::Value) -> Result<Vec<Picked>, String> {
  const SHAPE: &str = "answer must be { files: [path, ...] }, { directory: path } or { cancel: true }";
  let Some(obj) = json.as_object() else {
    return Err(SHAPE.to_string());
  };
  if obj.get("cancel").and_then(|c| c.as_bool()) == Some(true) {
    return Ok(Vec::new());
  }
  if let Some(directory) = obj.get("directory") {
    let location = directory.as_str().ok_or_else(|| SHAPE.to_string())?.to_string();
    let name = name_of(&location);
    return Ok(vec![Picked { location, name }]);
  }
  let Some(files) = obj.get("files").and_then(|f| f.as_array()) else {
    return Err(SHAPE.to_string());
  };
  files
    .iter()
    .map(|f| {
      if let Some(path) = f.as_str() {
        return Ok(Picked { location: path.to_string(), name: name_of(path) });
      }
      let path = f.get("path").and_then(|p| p.as_str()).ok_or_else(|| SHAPE.to_string())?.to_string();
      let name = f.get("name").and_then(|n| n.as_str()).map(str::to_string).unwrap_or_else(|| name_of(&path));
      Ok(Picked { location: path, name })
    })
    .collect()
}

fn name_of(path: &str) -> String {
  std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// A request as `{ kind, ...options }`: which function, with the options
/// it was given in the web pickers' words (`multiple`, `suggestedName`,
/// `types` as `[{ description, extensions }]`, `startIn`).
pub fn request_json(request: &DialogRequest) -> serde_json::Value {
  let types: Vec<serde_json::Value> =
    request.filters.iter().map(|f| serde_json::json!({ "description": f.name, "extensions": f.extensions })).collect();
  match &request.kind {
    DialogKind::OpenFile { multiple } => {
      serde_json::json!({ "kind": "openFiles", "multiple": multiple, "types": types, "startIn": request.start_in })
    }
    DialogKind::SaveFile { suggested_name } => serde_json::json!({
      "kind": "saveFile", "suggestedName": suggested_name, "types": types, "startIn": request.start_in
    }),
    DialogKind::OpenDirectory => serde_json::json!({ "kind": "openDirectory", "startIn": request.start_in }),
  }
}

// An answer: `{ cancelled: true }`, `{ picked: [{ location, name }] }` or
// `{ failed: why }`.
fn answer_json(answer: &DialogAnswer) -> serde_json::Value {
  match answer {
    Ok(picked) if picked.is_empty() => serde_json::json!({ "cancelled": true }),
    Ok(picked) => serde_json::json!({
      "picked": picked.iter().map(|p| serde_json::json!({ "location": p.location, "name": p.name })).collect::<Vec<_>>()
    }),
    Err(e) => serde_json::json!({ "failed": e.to_string() }),
  }
}

/// What the control API reports: the request waiting for an answer (null
/// when none), the last request made with its `answer` once it has one,
/// how many answers are queued, and whether this platform can show a
/// dialog.
pub fn status_json(status: &DialogStatus) -> serde_json::Value {
  let last = status.last.as_ref().map(|(request, answer)| {
    let mut json = request_json(request);
    if let (Some(answer), Some(obj)) = (answer, json.as_object_mut()) {
      obj.insert("answer".to_string(), answer_json(answer));
    }
    json
  });
  serde_json::json!({
    "pending": status.pending.as_ref().map(request_json),
    "last": last,
    "queued": status.queued,
    "available": status.available,
  })
}
