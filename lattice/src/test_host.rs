// Test mode (okf/done/test-harness.md, stage 4): the dev client as a test
// host. Headless on alloy's stepped mode (stepped.rs): a test asks for every
// frame (`sol:test` `frame`), so app time is frame / fps and nothing else,
// and a frame nobody demanded draws nothing. Every test runs in an engine of
// its own, built by the engine loop like any reload: the file is evaluated
// once for the listing and once more per test (flux::test), and a failed
// test's record carries what the app looked like (`failure_details`).

use std::path::{Path, PathBuf};

use flux::rquickjs::Ctx;

use crate::stepped::Stepper;

/// How many lines of the outline a failure prints before the rest is
/// counted: a screen's worth, where a whole app would bury the error above
/// it. The test reads any subtree in full with `locator.outline()`.
const FAILURE_OUTLINE_LINES: usize = 200;
/// How long a snapshot's file name may be before the test name is cut.
const SNAPSHOT_NAME_MAX: usize = 60;

/// What a test run is started with (see `start_tests`).
pub struct TestRun {
  pub options: flux::test::RunOptions,
  /// Where a failed test's snapshot is written (`<test name>.png`); None
  /// leaves no snapshot.
  pub failures: Option<PathBuf>,
}

/// Empty the app's sandbox, ahead of every test engine: its data folder
/// (the working directory, which stays) and its fetch cache. A test then
/// starts from no stored state, whatever the tests before it wrote; the
/// file is evaluated after this, so what it opens at module level is opened
/// in the emptied sandbox.
pub(crate) fn empty_sandbox(store: &crate::storage::Storage, app_id: &str) {
  for dir in [store.app_dir(app_id).join("data"), store.cache_dir(app_id)] {
    let Ok(entries) = std::fs::read_dir(&dir) else { continue };
    for entry in entries.flatten() {
      let path = entry.path();
      let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
      if let Err(e) = removed {
        log::warn!("[sol] test mode: could not remove {} from the sandbox: {e}", path.display());
      }
    }
  }
}

/// The details a failed app test's record carries beside the host's own
/// (okf/done/test-harness.md, D14), read in the test's engine when the
/// failure is known: when it was (app time and frame), what still wanted
/// frames, the tree as the test saw it, and a snapshot of the frame,
/// written under `failures` and named in the record. A test with no window
/// gets the time alone.
pub(crate) fn failure_details(ctx: &Ctx<'_>, name: &str, failures: Option<&Path>) -> flux::test::Details {
  let mut details = Vec::new();
  if let Some(stepper) = ctx.userdata::<Stepper>() {
    details.push((
      "At".to_string(),
      format!("{} ms of app time, frame {}", (stepper.time_ms() * 100.0).round() / 100.0, stepper.frame()),
    ));
  }
  let demand = flux::gui::frame::demand(ctx);
  if !demand.is_empty() {
    details.push(("Frames demanded by".to_string(), demand.join(", ")));
  }
  let Some(tree) = flux::gui::tree::with_tree(ctx, |tree| {
    tree.snapshot_from(None, None).map(|root| flux::gui::inspect::outline(&root, Some(tree), false))
  })
  .flatten() else {
    return details;
  };
  details.push(("Outline".to_string(), cut_lines(&tree, FAILURE_OUTLINE_LINES)));
  if let Some(dir) = failures {
    match snapshot(ctx, &dir.join(format!("{}.png", snapshot_name(name)))) {
      Ok(path) => details.push(("Snapshot".to_string(), path.display().to_string())),
      Err(e) => details.push(("Snapshot".to_string(), format!("not written: {e}"))),
    }
  }
  details
}

/// The first `max` lines of `text`, the rest counted.
fn cut_lines(text: &str, max: usize) -> String {
  let total = text.lines().count();
  if total <= max {
    return text.to_string();
  }
  let kept: Vec<&str> = text.lines().take(max).collect();
  format!("{}\n... {} more lines (locator.outline() reads a subtree in full)", kept.join("\n"), total - max)
}

/// A test's name as a file name: letters and digits kept, everything else
/// one dash, cut to `SNAPSHOT_NAME_MAX`.
fn snapshot_name(name: &str) -> String {
  let mut out = String::new();
  for c in name.chars() {
    if c.is_ascii_alphanumeric() {
      out.push(c.to_ascii_lowercase());
    } else if !out.ends_with('-') {
      out.push('-');
    }
    if out.len() >= SNAPSHOT_NAME_MAX {
      break;
    }
  }
  let trimmed = out.trim_matches('-');
  if trimmed.is_empty() {
    "test".to_string()
  } else {
    trimmed.to_string()
  }
}

/// Paint the tree as it stands (no frame runs, no app time passes, the way
/// `locator.pixels()` reads) and write the window to `path`.
fn snapshot(ctx: &Ctx<'_>, path: &Path) -> Result<PathBuf, String> {
  let alloy = flux::gui::alloy_context(ctx).ok_or_else(|| "no window".to_string())?;
  flux::gui::request_frame(ctx);
  crate::plugins::draw::render_now(ctx);
  let (width, height, pixels) = alloy.read_window()?;
  let png = forge::image::encode_png(&pixels, width, height, false)?;
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
  }
  std::fs::write(path, png).map_err(|e| format!("could not write {}: {e}", path.display()))?;
  Ok(path.to_path_buf())
}
