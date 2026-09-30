use rquickjs::JsLifetime;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// What an engine still has going on, as holds its plugins take. A hold is
/// one of two classes:
///
/// - work in flight: an operation that completes by itself (a fetch, a file
///   read, a query, a connect). `settled` waits for these.
/// - standing: something that lasts until its owner ends it or the outside
///   world does (a listening server, an open socket, a read waiting on a
///   peer, a running child, an event listener, a timer).
///
/// Both keep the engine alive: its loop ends when the job queue is dry and
/// nothing is held. Every hold names its kind, so whoever waits on the
/// engine can say what it is waiting for.
#[derive(Clone, JsLifetime)]
pub(crate) struct PendingOps {
  #[qjs(skip_trace)]
  inner: Arc<PendingOpsInner>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
  InFlight,
  Standing,
}

struct PendingOpsInner {
  // Holds of both classes: what keeps the engine alive.
  held: AtomicU32,
  in_flight: AtomicU32,
  // Bumped whenever work in flight starts or ends, so a waiter can tell
  // that none did between two looks.
  changes: AtomicU64,
  kinds: Mutex<BTreeMap<(Class, &'static str), u32>>,
  // The last hold was released.
  idle: tokio::sync::Notify,
  // The last work in flight ended.
  landed: tokio::sync::Notify,
  // The engine loop came round (see `turn`).
  turned: tokio::sync::Notify,
}

/// A hold on the engine, released when it is dropped: a future that is
/// dropped half way (its context went away) releases like one that ran to
/// its end.
pub struct Hold {
  ops: PendingOps,
  class: Class,
  kind: &'static str,
}

impl Drop for Hold {
  fn drop(&mut self) {
    self.ops.release(self.class, self.kind);
  }
}

impl PendingOps {
  pub(crate) fn new() -> Self {
    Self {
      inner: Arc::new(PendingOpsInner {
        held: AtomicU32::new(0),
        in_flight: AtomicU32::new(0),
        changes: AtomicU64::new(0),
        kinds: Mutex::new(BTreeMap::new()),
        idle: tokio::sync::Notify::new(),
        landed: tokio::sync::Notify::new(),
        turned: tokio::sync::Notify::new(),
      }),
    }
  }

  /// The pending ops of `ctx`'s engine.
  pub(crate) fn of(ctx: &rquickjs::Ctx<'_>) -> Self {
    ctx.userdata::<PendingOps>().expect("pending ops").clone()
  }

  /// Hold the engine for work that completes by itself. Take it where the
  /// work is started, not inside its future: what a script started is in
  /// flight from the call on, polled or not.
  pub(crate) fn in_flight(&self, kind: &'static str) -> Hold {
    self.hold(Class::InFlight, kind)
  }

  /// Hold the engine for something that lasts until it is ended.
  pub(crate) fn standing(&self, kind: &'static str) -> Hold {
    self.hold(Class::Standing, kind)
  }

  fn hold(&self, class: Class, kind: &'static str) -> Hold {
    *self.inner.kinds.lock().expect("pending kinds lock poisoned").entry((class, kind)).or_insert(0) += 1;
    if class == Class::InFlight {
      self.inner.in_flight.fetch_add(1, Ordering::SeqCst);
      self.inner.changes.fetch_add(1, Ordering::SeqCst);
    }
    self.inner.held.fetch_add(1, Ordering::SeqCst);
    Hold { ops: self.clone(), class, kind }
  }

  fn release(&self, class: Class, kind: &'static str) {
    {
      let mut kinds = self.inner.kinds.lock().expect("pending kinds lock poisoned");
      if let Some(count) = kinds.get_mut(&(class, kind)) {
        *count -= 1;
        if *count == 0 {
          kinds.remove(&(class, kind));
        }
      }
    }
    if class == Class::InFlight {
      self.inner.changes.fetch_add(1, Ordering::SeqCst);
      if self.inner.in_flight.fetch_sub(1, Ordering::SeqCst) == 1 {
        self.inner.landed.notify_waiters();
      }
    }
    if self.inner.held.fetch_sub(1, Ordering::SeqCst) == 1 {
      self.inner.idle.notify_waiters();
    }
  }

