// Seeded randomness: `seed_random` makes a context's Math.random a
// reproducible sequence, read here through the real global. A context nobody
// seeded keeps the engine's builtin, which these tests do not cover: it is
// random by design.

use rquickjs::{Context, Ctx, Runtime};

use crate::standards_plugins::random::{child_seed, seed_random};

fn with_ctx(f: impl FnOnce(&Ctx<'_>)) {
  let rt = Runtime::new().expect("js runtime");
  let context = Context::full(&rt).expect("js context");
  context.with(|ctx| f(&ctx));
}

/// The next `n` values of the context's Math.random.
fn draw(ctx: &Ctx<'_>, n: usize) -> Vec<f64> {
  (0..n).map(|_| ctx.eval::<f64, _>("Math.random()").expect("draw")).collect()
}

/// The first `n` values a fresh context draws once seeded with `seed`.
fn sequence(seed: u64, n: usize) -> Vec<f64> {
  let mut values = Vec::new();
  with_ctx(|ctx| {
    seed_random(ctx, seed).expect("seed");
    values = draw(ctx, n);
  });
  values
}

// Pinned against an independent implementation (splitmix64 of the seed into
// the state, then xorshift64* with the top 52 bits as the mantissa): the
// sequence of a seed is part of the contract, since a seed someone noted
// down has to mean the same inputs on another machine and after an upgrade.
#[test]
fn a_seed_names_one_sequence_on_every_run() {
  assert_eq!(sequence(0, 3), [0.4833481342839381, 0.8691389606829487, 0.7022433404894404]);
  assert_eq!(sequence(1, 3), [0.29404672187536485, 0.843291357405598, 0.37141301636381585]);
  assert_eq!(sequence(12345, 3), [0.28097516969868397, 0.20629907413712711, 0.22156272376249864]);
}

#[test]
fn values_lie_in_the_unit_interval_at_the_engines_resolution() {
  // 2^32 and 2^52 as doubles: a generator with 32 random bits would only
  // ever return multiples of 2^-32.
  const TWO_POW_32: f64 = 4294967296.0;
  const TWO_POW_52: f64 = 4503599627370496.0;
  // How many values are drawn, and how far their mean may sit from a
  // uniform draw's 0.5 (several standard errors at that count).
  const SAMPLES: usize = 1000;
  const MEAN_TOLERANCE: f64 = 0.05;
  let values = sequence(7, SAMPLES);
  assert!(values.iter().all(|v| (0.0..1.0).contains(v)), "a value left [0, 1)");
  assert!(values.iter().all(|v| (v * TWO_POW_52).fract() == 0.0), "a value is finer than 52 bits");
  assert!(values.iter().any(|v| (v * TWO_POW_32).fract() != 0.0), "every value fits in 32 bits");
  let mean = values.iter().sum::<f64>() / values.len() as f64;
  assert!((mean - 0.5).abs() < MEAN_TOLERANCE, "mean {mean} is far from a uniform draw's");
}

#[test]
fn neighboring_seeds_start_unrelated_sequences() {
  // Two values closer than this count as the same: seeds fed to the
  // generator unmixed would start sequences that differ in a few low bits.
  const SAME: f64 = 1e-6;
  let one = sequence(1, 8);
  let two = sequence(2, 8);
  assert!(one.iter().zip(&two).all(|(a, b)| (a - b).abs() > SAME), "seeds 1 and 2 agree on a value: {one:?} {two:?}");
}

#[test]
fn seeding_again_restarts_the_sequence() {
  with_ctx(|ctx| {
    seed_random(ctx, 5).expect("seed");
    let first = draw(ctx, 4);
    seed_random(ctx, 5).expect("seed again");
    assert_eq!(draw(ctx, 4), first);
    seed_random(ctx, 6).expect("seed another");
    assert_ne!(draw(ctx, 4), first);
    assert_eq!(first, sequence(5, 4));
  });
}

#[test]
fn the_seeded_function_looks_like_the_builtin() {
  with_ctx(|ctx| {
    seed_random(ctx, 0).expect("seed");
    let shape: String = ctx
      .eval(
        "let d = Object.getOwnPropertyDescriptor(Math, 'random'); \
         [Math.random.name, Math.random.length, d.writable, d.enumerable, d.configurable].join()",
      )
      .expect("read shape");
    assert_eq!(shape, "random,0,true,false,true");
  });
}

// A spawned context's seed comes from its parent's seed and its place among
// the children, never from the parent's own sequence: spawning one must not
// change what the parent draws.
#[test]
fn child_seeds_are_derived_without_drawing() {
  with_ctx(|ctx| {
    assert_eq!(child_seed(ctx), None, "an unseeded context seeds no child");
    seed_random(ctx, 1).expect("seed");
    assert_eq!(child_seed(ctx), Some(13757245211066428519));
    assert_eq!(child_seed(ctx), Some(17911839290282890590));
    assert_eq!(draw(ctx, 3), sequence(1, 3));
    // Seeding again starts the children over, as it does the sequence.
    seed_random(ctx, 1).expect("seed again");
    assert_eq!(child_seed(ctx), Some(13757245211066428519));
  });
}
