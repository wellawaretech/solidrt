// flux:test driven through its host: a file registers tests, the host
// lists them and runs each in an engine of its own, and the assertions here
// read the records. The module's JS half (test_plugins/test.js) has no
// other harness to stand on, so its matchers are checked from outside, as
// plain pass/throw tables inside one test.

use std::time::Duration;

use crate::test::{run_file, Record, RunOptions, TestResult};
use crate::{FluxEngine, ModuleCode};

/// Run `source` as a test file and return every record of the host's.
fn records_with(source: &str, options: RunOptions) -> Vec<Record> {
  let mut records = Vec::new();
  tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime").block_on(run_file(
    ModuleCode::Source(source.to_string()),
    options,
    FluxEngine::builder,
    |record| records.push(record),
  ));
  records
}

fn results_with(source: &str, options: RunOptions) -> Vec<TestResult> {
  records_with(source, options)
    .into_iter()
    .filter_map(|record| match record {
      Record::Result(result) => Some(result),
      _ => None,
    })
    .collect()
}

fn results(source: &str) -> Vec<TestResult> {
  results_with(source, RunOptions::default())
}

/// `name|ok|error message` of every result, one per line.
fn rows(results: &[TestResult]) -> String {
  results
    .iter()
    .map(|r| format!("{}|{}|{}", r.name, r.ok, r.error.as_ref().map(|e| e.message.as_str()).unwrap_or("")))
    .collect::<Vec<_>>()
    .join("\n")
}

/// What the one test of `source` logged, as one text.
fn logged(source: &str) -> String {
  let results = results(source);
  assert_eq!(results.len(), 1, "expected one test, got {results:?}");
  assert!(results[0].ok, "the test failed: {:?}", results[0].error);
  results[0].output.join("\n")
}

#[test]
fn results_come_in_registration_order_with_their_errors() {
  let results = results(
    r#"
    import { test, expect } from "flux:test"
    test("passes", () => { expect(1 + 1).toBe(2) })
    test("fails an expectation", () => { expect(1 + 1).toBe(3) })
    test("throws", () => { throw new RangeError("out of range") })
    test("awaits", async () => { await Promise.resolve(); expect("a").toBe("a") })
    test("throws a non-error", () => { throw "plain" })
    "#,
  );
  assert_eq!(
    rows(&results),
    [
      "passes|true|",
      "fails an expectation|false|AssertionError: expect(received).toBe(expected)\nExpected: 3\nReceived: 2",
      "throws|false|RangeError: out of range",
      "awaits|true|",
      "throws a non-error|false|Thrown: \"plain\"",
    ]
    .join("\n")
  );
}

#[test]
fn the_records_open_with_the_listing_and_close_with_done() {
  let records = records_with(
    r#"
    import { test } from "flux:test"
    console.log("loading")
    test("one", () => { console.log("in one") })
    test("two", () => {})
    "#,
    RunOptions::default(),
  );
  assert_eq!(records.len(), 4);
  assert_eq!(records[0], Record::Loaded { tests: vec!["one".into(), "two".into()], output: vec!["loading".into()] });
  // What the file logs while it loads is the listing's; a test's output is
  // what its engine logged after that.
  let Record::Result(one) = &records[1] else { panic!("expected a result, got {:?}", records[1]) };
  assert_eq!(one.output, vec!["in one".to_string()]);
  assert_eq!(records[3], Record::Done);
  assert!(records[1].line().starts_with("\u{1e}sol-test {"));
}

#[test]
fn a_failure_stack_cites_the_test_and_the_harness_by_name() {
  let results = results(
    r#"
    import { test, expect } from "flux:test"
    test("fails", () => { expect(1).toBe(2) })
    "#,
  );
  let stack = &results[0].error.as_ref().expect("the test fails").stack;
  assert!(stack.contains("(main:"), "no frame of the file in {stack}");
  assert!(stack.contains("(flux:test:"), "no frame of the harness in {stack}");
}

