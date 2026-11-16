// A script at the edge of its memory limit: every JS-heap allocation a
// plugin makes on a script-reachable path may fail there, and must come
// back to the script as a throw, never as a panic (rquickjs turns a
// callback's panic into a JS exception and resumes it once that exception
// reaches Rust; in the app runtime a panic ends the process). A walk fills
// the heap until an allocation throws, then frees a few bytes at a time and
// calls the API under test, so the allocation that fails inside the API
// moves step by step from its first allocation site to its last. A module's
// exports are built once, at its first import, so the import walk takes an
// engine per step. No sleeps and no timing: the heap's own accounting
// decides where each call fails.

mod common;

use common::{LogSink, TempDir};
use flux::FluxEngine;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Once};

// The heap limit of a walk: well above what the engine's own startup
// allocates (a few hundred KiB), so the startup and the modules under test
// fit, and small enough that the fill reaches the edge quickly.
const MEMORY_LIMIT: usize = 4 * 1024 * 1024;

// The chunk sizes of the coarse fill, in characters of padding: largest
// first to reach the edge fast, the last a bare list node.
const FILL_PADDING: [usize; 5] = [65536, 4096, 256, 16, 0];

// The fine pool a walk frees first: a resizable ArrayBuffer, taken before
// the fill and grown at its end until the heap is full, then shrunk by
// FINE_STEP per walk step. More than the largest call walked needs; the
// list nodes follow after it.
const FINE_POOL: usize = 64 * 1024;

// The most the fill grows the fine pool by at its end: more than the room
// a bare list node leaves.
const FINE_TAIL: usize = 4 * 1024;

// The bytes a walk step frees: the heap's own allocation granularity (the
// rust-alloc allocator rounds to 8), smaller than any object a plugin
// allocates, so a step moves the failing allocation by at most one site.
const FINE_STEP: usize = 8;

// The most steps a call walk takes, a bound for a call that never
// succeeds: the whole fine pool, and as many list nodes after it.
const WALK_STEPS_MAX: usize = 2 * FINE_POOL / FINE_STEP;

// The fine steps an import walk frees per engine. A module loads once per
// engine, so each import step is an engine of its own; a coarser stride
// than FINE_STEP keeps a walk that ends at the first import to a few
// hundred engines.
const IMPORT_STRIDE: usize = 4;

// The most fine steps an import walk frees, a bound for an import that
// never succeeds: the whole fine pool.
const IMPORT_STEPS_MAX: usize = FINE_POOL / FINE_STEP;

// Panics from any thread since the hook was installed. rquickjs catches a
// panic in a callback, stores it and throws a JS exception in its place;
// the walk's own try/catch swallows that exception, so the stored panic
// never reaches Rust and cannot fail the test. This count, taken by a panic
// hook, is what fails it. The walks run one at a time (WALKS), so a walk's
// count before and after is its own.
static PANICS: AtomicUsize = AtomicUsize::new(0);
static HOOK: Once = Once::new();
static WALKS: Mutex<()> = Mutex::new(());

fn count_panics() {
  HOOK.call_once(|| {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
      PANICS.fetch_add(1, Ordering::SeqCst);
      previous(info);
    }));
  });
}

/// JS that fills the heap up to the limit: the fine pool in `fine`, a list
/// of chunks in `head`, then the pool grown into what is left. `release()`
/// frees one step, the fine pool first, and is false once nothing is left
/// to free; it is built before the fill, as nothing after the fill may
/// allocate outside a try/catch.
fn fill() -> String {
  format!(
    r#"
let fine = new ArrayBuffer({FINE_POOL}, {{ maxByteLength: {FINE_POOL} + {FINE_TAIL} }});
let head = null;
let release = () => {{
  if (fine.byteLength >= {FINE_STEP}) fine.resize(fine.byteLength - {FINE_STEP});
  else if (head !== null) head = head.next;
  else return false;
  return true;
}};
for (let padding of {FILL_PADDING:?}) {{
  try {{
    for (;;) head = {{ next: head, pad: padding > 0 ? "x".repeat(padding) : null }};
  }} catch {{}}
}}
try {{
  for (;;) fine.resize(fine.byteLength + {FINE_STEP});
}} catch {{}}
"#
  )
}

