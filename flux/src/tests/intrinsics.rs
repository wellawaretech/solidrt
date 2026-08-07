// Engine-provided web globals: quickjs-ng (via rquickjs's Context::full)
// installs atob/btoa (with DOMException), queueMicrotask, and performance
// natively, and flux relies on them instead of shipping its own. These tests
// probe a bare context with NO flux plugins installed: if a future rquickjs
// bump loses one of these, they fail loudly and the flux standards layer has
// to fill the gap again.

use rquickjs::{Context, Ctx, Runtime};

fn with_bare_ctx(f: impl FnOnce(&Ctx<'_>)) {
  let rt = Runtime::new().expect("js runtime");
  let context = Context::full(&rt).expect("js context");
  context.with(|ctx| f(&ctx));
}

#[test]
fn engine_provides_web_globals() {
  with_bare_ctx(|ctx| {
    let probe: String = ctx
      .eval(
        r#"[
          typeof atob,
          typeof btoa,
          typeof DOMException,
          typeof queueMicrotask,
          typeof performance.now,
          typeof performance.timeOrigin,
        ].join(",")"#,
      )
      .expect("probe globals");
    assert_eq!(probe, "function,function,function,function,function,number");
  });
}

#[test]
fn engine_base64_follows_whatwg() {
  with_bare_ctx(|ctx| {
    // Round-trip through the full byte range.
    let round: bool = ctx
      .eval(r#"atob(btoa("\x00a\xff")) === "\x00a\xff""#)
      .expect("round-trip");
    assert!(round);
    // Forgiving decode: unpadded input is accepted.
    let unpadded: String = ctx.eval(r#"atob("YQ")"#).expect("unpadded decode");
    assert_eq!(unpadded, "a");
    // Invalid input throws an InvalidCharacterError DOMException.
    let err: String = ctx
      .eval(
        r#"(() => {
          try { atob("a!"); return "no throw"; }
          catch (e) { return e.name + ":" + (e instanceof DOMException); }
        })()"#,
      )
      .expect("invalid decode probe");
    assert_eq!(err, "InvalidCharacterError:true");
    // btoa rejects input outside the Latin1 range.
    let wide: String = ctx
      .eval(
        r#"(() => {
          try { btoa("\u{1F600}"); return "no throw"; }
          catch (e) { return e.name; }
        })()"#,
      )
      .expect("wide btoa probe");
    assert_eq!(wide, "InvalidCharacterError");
  });
}