#[test]
fn the_filter_selects_tests_by_a_part_of_their_name() {
  let results = results_with(
    r#"
    import { test } from "flux:test"
    test("matchPath: a literal", () => {})
    test("formatPath: a param", () => {})
    test("matchPath: a param", () => {})
    "#,
    RunOptions { filter: Some("matchPath".into()), ..RunOptions::default() },
  );
  assert_eq!(rows(&results), "matchPath: a literal|true|\nmatchPath: a param|true|");
}

// Every test runs in an engine of its own, on the file evaluated afresh:
// what a test did to the file's module state is not there for the next.
#[test]
fn every_test_starts_from_the_files_fresh_state() {
  let results = results(
    r#"
    import { test, expect } from "flux:test"
    let count = 0
    test("first", () => { count++; expect(count).toBe(1) })
    test("second", () => { count++; expect(count).toBe(1) })
    "#,
  );
  assert_eq!(rows(&results), "first|true|\nsecond|true|");
}

// A promise that never settles holds nothing in the engine, which then runs
// out of work: the host reports that at once instead of waiting for the cap.
#[test]
fn a_test_waiting_on_nothing_fails_and_the_run_continues() {
  let results = results(
    r#"
    import { test } from "flux:test"
    test("hangs", () => new Promise(() => {}))
    test("after", () => {})
    "#,
  );
  assert_eq!(
    rows(&results),
    "hangs|false|The test did not finish: it waits on a promise that nothing settles\nafter|true|"
  );
}

// The cap ends a test that keeps the engine busy, a synchronous loop
// included, and what it left running does not reach the test after it.
#[test]
fn a_test_past_the_cap_times_out_and_the_run_continues() {
  let results = results_with(
    r#"
    import { test } from "flux:test"
    test("waits on a timer", () => new Promise((resolve) => setTimeout(resolve, 60000)))
    test("loops", () => { for (;;) {} })
    test("leaks an interval", () => { setInterval(() => console.log("tick"), 1) })
    test("after", () => new Promise((resolve) => setTimeout(resolve, 20)))
    "#,
    RunOptions { timeout: Duration::from_millis(100), ..RunOptions::default() },
  );
  assert_eq!(
    rows(&results),
    [
      "waits on a timer|false|Timed out after 100 ms",
      "loops|false|Timed out after 100 ms",
      "leaks an interval|true|",
      "after|true|",
    ]
    .join("\n")
  );
  assert_eq!(results[1].output, Vec::<String>::new());
  assert_eq!(results[3].output, Vec::<String>::new());
}

// An uncaught error belongs to the test whose engine raised it.
#[test]
fn an_uncaught_error_fails_the_test_it_happened_in() {
  let results = results(
    r#"
    import { test } from "flux:test"
    test("throws in a timer", async () => {
      setTimeout(() => { throw new Error("from the timer") }, 1)
      await new Promise((resolve) => setTimeout(resolve, 20))
    })
    test("rejects unhandled", () => { Promise.reject(new Error("nobody handles this")) })
    test("after", () => {})
    "#,
  );
  assert_eq!(results.len(), 3);
  let message = |i: usize| results[i].error.as_ref().map(|e| e.message.clone()).unwrap_or_default();
  assert!(!results[0].ok && message(0).contains("from the timer"), "got {:?}", results[0]);
  assert!(!results[1].ok && message(1).contains("nobody handles this"), "got {:?}", results[1]);
  assert!(results[2].ok, "got {:?}", results[2]);
}

#[test]
fn a_file_that_fails_to_load_runs_no_test() {
  let records = records_with(
    r#"
    import { test } from "flux:test"
    test("never runs", () => {})
    throw new Error("at load")
    "#,
    RunOptions::default(),
  );
  assert_eq!(records.len(), 1);
  let Record::Failed { message, output } = &records[0] else { panic!("expected a failure, got {:?}", records[0]) };
  assert!(message.contains("at load"), "got {message}");
  assert_eq!(output.len(), 1, "the error is reported once, got {output:?}");
}

