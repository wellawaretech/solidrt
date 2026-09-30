// flux:test driven through the engine: a script registers tests, iterates
// `run` and logs what came out, and the assertions here read that log. The
// module's JS half (test_plugins/test.js) has no other harness to stand on,
// so its matchers are checked from outside, as plain pass/throw tables.

use std::sync::{Arc, Mutex};

use crate::FluxEngine;

/// Run `source` as the entry module to the end of its event loop and return
/// every line it logged.
fn run_script(source: &str) -> Vec<String> {
  let lines = Arc::new(Mutex::new(Vec::new()));
  let sink = lines.clone();
  let engine = FluxEngine::builder()
    .logger(move |_, msg| sink.lock().expect("log lock").push(msg.to_string()))
    .build();
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .expect("tokio runtime")
    .block_on(engine.eval_source(source));
  let lines = lines.lock().expect("log lock").clone();
  lines
}

/// The one line a script logs through `report`, which these scripts end on.
fn report_of(source: &str) -> String {
  let lines = run_script(source);
  assert_eq!(lines.len(), 1, "expected one report line, got {lines:?}");
  lines.into_iter().next().expect("one line")
}

const COLLECT: &str = r#"
  async function collect(options) {
    let results = []
    for await (let result of run(options)) results.push(result)
    return results
  }
"#;

#[test]
fn results_come_in_registration_order_with_their_errors() {
  let report = report_of(&format!(
    r#"
    import {{ test, expect, run }} from "flux:test"
    {COLLECT}
    test("passes", () => {{ expect(1 + 1).toBe(2) }})
    test("fails an expectation", () => {{ expect(1 + 1).toBe(3) }})
    test("throws", () => {{ throw new RangeError("out of range") }})
    test("awaits", async () => {{ await Promise.resolve(); expect("a").toBe("a") }})
    test("throws a non-error", () => {{ throw "plain" }})
    let results = await collect()
    console.log(results.map((r) => [r.name, r.ok, typeof r.durationMs, r.error?.message ?? null].join("|")).join("\n"))
    "#
  ));
  let rows: Vec<&str> = report.split('\n').collect();
  assert_eq!(rows[0], "passes|true|number|");
  assert_eq!(rows[1], "fails an expectation|false|number|AssertionError: expect(received).toBe(expected)");
  assert_eq!(rows[2], "Expected: 3");
  assert_eq!(rows[3], "Received: 2");
  assert_eq!(rows[4], "throws|false|number|RangeError: out of range");
  assert_eq!(rows[5], "awaits|true|number|");
  assert_eq!(rows[6], "throws a non-error|false|number|Thrown: \"plain\"");
}

#[test]
fn a_failure_stack_cites_the_test_and_the_harness_by_name() {
  let report = report_of(&format!(
    r#"
    import {{ test, expect, run }} from "flux:test"
    {COLLECT}
    test("fails", () => {{ expect(1).toBe(2) }})
    let [result] = await collect()
    console.log(JSON.stringify([result.error.stack.includes("(main:"), result.error.stack.includes("(flux:test:")]))
    "#
  ));
  assert_eq!(report, "[true,true]");
}

#[test]
fn the_filter_selects_tests_by_a_part_of_their_name() {
  let report = report_of(&format!(
    r#"
    import {{ test, run }} from "flux:test"
    {COLLECT}
    test("matchPath: a literal", () => {{}})
    test("formatPath: a param", () => {{}})
    test("matchPath: a param", () => {{}})
    let results = await collect({{ filter: "matchPath" }})
    console.log(results.map((r) => r.name).join(","))
    "#
  ));
  assert_eq!(report, "matchPath: a literal,matchPath: a param");
}

// A promise that never settles holds nothing in the engine, so without the
// cap the process would end cleanly with the test unreported. The cap makes
// it a failed result, and the run goes on to the next test.
#[test]
fn a_test_that_never_finishes_times_out_and_the_run_continues() {
  let report = report_of(&format!(
    r#"
    import {{ test, run }} from "flux:test"
    {COLLECT}
    test("hangs", () => new Promise(() => {{}}))
    test("after", () => {{}})
    let results = await collect({{ timeoutMs: 20 }})
    console.log(results.map((r) => [r.name, r.ok, r.error?.message ?? null].join("|")).join(","))
    "#
  ));
  assert_eq!(report, "hangs|false|Timed out after 20 ms,after|true|");
}

#[test]
fn registration_and_run_reject_bad_input() {
  let report = report_of(
    r#"
    import { test, run } from "flux:test"
    let thrown = (fn) => { try { fn() } catch (e) { return e.message } return "no throw" }
    test("first", () => {})
    let inside
    test("registers inside", () => { inside = thrown(() => test("late", () => {})) })
    let out = [
      thrown(() => test("first", () => {})),
      thrown(() => test("", () => {})),
      thrown(() => test("no function")),
      thrown(() => run({ filter: 1 })),
      thrown(() => run({ timeoutMs: 0 })),
      thrown(() => run(null)),
    ]
    for await (let result of run()) out.push(String(result.ok))
    out.push(inside)
    console.log(out.join("\n"))
    "#,
  );
  assert_eq!(
    report,
    [
      "test: \"first\" is registered twice",
      "test: the name must be a non-empty string",
      "test: \"no function\" needs a function",
      "run: filter must be a string",
      "run: timeoutMs must be a positive number",
      "run: the options must be an object",
      "true",
      "true",
      "test: \"late\" is registered while tests run; register tests at the top level",
    ]
    .join("\n")
  );
}

