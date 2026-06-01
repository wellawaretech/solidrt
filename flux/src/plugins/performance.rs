use std::sync::{Arc, OnceLock};
use std::time::Instant;

use rquickjs::{Ctx, Function, JsLifetime, Object};

// Process-wide monotonic origin, used when no Clock is injected.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

fn default_now_ms() -> f64 {
  ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

// A high-resolution clock an embedder can inject via the engine builder
// (`.userdata(clock)`), so performance.now() and other time sources (e.g. the
// requestAnimationFrame timestamp) report on one shared origin. Without one,
// performance.now() falls back to a process-wide monotonic origin.
#[derive(Clone, JsLifetime)]
pub struct Clock {
  #[qjs(skip_trace)]
  now: Arc<dyn Fn() -> f64 + Send + Sync>,
}

impl Clock {
  pub fn new(f: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
    Self { now: Arc::new(f) }
  }

  pub fn now_ms(&self) -> f64 {
    (self.now)()
  }
}

fn perf_now(ctx: Ctx<'_>) -> f64 {
  match ctx.userdata::<Clock>() {
    Some(clock) => clock.now_ms(),
    None => default_now_ms(),
  }
}

pub(crate) fn init_performance(ctx: &Ctx<'_>) {
  let performance = Object::new(ctx.clone()).expect("create performance object");
  let now = Function::new(ctx.clone(), perf_now).expect("create performance.now");
  performance.set("now", now).expect("set performance.now");
  ctx.globals().set("performance", performance).expect("set performance global");
}
