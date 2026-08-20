//! Engine-free half of isolates: the link between two runtimes, the call
//! protocol that travels on it, and the kill switch that ends one.
//!
//! A `Link` is one end of a bidirectional, unbounded message queue. Calls go
//! parent -> child (`Msg::Call`), replies child -> parent (`Msg::Reply`), and
//! a child's uncaught errors child -> parent (`Msg::Error`). A call whose
//! result is a stream (an async iterable) is announced with `Msg::Stream` and
//! then pulled item by item (`Msg::Next` -> `Msg::Yield`) until a `Reply` ends
//! it, or `Msg::Return` ends it early. Arguments and
//! results are neutral `Value`s (copied, shared-nothing). `Link::pair` makes
//! the two ends; the host hands one to each runtime. Which runtime a link
//! belongs to and how the peer runs (a thread, an engine) is the host's
//! business (see okf/done/isolates-and-ports.md).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, Notify};

use crate::Value;

/// What a call rejects with, crossing the link as data: an error (rebuilt as
/// a real error on the peer), or - when the throw was not an Error - the
/// thrown value itself, sendable values only. Internal failures (a missing
/// export, a closed link, an exit) are plain messages via `From`.
pub enum Thrown {
  Error(CallError),
  Value(Value),
}

impl From<String> for Thrown {
  fn from(message: String) -> Self {
    Thrown::Error(CallError { name: "Error".to_string(), message, stack: None, cause: None })
  }
}

/// A thrown error as data: the error's `name` (its constructor's), `message`,
/// its stack text, and its `cause` chain - each cause another error or a
/// sendable value; an unsendable cause is dropped, as is anything past the
/// sender's depth cap (which also ends a cyclic chain).
pub struct CallError {
  pub name: String,
  pub message: String,
  pub stack: Option<String>,
  pub cause: Option<Box<Thrown>>,
}

/// One message on a link.
pub enum Msg {
  /// Parent -> child: call the module export `name` with `args`; answer with
  /// a `Reply` carrying the same `id`.
  Call { id: u64, name: String, args: Vec<Value> },
  /// Child -> parent: the outcome of the call `id` (a throw as `Thrown`
  /// data). For a stream this is the end of it: `Ok` when the iterator
  /// completed, `Err` when it threw.
  Reply { id: u64, result: Result<Value, Thrown> },
  /// Child -> parent: call `id` returned an async iterable; its items follow
  /// as `Yield`s, one per `Next`.
  Stream { id: u64 },
  /// Parent -> child: pull the next item of stream `id`.
  Next { id: u64 },
  /// Parent -> child: end stream `id` early (the iterator's `return()`).
  Return { id: u64 },
  /// Child -> parent: one item of stream `id`.
  Yield { id: u64, value: Value },
  /// Child -> parent: an uncaught error not tied to a call (already logged on
  /// the child's side).
  Error(String),
}

/// One end of a port. Cheaply cloned; every clone shares the same queue ends.
#[derive(Clone)]
pub struct Link {
  tx: Arc<Mutex<Option<mpsc::UnboundedSender<Msg>>>>,
  rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Msg>>>,
}

impl Link {
  /// The two ends of a new port.
  pub fn pair() -> (Link, Link) {
    let (a_tx, a_rx) = mpsc::unbounded_channel();
    let (b_tx, b_rx) = mpsc::unbounded_channel();
    let a = Link { tx: Arc::new(Mutex::new(Some(a_tx))), rx: Arc::new(tokio::sync::Mutex::new(b_rx)) };
    let b = Link { tx: Arc::new(Mutex::new(Some(b_tx))), rx: Arc::new(tokio::sync::Mutex::new(a_rx)) };
    (a, b)
  }

  /// Queue a message for the peer. `Err` only when this end is closed; a peer
  /// that is gone drops the message silently, like posting to an exited
  /// worker.
  pub fn send(&self, msg: Msg) -> Result<(), String> {
    match self.tx.lock().expect("link sender poisoned").as_ref() {
      Some(tx) => {
        let _ = tx.send(msg);
        Ok(())
      }
      None => Err("port is closed".to_string()),
    }
  }

  /// Wait for the next message; `None` once the peer closed its end (or is
  /// gone) and the queue is drained. Concurrent callers are served in turn.
  pub async fn recv(&self) -> Option<Msg> {
    self.rx.lock().await.recv().await
  }

  /// Stop sending. The peer's `recv` reports `None` once it has drained
  /// what was already queued. Idempotent.
  pub fn close(&self) {
    self.tx.lock().expect("link sender poisoned").take();
  }
}

/// A one-shot kill switch for a runtime: `flag` is polled by the runtime's
/// interrupt handler (unwinds busy JS), `notify` wakes the host loop that owns
/// the runtime so it can drop it.
#[derive(Default)]
pub struct Kill {
  flag: Arc<AtomicBool>,
  notify: Notify,
}

impl Kill {
  /// The flag to hand the runtime's interrupt handler.
  pub fn flag(&self) -> Arc<AtomicBool> {
    self.flag.clone()
  }

  pub fn fire(&self) {
    self.flag.store(true, Ordering::Relaxed);
    self.notify.notify_one();
  }

  /// Resolves once `fire` has been called (before or after the await).
  pub async fn fired(&self) {
    self.notify.notified().await;
  }
}
