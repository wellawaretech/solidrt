// The test host: what runs a test file (okf/plans/test-harness.md). Every
// test gets an engine of its own. The file is evaluated once to list the
// tests it registers, then once more for each test, in an engine that runs
// that test and nothing else. A test therefore starts from the file's
// freshly evaluated module state, its `Math.random` sequence from the start
// and no timer, listener or rejection of another test's: its result does
// not depend on the tests before it or on a filter, an uncaught error is
// the error of the test whose engine raised it, and what a test left
// running ends with its engine.
//
// A `Session` is one engine's part: `install` adds what a test engine needs
// to a builder, and `FileRun::drive` evaluates the file in the engine built
// from it and waits for the listing or the result. A `FileRun` is the file's
// part: which session comes next, and the records. `run_file` is the whole
// of it for a host whose engines come from one builder function (the `flux`
// binary); a host that builds its engines inside a loop of its own (the dev
// client) asks the `FileRun` for each session and drives it there.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::function::{MutFn, This};
use rquickjs::{Ctx, Function, JsLifetime, Value};

use crate::engine::{ExecHandle, FluxEngine, FluxEngineBuilder, ModuleCode};
use crate::logger::CtxLogger;

use super::Registry;

/// Marks a stdout line as a record of the host's, not something a test or
/// a native library printed. It starts with a control character (the ASCII
/// record separator) so that nothing prints it by accident.
pub const RECORD_PREFIX: &str = "\u{1e}srt-test ";
/// How long one engine may take, from the start of the file's evaluation to
/// the test's end, when the host is given no other cap. A safety cap
/// against a test that never finishes, not a wait: a test that finishes is
/// never held to it.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(5000);
/// The seed `Math.random` runs on when the host is given none. Any fixed
/// number does: what matters is that it is the same on every run.
pub const DEFAULT_SEED: u64 = 0;
/// The module name the file is evaluated under, which is what its stack
/// frames cite and so what a sourcemap for it is keyed by.
pub const ENTRY_MODULE: &str = "main";

/// Context userdata of an engine a test host built. `flux:test` evaluates
/// only where it is present (see `TestModule::evaluate`).
#[derive(Clone, JsLifetime)]
pub(crate) struct Hosted;

#[derive(Clone)]
pub struct RunOptions {
  /// Run only the tests whose name contains this text.
  pub filter: Option<String>,
  /// The seed every engine's `Math.random` starts from.
  pub seed: u64,
  /// The cap on one engine (see `DEFAULT_TIMEOUT`).
  pub timeout: Duration,
}