/// Evaluate a table of `[label, () => expectation, holds]` rows and return
/// the labels whose expectation did not behave as `holds` says.
fn wrong_rows(rows: &str) -> String {
  report_of(&format!(
    r#"
    import {{ expect }} from "flux:test"
    let rows = [{rows}]
    let wrong = []
    for (let [label, check, holds] of rows) {{
      let held = true
      try {{ check() }} catch (e) {{ held = false }}
      if (held !== holds) wrong.push(label)
    }}
    console.log(wrong.join(", "))
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
  let report = report_of(
    r#"
    import { expect } from "flux:test"
    let message = (fn) => { try { fn() } catch (e) { return e.message } return "no throw" }
    console.log([
      message(() => expect({ a: [1, -0], b: "x" }).toEqual({ a: new Float32Array([1]) })),
      message(() => expect(() => {}).toThrow("boom")),
      message(() => expect(null).not.toBeNull()),
    ].join("\n--\n"))
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

// A test that takes the clock runs stepped: nothing fires until it says
// so, `advance` stops at every deadline on the way (an interval fires as
// often as fits, each callback at the time it was set for), and what one
// timer starts - here a promise chain several hops deep - has run before
// the next timer fires.
#[test]
fn a_test_that_takes_the_clock_steps_its_timers() {
  let report = report_of(&format!(
    r#"
    import {{ test, expect, run }} from "flux:test"
    {COLLECT}
    let events = []
    test("stepped", async (clock) => {{
      setTimeout(() => {{
        events.push("a@" + clock.now)
        Promise.resolve().then(() => {{}}).then(() => {{}}).then(() => events.push("a-chain"))
      }}, 100)
      setTimeout(() => events.push("b@" + clock.now), 100)
      setTimeout(() => events.push("c@" + clock.now), 250)
      setInterval(() => events.push("i@" + clock.now), 90)
      events.push("start@" + clock.now)
      await clock.advance(99)
      events.push("idle@" + clock.now)
      await clock.advance(171)
      events.push("end@" + clock.now)
    }})
    let results = await collect()
    console.log([String(results[0].ok), ...events].join(" "))
    "#
  ));
  assert_eq!(report, "true start@0 i@90 idle@99 a@100 b@100 a-chain i@180 c@250 i@270 end@270");
}

// The clock belongs to the one test that took it: the next test is back on
// real time, a stepped test after that starts at 0 again, and a timer the
// first left waiting neither fires later nor keeps the process alive.
#[test]
fn the_clock_is_per_test() {
  let report = report_of(&format!(
    r#"
    import {{ test, expect, run }} from "flux:test"
    {COLLECT}
    let events = []
    test("stepped, leaves a timer", async (clock) => {{
      setTimeout(() => events.push("leftover"), 1000)
      await clock.advance(10)
      events.push("first@" + clock.now)
    }})
    test("real time", async () => {{
      await new Promise((resolve) => setTimeout(resolve, 5))
      events.push("real")
    }})
    test("stepped again", async (clock) => {{
      events.push("second@" + clock.now)
      await clock.advance(2000)
    }})
    let results = await collect()
    console.log([...results.map((r) => String(r.ok)), ...events].join(" "))
    "#
  ));
  assert_eq!(report, "true true true first@10 real second@0");
}

// The cap on a test rides the wall clock whatever the test does to time: a
// stepped test waiting on a timer it never advances to still times out.
#[test]
fn a_stepped_test_still_times_out_on_the_wall_clock() {
  let report = report_of(&format!(
    r#"
    import {{ test, run }} from "flux:test"
    {COLLECT}
    test("waits on a timer it never reaches", async (clock) => {{
      await new Promise((resolve) => setTimeout(resolve, 50))
    }})
    test("after", () => {{}})
    let results = await collect({{ timeoutMs: 20 }})
    console.log(results.map((r) => [r.name, r.ok, r.error?.message ?? null].join("|")).join(","))
    "#
  ));
  assert_eq!(report, "waits on a timer it never reaches|false|Timed out after 20 ms,after|true|");
}

#[test]
fn advance_rejects_bad_input_and_timers_that_hold_time_still() {
  let report = report_of(&format!(
    r#"
    import {{ test, run }} from "flux:test"
    {COLLECT}
    let message = async (fn) => {{ try {{ await fn() }} catch (e) {{ return e.message }} return "no throw" }}
    let out = []
    let kept
    test("bad input", async (clock) => {{
      kept = clock
      out.push(await message(() => clock.advance(-1)))
      out.push(await message(() => clock.advance("10")))
    }})
    test("re-arming", async (clock) => {{
      let again = () => setTimeout(again, 0)
      again()
      out.push(await message(() => clock.advance(5)))
    }})
    await collect()
    out.push(await message(() => kept.advance(1)))
    console.log(out.join("\n"))
    "#
  ));
  assert_eq!(
    report,
    [
      "clock.advance: the time must be a non-negative number of milliseconds, got -1",
      "clock.advance: the time must be a non-negative number of milliseconds, got \"10\"",
      "clock.advance: timers keep re-arming at 0 ms without letting time pass",
      "clock.advance: the test this clock belongs to has ended",
    ]
    .join("\n")
  );
}
