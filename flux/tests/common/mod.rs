// Shared test harness for the flux integration tests. Each test file declares
// `mod common;` and pulls these in. Not every file uses every helper, so dead
// code is expected per test crate.
#![allow(dead_code)]

use flux::{FluxEngine, LogLevel};
use std::sync::{Arc, Mutex};

/// A log sink that records every (level, message) the engine emits. Wire it
/// into the builder with `logger()` and read results with `captured()`.
#[derive(Clone)]
pub struct LogSink {
  entries: Arc<Mutex<Vec<(LogLevel, String)>>>,
}

impl LogSink {
  pub fn new() -> Self {
    Self { entries: Arc::new(Mutex::new(Vec::new())) }
  }

  /// A logger callback that appends to this sink. Pass to
  /// `FluxEngine::builder().logger(..)`.
  pub fn logger(&self) -> impl Fn(LogLevel, &str) + Send + Sync + 'static {
    let entries = self.entries.clone();
    move |level, msg| entries.lock().expect("log sink poisoned").push((level, msg.to_string()))
  }

  /// Snapshot the captured log so far.
  pub fn captured(&self) -> Captured {
    Captured { entries: self.entries.lock().expect("log sink poisoned").clone() }
  }
}

/// A snapshot of captured log entries, with accessors for asserting on output
/// by level.
pub struct Captured {
  entries: Vec<(LogLevel, String)>,
}

impl Captured {
  /// All `console.log` messages joined by newline (the common assertion).
  pub fn log(&self) -> String {
    self.at(LogLevel::Log)
  }

  /// All `console.error` messages joined by newline.
  pub fn errors(&self) -> String {
    self.at(LogLevel::Error)
  }

  /// All messages at `level`, joined by newline.
  pub fn at(&self, level: LogLevel) -> String {
    self.lines_at(level).join("\n")
  }

  /// Messages at `level` as a vector, for exact per-line assertions.
  pub fn lines_at(&self, level: LogLevel) -> Vec<&str> {
    self.entries.iter().filter(|(l, _)| *l == level).map(|(_, m)| m.as_str()).collect()
  }

  /// Whether any message was logged at error level.
  pub fn has_error(&self) -> bool {
    self.entries.iter().any(|(l, _)| *l == LogLevel::Error)
  }
}

/// Build an engine, evaluate `code` as a module, run the event loop to
/// completion, and return the captured log. Covers the common single-shot
/// test case.
pub async fn run_source(code: &str) -> Captured {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  engine.eval_source(code).await;
  sink.captured()
}