impl Default for RunOptions {
  fn default() -> Self {
    Self { filter: None, seed: DEFAULT_SEED, timeout: DEFAULT_TIMEOUT }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestError {
  /// The thrown error's name and message, or the host's own sentence for a
  /// test that timed out, never finished or left an uncaught error.
  pub message: String,
  /// The thrown error's stack; empty when the host wrote the message.
  pub stack: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestResult {
  pub name: String,
  pub ok: bool,
  /// Real time from the test's start to its end, without the evaluation of
  /// its file; 0 for a test that never started.
  pub duration_ms: f64,
  pub error: Option<TestError>,
  /// What the engine logged while the test ran, the file's own output at
  /// load left out (it is the same in every engine and reported once).
  pub output: Vec<String>,
}

/// What a host reports about a file, in order: `Loaded` or `Failed` first,
/// after `Loaded` one `Result` per test that ran, then `Done`.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
  /// The file evaluated. `tests` are the ones that will run (the filter
  /// applied), `output` what the file logged while it loaded.
  Loaded {
    tests: Vec<String>,
    output: Vec<String>,
  },
  /// The file did not evaluate, so none of its tests ran.
  Failed {
    message: String,
    output: Vec<String>,
  },
  Result(TestResult),
  Done,
}

impl Record {
  /// The record as one stdout line: the prefix and a JSON object.
  pub fn line(&self) -> String {
    let json = match self {
      Record::Loaded { tests, output } => serde_json::json!({ "type": "loaded", "tests": tests, "output": output }),
      Record::Failed { message, output } => {
        serde_json::json!({ "type": "failed", "message": message, "output": output })
      }
      Record::Result(result) => {
        let mut record = serde_json::json!({
          "name": result.name,
          "ok": result.ok,
          "durationMs": result.duration_ms,
          "output": result.output,
        });
        if let Some(error) = &result.error {
          record["error"] = serde_json::json!({ "message": error.message, "stack": error.stack });
        }
        serde_json::json!({ "type": "result", "result": record })
      }
      Record::Done => serde_json::json!({ "type": "done" }),
    };
    format!("{RECORD_PREFIX}{json}")
  }
}

/// What one engine is built for.
enum Task {
  List,
  Run(String),
}

/// What a listing engine came to.
enum Listed {
  Tests { names: Vec<String>, output: Vec<String> },
  Failed { message: String, output: Vec<String> },
}

/// What the engine reports to the host waiting on it.
enum Signal {
  Names(Vec<String>),
  Finished,
}

/// How the host's wait on an engine ended.
enum End {
  Signal(Signal),
  /// The cap passed.
  TimedOut,
  /// The engine ran out of work with nothing reported.
  Idle,
}

/// What the engine's side of a session writes and the host's side reads.
#[derive(Default)]
struct Shared {
  output: Mutex<Vec<String>>,
  uncaught: Mutex<Vec<String>>,
  /// How many output lines the file's evaluation logged: set when the file
  /// has evaluated, which is also the mark that it did.
  loaded_at: Mutex<Option<usize>>,
  started: Mutex<Option<Instant>>,
  /// The test's end: its duration and its error, None for a pass.
  ended: Mutex<Option<(f64, Option<TestError>)>>,
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  mutex.lock().expect("test session lock poisoned")
}

/// Sets the engine's interrupt flag once the cap has passed, from a thread
/// of its own: a test stuck in a synchronous loop never yields to the
/// host's own timer, and the interrupt is what unwinds it. Dropping the
/// watchdog disarms it.
struct Watchdog {
  _disarm: mpsc::Sender<()>,
}

impl Watchdog {
  fn arm(flag: Arc<AtomicBool>, after: Duration) -> Self {
    let (disarm, armed) = mpsc::channel::<()>();
    std::thread::spawn(move || {
      if let Err(mpsc::RecvTimeoutError::Timeout) = armed.recv_timeout(after) {
        flag.store(true, Ordering::Relaxed);
      }
    });
    Self { _disarm: disarm }
  }
}

/// One engine of a file's run: the listing engine or one test's.
pub struct Session {
  task: Task,
  seed: u64,
  timeout: Duration,
  shared: Arc<Shared>,
  interrupt: Arc<AtomicBool>,
}

impl Session {
  fn list(options: &RunOptions) -> Self {
    Self::new(Task::List, options)
  }

  fn run(name: &str, options: &RunOptions) -> Self {
    Self::new(Task::Run(name.to_string()), options)
  }

  fn new(task: Task, options: &RunOptions) -> Self {
    Self {
      task,
      seed: options.seed,
      timeout: options.timeout,
      shared: Arc::new(Shared::default()),
      interrupt: Arc::new(AtomicBool::new(false)),
    }
  }

  /// Add what a test engine needs to `builder`: the log sink the output is
  /// collected through, the uncaught-error hook, the interrupt flag of the
  /// cap, the seed of `Math.random` and the mark `flux:test` asks for. It
  /// replaces the builder's own logger and uncaught hook.
  pub fn install(&self, builder: FluxEngineBuilder) -> FluxEngineBuilder {
    let output = self.shared.clone();
    let uncaught = self.shared.clone();
    // Once the cap has interrupted the engine, what it still says is the
    // interruption unwinding, not the test.
    let output_cut = self.interrupt.clone();
    let uncaught_cut = self.interrupt.clone();
    let seed = self.seed;
    builder
      .logger(move |_, msg| {
        if !output_cut.load(Ordering::Relaxed) {
          locked(&output.output).push(msg.to_string())
        }
      })
      .on_uncaught(move |msg| {
        if !uncaught_cut.load(Ordering::Relaxed) {
          locked(&uncaught.uncaught).push(msg.to_string())
        }
      })
      .interrupt_flag(self.interrupt.clone())
      .userdata(Hosted)
      .plugin(move |ctx| {
        if let Err(e) = crate::seed_random(&ctx, seed) {
          ctx.logger().error(&format!("Test host: failed to seed Math.random: {e}"));
        }
      })
  }

  /// Evaluate the file in `engine` (built from a builder `install` went
  /// over) and wait for what the session is for. Returns once the listing
  /// or the result is in, the engine ended by itself or the cap passed; the
  /// engine is dropped then, with everything the file left running.
  async fn drive(&self, engine: FluxEngine, code: ModuleCode) -> End {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Signal>();
    let exec = engine.exec_handle();
    let shared = self.shared.clone();
    let name = match &self.task {
      Task::List => None,
      Task::Run(name) => Some(name.clone()),
    };
    let _watchdog = Watchdog::arm(self.interrupt.clone(), self.timeout);
    let evaluation = engine.eval_module(ENTRY_MODULE.to_string(), code, move |ctx, _namespace| {
      let lines = locked(&shared.output).len();
      *locked(&shared.loaded_at) = Some(lines);
      let registry = ctx.userdata::<Registry>().map(|registry| (*registry).clone());
      match name {
        None => {
          let names = match registry.map(|registry| registry.names(&ctx)) {
            Some(Ok(names)) => names,
            Some(Err(e)) => {
              ctx.logger().error(&format!("Test host: failed to read the registered tests: {e}"));
              Vec::new()
            }
            None => Vec::new(),
          };
          let _ = tx.send(Signal::Names(names));
        }
        Some(name) => start(ctx, registry, &name, shared, exec, tx),
      }
    });
    tokio::pin!(evaluation);
    let end = tokio::select! {
      biased;
      Some(signal) = rx.recv() => End::Signal(signal),
      _ = tokio::time::sleep(self.timeout) => End::TimedOut,
      _ = &mut evaluation => rx.try_recv().map(End::Signal).unwrap_or(End::Idle),
    };
    // An engine the watchdog interrupted can end before the host's own
    // timer fires: it ran out of time all the same.
    match end {
      End::Idle if self.interrupt.load(Ordering::Relaxed) => End::TimedOut,
      end => end,
    }
  }

  /// Drive the listing engine.
  async fn listed(self, engine: FluxEngine, code: ModuleCode) -> Listed {
    let end = self.drive(engine, code).await;
    let output = locked(&self.shared.output).clone();
    match end {
      End::Signal(Signal::Names(names)) => Listed::Tests { names, output },
      end => Listed::Failed { message: self.unfinished("The file", &end), output },
    }
  }

  /// Drive a test's engine.
  async fn result(self, engine: FluxEngine, code: ModuleCode) -> TestResult {
    let Task::Run(name) = &self.task else {
      panic!("Session::result on a listing session");
    };
    let name = name.clone();
    let end = self.drive(engine, code).await;
    let output = locked(&self.shared.output).clone();
    let loaded_at = *locked(&self.shared.loaded_at);
    let uncaught = locked(&self.shared.uncaught).first().cloned();
    let ended = locked(&self.shared.ended).clone();
    let (duration_ms, error) = match ended {
      // The test came to its own end. An uncaught error in its engine
      // fails it all the same; its own error is the one named.
      Some((duration_ms, error)) => {
        (duration_ms, error.or(uncaught.map(|message| TestError { message, stack: String::new() })))
      }
      None => {
        let duration_ms = locked(&self.shared.started).map(|at| at.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
        let subject = if loaded_at.is_some() { "The test" } else { "The file" };
        (duration_ms, Some(TestError { message: self.unfinished(subject, &end), stack: String::new() }))
      }
    };
    // The file's own output at load is the listing engine's to report; a
    // file that did not load this time keeps all of it.
    let mut output = match loaded_at {
      Some(lines) => output[lines..].to_vec(),
      None => output,
    };
    // An uncaught error was logged as well; as the test's error it is not
    // repeated in its output.
    if let Some(error) = &error {
      output.retain(|line| *line != error.message);
    }
    TestResult { name, ok: error.is_none(), duration_ms, error, output }
  }

  /// Why an engine gave no listing or result, for `subject` ("The file"
  /// while it loads, "The test" once it runs).
  fn unfinished(&self, subject: &str, end: &End) -> String {
    if let End::TimedOut = end {
      return format!("Timed out after {} ms", self.timeout.as_millis());
    }
    match locked(&self.shared.uncaught).first() {
      Some(message) => message.clone(),
      None => format!("{subject} did not finish: it waits on a promise that nothing settles"),
    }
  }
}

/// Start the session's test in an engine whose file has evaluated, and
/// report its end to the host.
fn start<'js>(
  ctx: Ctx<'js>,
  registry: Option<Registry>,
  name: &str,
  shared: Arc<Shared>,
  exec: ExecHandle,
  tx: tokio::sync::mpsc::UnboundedSender<Signal>,
) {
  let started = Instant::now();
  *locked(&shared.started) = Some(started);
  let finish = {
    let shared = shared.clone();
    move |error: Option<TestError>| {
      *locked(&shared.ended) = Some((started.elapsed().as_secs_f64() * 1000.0, error));
      // The host hears of the end two engine turns later: a rejection the
      // test left unhandled is reported at the checkpoint after the turn
      // that settled the test, and the host drops the engine on the signal.
      let relay = exec.clone();
      exec.exec(move |_| relay.exec(move |_| drop(tx.send(Signal::Finished))));
    }
  };
  let host_error = |message: String| Some(TestError { message, stack: String::new() });
  let Some(registry) = registry else {
    return finish(host_error(format!("The test \"{name}\" is gone: the file did not import flux:test this time")));
  };
  let promise = match registry.run_one(&ctx, name) {
    Ok(promise) => promise,
    Err(e) => return finish(host_error(format!("Test host: failed to start the test: {e}"))),
  };
  let mut finish = Some(finish);
  let settled = MutFn::from(move |value: Value<'_>| {
    let error = value.as_object().map(|thrown| TestError {
      message: thrown.get::<_, String>("message").unwrap_or_default(),
      stack: thrown.get::<_, String>("stack").unwrap_or_default(),
    });
    if let Some(finish) = finish.take() {
      finish(error);
    }
  });
  let attached = Function::new(ctx.clone(), settled)
    .and_then(|handler| promise.then().and_then(|then| then.call::<_, ()>((This(promise.clone()), handler))));
  if let Err(e) = attached {
    ctx.logger().error(&format!("Test host: failed to wait for the test: {e}"));
  }
}

/// One file's run: the listing engine first, then an engine per test that
/// passes the filter, in registration order.
pub struct FileRun {
  options: RunOptions,
  /// The tests still to run; None until the file is listed.
  queue: Option<VecDeque<String>>,
  passed: bool,
}

impl FileRun {
  pub fn new(options: RunOptions) -> Self {
    Self { options, queue: None, passed: true }
  }

