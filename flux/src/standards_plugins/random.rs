use rquickjs::{function::MutFn, Ctx, Function, JsLifetime, Object};
use std::cell::Cell;
use std::rc::Rc;

// ----- Seeded randomness: a reproducible Math.random -----

// The engine seeds its own `Math.random` from the clock when the context is
// created and offers no way to seed it, so a run that must be reproducible
// cannot use it. A host that needs one (flux:test for each test; a headless
// render or a playback are the same kind of host) opts in per context with
// `seed_random`: from then on `Math.random` is flux's generator, the same
// algorithm and resolution as the engine's, started from the seed. A context
// nobody seeded keeps the engine's builtin untouched, which is every
// shipping app: the seeded function is a native call, the builtin is not.
//
// A context spawned by a seeded one (flux:isolate) is seeded too, with a
// seed derived from its parent's (`child_seed`), so a test's isolates are as
// reproducible as the test.

/// The generator is xorshift64*, the one the engine's `Math.random` runs:
/// three shifts of the state, and the multiplier that scrambles the output.
const XORSHIFT_A: u32 = 12;
const XORSHIFT_B: u32 = 25;
const XORSHIFT_C: u32 = 27;
const XORSHIFT_STAR_MULTIPLIER: u64 = 0x2545_F491_4F6C_DD1D;

/// A double holds 52 random bits, as the engine's does: the top 52 bits of
/// the generator's output become the mantissa of a number in [1, 2).
const MANTISSA_BITS: u32 = 52;
/// The bit pattern of 1.0: the exponent that puts the mantissa in [1, 2).
const ONE_BITS: u64 = 0x3ff << MANTISSA_BITS;

/// splitmix64, which turns a seed into well-mixed 64-bit values: the step
/// added per value, and the shifts and multipliers of its finalizer. A seed
/// is a small number a person types; fed to xorshift directly, seeds 1 and 2
/// would start two nearly equal sequences.
const SPLITMIX_STEP: u64 = 0x9E37_79B9_7F4A_7C15;
const SPLITMIX_SHIFT_A: u32 = 30;
const SPLITMIX_SHIFT_B: u32 = 27;
const SPLITMIX_SHIFT_C: u32 = 31;
const SPLITMIX_MULTIPLIER_A: u64 = 0xBF58_476D_1CE4_E5B9;
const SPLITMIX_MULTIPLIER_B: u64 = 0x94D0_49BB_1331_11EB;

/// The generator of a seeded context, kept in its userdata and shared with
/// the `Math.random` that reads it.
#[derive(Clone, JsLifetime)]
struct SeededRandom(#[qjs(skip_trace)] Rc<RandomState>);

struct RandomState {
  seed: Cell<u64>,
  state: Cell<u64>,
  // How many child seeds were handed out since the last seeding.
  children: Cell<u64>,
}

/// The `index`th value of the seed's splitmix64 sequence. Value 0 is the
/// generator's starting state, the ones after it are the children's seeds.
fn splitmix(seed: u64, index: u64) -> u64 {
  let mut z = seed.wrapping_add(SPLITMIX_STEP.wrapping_mul(index.wrapping_add(1)));
  z = (z ^ (z >> SPLITMIX_SHIFT_A)).wrapping_mul(SPLITMIX_MULTIPLIER_A);
  z = (z ^ (z >> SPLITMIX_SHIFT_B)).wrapping_mul(SPLITMIX_MULTIPLIER_B);
  z ^ (z >> SPLITMIX_SHIFT_C)
}

impl RandomState {
  fn restart(&self, seed: u64) {
    self.seed.set(seed);
    // xorshift never leaves zero, so the one seed that mixes to it starts
    // from the step instead.
    let state = splitmix(seed, 0);
    self.state.set(if state == 0 { SPLITMIX_STEP } else { state });
    self.children.set(0);
  }

  /// The next number in [0, 1).
  fn next(&self) -> f64 {
    let mut x = self.state.get();
    x ^= x >> XORSHIFT_A;
    x ^= x << XORSHIFT_B;
    x ^= x >> XORSHIFT_C;
    self.state.set(x);
    let bits = x.wrapping_mul(XORSHIFT_STAR_MULTIPLIER) >> (u64::BITS - MANTISSA_BITS);
    f64::from_bits(ONE_BITS | bits) - 1.0
  }
}

/// Make this context's `Math.random` reproducible: it returns the sequence
/// of `seed` from its start, the same one on every run and every platform.
/// Seeding again restarts the sequence (of that seed, or another), which is
/// how a host gives each unit of work its own start (flux:test, per test).
pub fn seed_random(ctx: &Ctx<'_>, seed: u64) -> rquickjs::Result<()> {
  if let Some(random) = ctx.userdata::<SeededRandom>() {
    random.0.restart(seed);
    return Ok(());
  }
  let random = Rc::new(RandomState { seed: Cell::new(0), state: Cell::new(0), children: Cell::new(0) });
  random.restart(seed);
  let math: Object = ctx.globals().get("Math")?;
  let source = random.clone();
  let next = Function::new(ctx.clone(), MutFn::from(move || source.next()))?.with_name("random")?;
  math.set("random", next)?;
  ctx.store_userdata(SeededRandom(random)).expect("store seeded random");
  Ok(())
}

/// The seed for a context this one spawns: derived from this context's seed
/// and how many it spawned since it was seeded, without drawing from its
/// `Math.random` sequence, so a spawn does not change what the parent draws.
/// None when this context is not seeded: its children keep the engine's
/// clock-seeded builtin, as it does.
pub(crate) fn child_seed(ctx: &Ctx<'_>) -> Option<u64> {
  let random = ctx.userdata::<SeededRandom>()?;
  let index = random.0.children.get() + 1;
  random.0.children.set(index);
  Some(splitmix(random.0.seed.get(), index))
}
