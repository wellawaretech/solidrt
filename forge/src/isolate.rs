//! Engine-free half of isolates: the port link between two runtimes and the
//! kill switch that ends one.
//!
//! A `Link` is one end of a bidirectional, unbounded message queue carrying
//! neutral `Value`s (copied, shared-nothing) plus the peer's uncaught errors.
//! `Link::pair` makes the two ends; the host hands one to each runtime. Which
//! runtime a link belongs to and how the peer runs (a thread, an engine) is the
//! host's business (see okf/backlog/isolates-and-ports.md).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, Notify};

use crate::Value;

/// One message on a link.
pub enum Msg {
  Value(Value),
  /// The peer reported an uncaught error (already logged on its side).
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

  /// Stop sending. The peer's `recv` reports `Closed` once it has drained
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