  /// The session of the next engine: the host installs it on the engine's
  /// builder, builds the engine and hands both to `drive`.
  pub fn session(&self) -> Session {
    match self.queue.as_ref().and_then(|queue| queue.front()) {
      Some(name) => Session::run(name, &self.options),
      None => Session::list(&self.options),
    }
  }

  /// Evaluate the file in the session's engine, wait for its listing or
  /// result and hand the records that come of it to `emit`. Returns whether
  /// the run is over; if not, the host builds the next engine.
  pub async fn drive(
    &mut self,
    session: Session,
    engine: FluxEngine,
    code: ModuleCode,
    mut emit: impl FnMut(Record),
  ) -> bool {
    match self.queue.as_mut() {
      None => match session.listed(engine, code).await {
        Listed::Tests { names, output } => {
          let filter = self.options.filter.as_deref();
          let names: VecDeque<String> =
            names.into_iter().filter(|name| filter.is_none_or(|part| name.contains(part))).collect();
          emit(Record::Loaded { tests: names.iter().cloned().collect(), output });
          self.queue = Some(names);
        }
        Listed::Failed { message, output } => {
          emit(Record::Failed { message, output });
          self.passed = false;
          return true;
        }
      },
      Some(queue) => {
        let result = session.result(engine, code).await;
        queue.pop_front();
        self.passed &= result.ok;
        emit(Record::Result(result));
      }
    }
    let over = self.queue.as_ref().is_some_and(|queue| queue.is_empty());
    if over {
      emit(Record::Done);
    }
    over
  }

  /// Whether the file loaded and every test so far passed.
  pub fn passed(&self) -> bool {
    self.passed
  }
}

/// Run a test file: list its tests, run each one that passes the filter in
/// an engine of its own, and hand every record to `emit` as it comes.
/// `build` is called once per engine and returns the host's builder for
/// it (its userdata, plugins and isolate resolver; the logger and the
/// uncaught hook are the session's). Returns whether the file loaded and
/// every test passed.
pub async fn run_file(
  code: ModuleCode,
  options: RunOptions,
  build: impl Fn() -> FluxEngineBuilder,
  mut emit: impl FnMut(Record),
) -> bool {
  let mut run = FileRun::new(options);
  loop {
    let session = run.session();
    let engine = session.install(build()).build();
    if run.drive(session, engine, code.clone(), &mut emit).await {
      return run.passed();
    }
  }
}
