//! The dialog session of one engine (see the module doc): one request at a
//! time, the answers given ahead, the answers-only mode, and the show over
//! the main-thread bridge when the session decides the platform answers.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use super::{
  availability, platform, set_availability, DialogAnswer, DialogError, DialogRequest, DialogResponder, Picked,
};
use crate::availability::Availability;

/// Told, with the request, once a request took an answer given ahead.
pub type Consumed = Box<dyn FnOnce(DialogRequest) + Send>;

/// What shows a dialog on the platform, on the main thread: the backend's
/// `show`, or what a test stands in with.
pub(crate) type Show = fn(DialogRequest, DialogResponder);

// The request waiting for its answer: parked on the queue (answers only),
// or on the platform's dialog, whose own answer carries the id and is
// dropped if an outside answer settled the request first.
struct Pending {
  id: u64,
  request: DialogRequest,
  respond: DialogResponder,
}

// An answer given ahead, with whom to tell once a request took it.
struct Queued {
  picked: Vec<Picked>,
  consumed: Option<Consumed>,
}

struct State {
  answers_only: bool,
  queue: VecDeque<Queued>,
  pending: Option<Pending>,
  /// The last request made, with its answer once it has one.
  last: Option<(DialogRequest, Option<DialogAnswer>)>,
  next_id: u64,
}

/// What an embedder's control surface reports about a session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogStatus {
  /// Whether this platform can show a dialog (`availability`).
  pub availability: Availability,
  /// The request waiting for an answer.
  pub pending: Option<DialogRequest>,
  /// The last request made, with its answer once it has one.
  pub last: Option<(DialogRequest, Option<DialogAnswer>)>,
  /// How many answers given ahead wait for a request.
  pub queued: usize,
}

/// The dialog session of one engine (see the module doc).
pub struct Session {
  state: Mutex<State>,
  show: Show,
}

// What `begin` decided under the lock, acted on outside it (a responder may
// reach back into the session).
enum Next {
  Refuse(DialogResponder, DialogError),
  Consumed(DialogResponder, Vec<Picked>, Option<Consumed>),
  Parked(Option<u64>),
}

impl Session {
  pub fn new() -> Arc<Session> {
    Session::with_show(platform::show)
  }

  /// A session whose platform dialog is `show`: the backend's, or a test's.
  pub(crate) fn with_show(show: Show) -> Arc<Session> {
    Arc::new(Session {
      state: Mutex::new(State { answers_only: false, queue: VecDeque::new(), pending: None, last: None, next_id: 1 }),
      show,
    })
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Never show a dialog in this session: every request waits on the queue
  /// (a test host's engine, which has no surface to show one on and whose
  /// test answers through the harness).
  pub fn set_answers_only(&self) {
    self.lock().answers_only = true;
  }

  /// Begin a request: refuse it while another waits, answer it from the
  /// queue, park it for an answer (answers only), refuse it where dialogs
  /// are Unavailable, or show the platform's dialog over the main-thread
  /// bridge, with the window as its parent (an Unknown platform is tried).
  /// `respond` is called once, with the picks or why there are none, from
  /// whatever thread the answer arrives on.
  pub fn begin(self: &Arc<Self>, request: DialogRequest, respond: DialogResponder) {
    let next = {
      let mut s = self.lock();
      if s.pending.is_some() {
        Next::Refuse(respond, DialogError::Busy)
      } else if let Some(queued) = s.queue.pop_front() {
        s.last = Some((request.clone(), Some(Ok(queued.picked.clone()))));
        Next::Consumed(respond, queued.picked, queued.consumed)
      } else if !s.answers_only && availability() == Availability::Unavailable {
        s.last = Some((request.clone(), Some(Err(DialogError::Unavailable))));
        Next::Refuse(respond, DialogError::Unavailable)
      } else {
        let id = s.next_id;
        s.next_id += 1;
        let shown = !s.answers_only;
        s.last = Some((request.clone(), None));
        s.pending = Some(Pending { id, request: request.clone(), respond });
        Next::Parked(shown.then_some(id))
      }
    };
    match next {
      Next::Refuse(respond, error) => respond(Err(error)),
      Next::Consumed(respond, picked, consumed) => {
        respond(Ok(picked));
        if let Some(consumed) = consumed {
          consumed(request);
        }
      }
      Next::Parked(None) => {}
      Next::Parked(Some(id)) => {
        let session = self.clone();
        let show = self.show;
        crate::main_thread::run(move |on_main| match on_main {
          Ok(()) => show(request, Box::new(move |answer| session.settle(id, answer))),
          Err(error) => session.settle(id, Err(DialogError::Failed(format!("dialog: {error}")))),
        });
      }
    }
  }

  // The platform's answer to request `id`: taken if that request still
  // waits, dropped if an outside answer settled it (or a later request
  // waits under another id). Either way it corrects the fact first (see
  // `correct`). Crate-visible for the tests, which stand in for a
  // platform's late answer.
  pub(crate) fn settle(&self, id: u64, answer: DialogAnswer) {
    correct(&answer);
    let taken = {
      let mut s = self.lock();
      if s.pending.as_ref().is_some_and(|p| p.id == id) {
        record(&mut s, &answer);
        s.pending.take()
      } else {
        None
      }
    };
    if let Some(pending) = taken {
      (pending.respond)(answer);
    }
  }

  /// Answer the waiting request, or keep the answer for the next one.
  /// `consumed` is told with the request once one took the answer (at once
  /// when one is waiting). Returns whether a request was waiting.
  pub fn answer(&self, picked: Vec<Picked>, consumed: Option<Consumed>) -> bool {
    let pending = {
      let mut s = self.lock();
      match s.pending.take() {
        Some(pending) => {
          record(&mut s, &Ok(picked.clone()));
          pending
        }
        None => {
          s.queue.push_back(Queued { picked, consumed });
          return false;
        }
      }
    };
    (pending.respond)(Ok(picked));
    if let Some(consumed) = consumed {
      consumed(pending.request);
    }
    true
  }

  pub fn status(&self) -> DialogStatus {
    let s = self.lock();
    DialogStatus {
      availability: availability(),
      pending: s.pending.as_ref().map(|p| p.request.clone()),
      last: s.last.clone(),
      queued: s.queue.len(),
    }
  }
}

// A call corrects the fact (see `crate::availability`): a platform answer
// that says there is no dialog here records Unavailable, one that worked (a
// cancel included) on an Unknown facility records Available. Any other
// failure is about this request, not the facility, and changes nothing.
fn correct(answer: &DialogAnswer) {
  match answer {
    Err(DialogError::Unavailable) => set_availability(Availability::Unavailable),
    Ok(_) if availability() == Availability::Unknown => set_availability(Availability::Available),
    Ok(_) | Err(DialogError::Busy | DialogError::Failed(_)) => {}
  }
}

fn record(s: &mut State, answer: &DialogAnswer) {
  if let Some((_, slot)) = s.last.as_mut() {
    *slot = Some(answer.clone());
  }
}