  pub(crate) fn is_idle(&self) -> bool {
    self.inner.held.load(Ordering::SeqCst) == 0
  }

  /// The release-to-zero notification as a future, not awaited here: the
  /// engine loop pins it and calls `enable()` to register interest before
  /// re-checking the count (Notify stores no permit, so registering after a
  /// racing release would lose the wakeup).
  pub(crate) fn notified(&self) -> tokio::sync::futures::Notified<'_> {
    self.inner.idle.notified()
  }

  /// The engine loop came round: whoever waits on a turn goes on.
  pub(crate) fn turn(&self) {
    self.inner.turned.notify_waiters();
  }

  /// The work in flight, as a count per kind.
  pub(crate) fn in_flight_kinds(&self) -> Vec<(&'static str, u32)> {
    let kinds = self.inner.kinds.lock().expect("pending kinds lock poisoned");
    kinds.iter().filter(|((class, _), _)| *class == Class::InFlight).map(|((_, kind), count)| (*kind, *count)).collect()
  }

  fn nothing_in_flight(&self) -> bool {
    self.inner.in_flight.load(Ordering::SeqCst) == 0
  }

  /// Resolves once nothing is in flight and what the finished work woke has
  /// run: the job queue is dry and no work started or ended since. Standing
  /// holds are not waited for.
  ///
  /// Awaited in a task of the engine (`ctx.spawn`, a `Promised`). Such a
  /// task is polled only when the job queue is dry, but a task polled ahead
  /// of it in the same pass may have queued jobs by finishing work. So after
  /// the count reads zero this waits for the engine loop to come round once
  /// (the wake then arrives from outside any pass) and looks again: if no
  /// work started or ended meanwhile, the jobs of everything that finished
  /// have run and started nothing.
  pub(crate) async fn settled(&self) {
    loop {
      loop {
        let landed = self.inner.landed.notified();
        tokio::pin!(landed);
        landed.as_mut().enable();
        if self.nothing_in_flight() {
          break;
        }
        landed.await;
      }
      let seen = self.inner.changes.load(Ordering::SeqCst);
      {
        let turned = self.inner.turned.notified();
        tokio::pin!(turned);
        turned.as_mut().enable();
        // Bring the engine loop round: it parks on this notification.
        self.inner.idle.notify_waiters();
        turned.await;
      }
      if self.nothing_in_flight() && self.inner.changes.load(Ordering::SeqCst) == seen {
        return;
      }
    }
  }
}

/// Hold the engine of `ctx` for an embedder's own work on the engine's
/// behalf that is in flight outside it (a frame a test asked the runner
/// for), which a promise alone does not do: the loop ends when the job
/// queue is dry and nothing is held. `kind` names the work, for whoever
/// reports what an engine waits on. Released on drop.
pub fn hold_engine(ctx: &rquickjs::Ctx<'_>, kind: &'static str) -> Hold {
  PendingOps::of(ctx).in_flight(kind)
}

/// A future that resolves once nothing in `ctx`'s engine is in flight and
/// the job queue is dry (see `PendingOps::settled`). Await it in a task of
/// the engine.
pub fn settled(ctx: &rquickjs::Ctx<'_>) -> impl std::future::Future<Output = ()> + 'static {
  let pending = PendingOps::of(ctx);
  async move { pending.settled().await }
}

/// The work in flight in `ctx`'s engine, as a count per kind.
pub fn in_flight(ctx: &rquickjs::Ctx<'_>) -> Vec<(&'static str, u32)> {
  PendingOps::of(ctx).in_flight_kinds()
}

/// `kinds` as a sentence part: "2 fetch, 1 file read".
pub fn describe_in_flight(kinds: &[(&'static str, u32)]) -> String {
  kinds.iter().map(|(kind, count)| format!("{count} {kind}")).collect::<Vec<_>>().join(", ")
}