/// Run `code` under the walk's memory limit and return the log, asserting
/// that nothing panicked meanwhile.
async fn run_limited(code: &str) -> String {
  count_panics();
  let _walk = WALKS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let before = PANICS.load(Ordering::SeqCst);
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).memory_limit(MEMORY_LIMIT).build();
  engine.eval_source(code).await;
  assert_eq!(PANICS.load(Ordering::SeqCst) - before, 0, "a plugin panicked under memory pressure");
  sink.captured().log()
}

// The calls walked, each in an engine of its own: JS statements that
// throw when the call fails. A consumed body throws a string ("Body already
// consumed") when the call succeeds. A step neither makes a closure nor
// calls a method on a built-in: QuickJS builds a built-in's methods at
// their first use, and a first use that fails at the edge leaves the method
// undefined for the rest of the engine, which would end the walk early.
// `spawn()` is not walked: it starts its process before its first JS
// allocation, so every failing step would start a real process.
const CALLS: &[&str] = &[
  "file(path);",
  "dir(folder);",
  "command(\"true\");",
  "try { response.text(); } catch (e) { if (typeof e !== \"string\") throw e; }",
  "try { request.json(); } catch (e) { if (typeof e !== \"string\") throw e; }",
];

#[tokio::test]
async fn plugin_calls_at_the_memory_limit_throw_instead_of_panicking() {
  let dir = TempDir::new();
  let path = dir.join("data.txt");
  std::fs::write(&path, "data").expect("write the walk's file");
  for call in CALLS {
    // The modules are imported and the bodies consumed before the fill:
    // this walk is over the call, the next test walks the imports. The
    // walk ends at the first step the call succeeds.
    let code = format!(
      r#"
import {{ file, dir }} from "flux:fs";
import {{ command }} from "flux:subprocess";
let path = {path:?};
let folder = {folder:?};
let response = new Response("x");
await response.text();
let request = new Request("http://localhost/", {{ method: "POST", body: "x" }});
await request.text();
let call = () => {{
  {call}
}};
{fill}
let steps = 0;
let failures = 0;
let succeeded = false;
for (; steps < {WALK_STEPS_MAX} && release(); steps++) {{
  try {{
    call();
    succeeded = true;
    break;
  }} catch {{
    failures++;
  }}
}}
head = null;
fine = null;
console.log(`walked ${{steps}} steps, ${{failures}} failures, succeeded ${{succeeded}}`);
"#,
      folder = dir.path(),
      fill = fill(),
    );
    let log = run_limited(&code).await;
    let walked =
      log.lines().find(|line| line.starts_with("walked ")).unwrap_or_else(|| panic!("{call}: no walk: {log}"));
    assert!(!walked.contains(" 0 failures"), "{call}: the walk met no allocation failure: {walked}");
    assert!(walked.ends_with("succeeded true"), "{call}: the walk never reached a call that succeeds: {walked}");
  }
}

#[tokio::test]
async fn module_imports_at_the_memory_limit_throw_instead_of_panicking() {
  // A native module's exports are built at its first import, under the
  // limit. A module loads once per engine, and an `await` at the edge can
  // fail in QuickJS itself, outside any try/catch, so each step is an
  // engine of its own: fill, free the step's bytes, and import, the import
  // running after the entry with the heap still full. The entry's first
  // await suspends it before the fill, so the engine attaches its handler
  // to the entry's promise while there is room. The walk ends at the first
  // step whose import succeeds.
  for module in ["flux:fs", "flux:subprocess"] {
    let mut failures = 0;
    let mut imported = false;
    for step in (0..IMPORT_STEPS_MAX).step_by(IMPORT_STRIDE) {
      let code = format!(
        r#"
await null;
{fill}
for (let i = 0; i < {step} && release(); i++);
try {{
  import("{module}").then(() => {{ head = null; fine = null; console.log("imported"); }}, () => {{}});
}} catch {{}}
"#,
        fill = fill(),
      );
      if run_limited(&code).await.lines().any(|line| line == "imported") {
        imported = true;
        break;
      }
      failures += 1;
    }
    assert!(imported, "{module} never imported");
    assert!(failures > 0, "the import of {module} met no allocation failure");
  }
}