// A failed load is reported when it fails, not when the cap passes: what
// the file left pending keeps its engine alive, and the host does not wait
// for that.
#[test]
fn a_failed_load_is_reported_at_once_whatever_the_file_left_running() {
  let started = std::time::Instant::now();
  let records = records_with(
    r#"
    import { test } from "flux:test"
    setInterval(() => {}, 1000)
    test("never runs", () => {})
    await Promise.reject(new Error("rejected at load"))
    "#,
    RunOptions { timeout: Duration::from_secs(30), ..RunOptions::default() },
  );
  assert!(started.elapsed() < Duration::from_secs(5), "the host waited for the cap");
  let Record::Failed { message, .. } = &records[0] else { panic!("expected a failure, got {:?}", records[0]) };
  assert!(message.contains("rejected at load"), "got {message}");
}

// Registered tests only run under a host; elsewhere the file would end
// cleanly with nothing run.
#[test]
fn the_module_refuses_to_load_without_a_host() {
  let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let sink = lines.clone();
  let engine = FluxEngine::builder().logger(move |_, msg| sink.lock().expect("log lock").push(msg.to_string())).build();
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .expect("tokio runtime")
    .block_on(engine.eval_source(r#"import { test } from "flux:test"; test("x", () => {})"#));
  let lines = lines.lock().expect("log lock").join("\n");
  assert!(lines.contains("a test file is run by a test host"), "got {lines}");
}

#[test]
fn registration_rejects_bad_input() {
  let report = logged(
    r#"
    import { test } from "flux:test"
    let thrown = (fn) => { try { fn() } catch (e) { return e.message } return "no throw" }
    let atLoad = [
      thrown(() => test("", () => {})),
      thrown(() => test("no function")),
    ]
    test("first", () => {
      console.log([...atLoad, thrown(() => test("late", () => {}))].join("\n"))
    })
    atLoad.push(thrown(() => test("first", () => {})))
    "#,
  );
  assert_eq!(
    report,
    [
      "test: the name must be a non-empty string",
      "test: \"no function\" needs a function",
      "test: \"first\" is registered twice",
      "test: \"late\" is registered while a test runs; register tests at the top level",
    ]
    .join("\n")
  );
}

/// Evaluate a table of `[label, () => expectation, holds]` rows inside a
/// test and return the labels whose expectation did not behave as `holds`
/// says.
fn wrong_rows(rows: &str) -> String {
  logged(&format!(
    r#"
    import {{ test, expect }} from "flux:test"
    test("rows", () => {{
      let rows = [{rows}]
      let wrong = []
      for (let [label, check, holds] of rows) {{
        let held = true
        try {{ check() }} catch (e) {{ held = false }}
        if (held !== holds) wrong.push(label)
      }}
      console.log(wrong.join(", "))
    }})
    "#
  ))
}

#[test]
fn to_be_is_object_is() {
  assert_eq!(
    wrong_rows(
      r#"
      ["same number", () => expect(1).toBe(1), true],
      ["nan is nan", () => expect(NaN).toBe(NaN), true],
      ["zero is not negative zero", () => expect(0).toBe(-0), false],
      ["equal objects are not the same", () => expect({}).toBe({}), false],
      ["not", () => expect(1).not.toBe(2), true],
      ["not fails on equal", () => expect(1).not.toBe(1), false],
      "#
    ),
    ""
  );
}

#[test]
fn to_equal_is_recursive_and_skips_undefined_properties() {
  assert_eq!(
    wrong_rows(
      r#"
      ["nested", () => expect({ a: [1, { b: 2 }] }).toEqual({ a: [1, { b: 2 }] }), true],
      ["nested differs", () => expect({ a: [1, { b: 2 }] }).toEqual({ a: [1, { b: 3 }] }), false],
      ["undefined property skipped", () => expect({ a: 1, b: undefined }).toEqual({ a: 1 }), true],
      ["missing property", () => expect({ a: 1 }).toEqual({ a: 1, b: 2 }), false],
      ["zero from negative zero", () => expect([0]).toEqual([-0]), false],
      ["nan", () => expect([NaN]).toEqual([NaN]), true],
      ["array length", () => expect([1, 2]).toEqual([1, 2, 3]), false],
      ["typed array", () => expect(new Float32Array([1, 2])).toEqual(new Float32Array([1, 2])), true],
      ["typed array differs", () => expect(new Float32Array([1, 2])).toEqual(new Float32Array([1, 3])), false],
      ["typed array is no array", () => expect(new Uint8Array([1])).toEqual([1]), false],
      ["typed array kinds differ", () => expect(new Uint8Array([1])).toEqual(new Int8Array([1])), false],
      ["array buffer bytes", () => expect(new Uint8Array([1, 2]).buffer).toEqual(new Uint8Array([1, 2]).buffer), true],
      ["array buffer bytes differ", () => expect(new Uint8Array([1, 2]).buffer).toEqual(new Uint8Array([1, 3]).buffer), false],
      ["map", () => expect(new Map([["a", { x: 1 }]])).toEqual(new Map([["a", { x: 1 }]])), true],
      ["map differs", () => expect(new Map([["a", 1]])).toEqual(new Map([["a", 2]])), false],
      ["set", () => expect(new Set([{ x: 1 }])).toEqual(new Set([{ x: 1 }])), true],
      ["date", () => expect(new Date(5)).toEqual(new Date(5)), true],
      ["date differs", () => expect(new Date(5)).toEqual(new Date(6)), false],
      ["error by message", () => expect(new Error("a")).toEqual(new Error("b")), false],
      ["null is no object", () => expect(null).toEqual({}), false],
      ["cycle", () => { let a = { n: 1 }; a.self = a; let b = { n: 1 }; b.self = b; expect(a).toEqual(b) }, true],
      ["not", () => expect({ a: 1 }).not.toEqual({ a: 2 }), true],
      "#
    ),
    ""
  );
}

#[test]
fn to_be_close_to_passes_within_half_a_unit_of_the_last_digit() {
  assert_eq!(
    wrong_rows(
      r#"
      ["float sum", () => expect(0.1 + 0.2).toBeCloseTo(0.3), true],
      ["default two digits", () => expect(1.004).toBeCloseTo(1), true],
      ["outside two digits", () => expect(1.006).toBeCloseTo(1), false],
      ["five digits", () => expect(1.004).toBeCloseTo(1, 5), false],
      ["zero digits", () => expect(1.4).toBeCloseTo(1, 0), true],
      ["infinity", () => expect(Infinity).toBeCloseTo(Infinity), true],
      ["nan", () => expect(NaN).toBeCloseTo(1), false],
      ["not a number", () => expect("1").toBeCloseTo(1), false],
      "#
    ),
    ""
  );
}

#[test]
fn to_throw_takes_a_message_part_a_pattern_a_class_or_nothing() {
  assert_eq!(
    wrong_rows(
      r#"
      ["any throw", () => expect(() => { throw new Error("boom") }).toThrow(), true],
      ["no throw", () => expect(() => {}).toThrow(), false],
      ["message part", () => expect(() => { throw new Error("a big boom") }).toThrow("big"), true],
      ["other message", () => expect(() => { throw new Error("a big boom") }).toThrow("small"), false],
      ["pattern", () => expect(() => { throw new Error("a big boom") }).toThrow(/b.g/), true],
      ["other pattern", () => expect(() => { throw new Error("a big boom") }).toThrow(/^big/), false],
      ["class", () => expect(() => { throw new TypeError("t") }).toThrow(TypeError), true],
      ["base class", () => expect(() => { throw new TypeError("t") }).toThrow(Error), true],
      ["other class", () => expect(() => { throw new TypeError("t") }).toThrow(RangeError), false],
      ["thrown string", () => expect(() => { throw "plain text" }).toThrow("plain"), true],
      ["not", () => expect(() => {}).not.toThrow(), true],
      ["not fails on a throw", () => expect(() => { throw new Error("x") }).not.toThrow(), false],
      ["not a function", () => expect(1).toThrow(), false],
      "#
    ),
    ""
  );
}

#[test]
fn rejects_applies_the_matchers_to_the_rejection_reason() {
  // `expect(promise).rejects`: the matcher runs on the reason once the
  // promise rejected; a promise that fulfills fails it, `not` or no `not`;
  // `toThrow` sees the reason as the thrown value.
  assert_eq!(
    logged(
      r#"
    import { test, expect } from "flux:test"
    test("rows", async () => {
      let rows = [
        ["reason", () => expect(Promise.reject(new Error("boom"))).rejects.toThrow("boom"), true],
        ["other reason", () => expect(Promise.reject(new Error("boom"))).rejects.toThrow("bang"), false],
        ["class", () => expect(Promise.reject(new TypeError("t"))).rejects.toThrow(TypeError), true],
        ["plain reason", () => expect(Promise.reject(7)).rejects.toBe(7), true],
        ["object reason", () => expect(Promise.reject({ code: 4 })).rejects.toMatchObject({ code: 4 }), true],
        ["fulfilled", () => expect(Promise.resolve(1)).rejects.toThrow(), false],
        ["fulfilled with not", () => expect(Promise.resolve(1)).rejects.not.toThrow(), false],
        ["not", () => expect(Promise.reject(new Error("boom"))).rejects.not.toThrow("bang"), true],
        ["not before rejects", () => expect(Promise.reject(7)).not.rejects.toBe(8), true],
        ["not a promise", () => expect(1).rejects.toThrow(), false],
      ]
      let wrong = []
      for (let [label, check, holds] of rows) {
        let held = true
        try { await check() } catch (e) { held = false }
        if (held !== holds) wrong.push(label)
      }
      console.log(wrong.join(", "))
    })
    "#
    ),
    ""
  );
}

#[test]
fn a_rejects_failure_names_the_chain_and_the_promise_outcome() {
  let report = logged(
    r#"
    import { test, expect } from "flux:test"
    let message = async (fn) => { try { await fn() } catch (e) { return e.message } return "no throw" }
    test("messages", async () => {
      console.log([
        await message(() => expect(Promise.resolve(1)).rejects.toThrow()),
        await message(() => expect(Promise.reject(7)).rejects.toBe(8)),
      ].join("\n--\n"))
    })
    "#,
  );
  assert_eq!(
    report,
    [
      "expect(received).rejects.toThrow()\nExpected: a rejected promise\nReceived: a promise fulfilled with 1",
      "expect(received).rejects.toBe(expected)\nExpected: 8\nReceived: 7",
    ]
    .join("\n--\n")
  );
}

#[test]
fn the_remaining_matchers() {
  assert_eq!(
    wrong_rows(
      r#"
      ["null", () => expect(null).toBeNull(), true],
      ["undefined is not null", () => expect(undefined).toBeNull(), false],
      ["not null", () => expect(0).not.toBeNull(), true],
      ["array contains", () => expect([1, 2]).toContain(2), true],
      ["array lacks", () => expect([1, 2]).toContain(3), false],
      ["contains by identity", () => expect([{}]).toContain({}), false],
      ["string contains", () => expect("hello").toContain("ell"), true],
      ["string lacks", () => expect("hello").not.toContain("x"), true],
      ["set contains", () => expect(new Set([1])).toContain(1), true],
      ["number contains nothing", () => expect(5).toContain(5), false],
      ["match subset", () => expect({ a: 1, b: { c: 2, d: 3 } }).toMatchObject({ b: { c: 2 } }), true],
      ["match differs", () => expect({ a: 1, b: { c: 2 } }).toMatchObject({ b: { c: 3 } }), false],
      ["match missing key", () => expect({ a: 1 }).toMatchObject({ b: undefined }), false],
      ["match array in full", () => expect({ a: [{ x: 1, y: 2 }] }).toMatchObject({ a: [{ x: 1 }] }), true],
      ["match array length", () => expect({ a: [1, 2] }).toMatchObject({ a: [1] }), false],
      ["less", () => expect(1).toBeLessThan(2), true],
      ["not less", () => expect(2).toBeLessThan(2), false],
      ["less or equal", () => expect(2).toBeLessThanOrEqual(2), true],
      ["greater", () => expect(3).toBeGreaterThan(2), true],
      ["not greater", () => expect(2).toBeGreaterThan(2), false],
      ["greater or equal", () => expect(2).toBeGreaterThanOrEqual(2), true],
      ["ordering needs numbers", () => expect("1").toBeLessThan(2), false],
      "#
    ),
    ""
  );
}

#[test]
fn a_failure_message_prints_both_values() {
  let report = logged(
    r#"
    import { test, expect } from "flux:test"
    let message = (fn) => { try { fn() } catch (e) { return e.message } return "no throw" }
    test("messages", () => {
      console.log([
        message(() => expect({ a: [1, -0], b: "x" }).toEqual({ a: new Float32Array([1]) })),
        message(() => expect(() => {}).toThrow("boom")),
        message(() => expect(null).not.toBeNull()),
      ].join("\n--\n"))
    })
    "#,
  );
  assert_eq!(
    report,
    [
      "expect(received).toEqual(expected)\nExpected: {a: Float32Array [1]}\nReceived: {a: [1, -0], b: \"x\"}",
      "expect(received).toThrow(expected)\nExpected: an error whose message contains \"boom\"\nReceived: the function did not throw",
      "expect(received).not.toBeNull()\nExpected: not null\nReceived: null",
    ]
    .join("\n--\n")
  );
}

/// What each test of `source` logged, one entry per test.
fn logged_each(source: &str, options: RunOptions) -> Vec<String> {
  results_with(source, options).into_iter().map(|result| result.output.join("\n")).collect()
}

const DRAWS: &str = r#"
  import { test } from "flux:test"
  let atLoad = Math.random()
  test("first", () => { console.log([atLoad, Math.random()].join()) })
  test("second", () => { console.log([atLoad, Math.random()].join()) })
"#;

// Math.random is seeded in every engine of a run, so a test draws the same
// values whatever ran before it and whatever the filter leaves out, the
// file's own draws at load included.
#[test]
fn every_test_draws_the_seeds_sequence_from_its_start() {
  let full = logged_each(DRAWS, RunOptions::default());
  assert_eq!(full.len(), 2);
  assert_eq!(full[0], full[1]);
  let filtered = logged_each(DRAWS, RunOptions { filter: Some("second".into()), ..RunOptions::default() });
  assert_eq!(filtered, vec![full[1].clone()]);
  let other = logged_each(DRAWS, RunOptions { seed: 7, ..RunOptions::default() });
  assert_ne!(other[0], full[0]);
}

// The default seed and an explicit one name the generator's pinned
// sequences (tests/random.rs): what a test draws is the same on every run.
#[test]
fn the_seed_option_picks_the_sequence() {
  let source = r#"
    import { test } from "flux:test"
    test("draws", () => { console.log(Math.random()) })
  "#;
  let draw = |seed: u64| logged_each(source, RunOptions { seed, ..RunOptions::default() }).join("");
  assert_eq!(draw(0), "0.4833481342839381");
  assert_eq!(draw(12345), "0.28097516969868397");
}

// settle() ends when nothing the test started is in flight and what the
// finished work woke has run: here a write nobody awaits, whose
// continuation hops through the job queue and starts a second write.
#[test]
fn settle_waits_for_work_in_flight_and_what_it_starts() {
  let path = std::env::temp_dir().join(format!("flux-settle-{}.txt", std::process::id()));
  let source = r#"
    import { test, expect, settle } from "flux:test"
    import { file } from "flux:fs"
    test("settles", async () => {
      let target = file(PATH)
      let done = false
      ;(async () => {
        await target.write("one")
        await Promise.resolve()
        await Promise.resolve()
        await target.write("two")
        done = true
      })()
      await settle()
      expect(done).toBe(true)
      expect(await target.text()).toBe("two")
    })
  "#
  .replace("PATH", &format!("{:?}", path.to_string_lossy()));
  let results = results(&source);
  let _ = std::fs::remove_file(&path);
  assert_eq!(rows(&results), "settles|true|");
}

// What stands is not waited for: a timer far in the future and an event
// listener keep the engine alive, and settle() still ends at once.
#[test]
fn settle_does_not_wait_for_what_stands() {
  let results = results(
    r#"
    import { test, expect, settle } from "flux:test"
    test("settles", async () => {
      let fired = false
      let timer = setTimeout(() => { fired = true }, 60000)
      await settle()
      await settle()
      expect(fired).toBe(false)
      clearTimeout(timer)
    })
    "#,
  );
  assert_eq!(rows(&results), "settles|true|");
}

// A failed test's record carries the details read in its engine when the
// failure was known: what flux has in flight, then the embedder's own,
// under labels. A pass carries none.
#[test]
fn a_failure_carries_the_details_read_in_its_engine() {
  let hook: crate::test::DetailsHook =
    std::sync::Arc::new(|_ctx, name| vec![("At".to_string(), format!("the end of {name:?}"))]);
  let results = results_with(
    r#"
    import { test, expect } from "flux:test"
    test("passes", () => {})
    test("fails", () => { expect(1).toBe(2) })
    test("rejects unhandled", () => { Promise.reject(new Error("nobody handles this")) })
    "#,
    RunOptions { details: Some(hook), ..RunOptions::default() },
  );
  assert_eq!(results.len(), 3);
  assert_eq!(results[0].details, Vec::<(String, String)>::new());
  assert_eq!(results[1].details, vec![("At".to_string(), "the end of \"fails\"".to_string())]);
  assert_eq!(results[2].details, vec![("At".to_string(), "the end of \"rejects unhandled\"".to_string())]);
}

// A test that ran out of time has its details read all the same, in the
// interrupted engine: here a fetch nobody answers, which is what kept it.
#[test]
fn a_timed_out_test_says_what_is_in_flight() {
  let hook: crate::test::DetailsHook =
    std::sync::Arc::new(|_ctx, _name| vec![("At".to_string(), "the cap".to_string())]);
  let port = {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    listener.local_addr().expect("the bound address").port()
  };
  let source = r#"
    import { test } from "flux:test"
    import { serve } from "flux:http"
    test("waits on a server that never answers", async () => {
      serve({ port: PORT, fetch: () => new Promise(() => {}) })
      await fetch("http://127.0.0.1:PORT/")
    })
    "#
  .replace("PORT", &port.to_string());
  let results = results_with(
    &source,
    RunOptions { timeout: Duration::from_millis(200), details: Some(hook), ..RunOptions::default() },
  );
  assert_eq!(rows(&results), "waits on a server that never answers|false|Timed out after 200 ms");
  let in_flight = results[0].details.iter().find(|(label, _)| label == "In flight").map(|(_, text)| text.as_str());
  assert!(matches!(in_flight, Some(text) if text.contains("fetch")), "got {:?}", results[0].details);
  assert!(results[0].details.contains(&("At".to_string(), "the cap".to_string())), "got {:?}", results[0].details);
}
