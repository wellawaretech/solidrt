// Optional arguments: bindings take `marshal::OptArg<T>` for optional params.
// JS wrappers forward their own optional parameter verbatim, so an explicit
// `undefined` (or `null`) must behave like an absent argument instead of
// failing conversion with "Error converting from js 'undefined' into type
// ..." (okf/done/binding-optional-arg-undefined.md).

use rquickjs::{CatchResultExt, Context, Ctx, Function, Object, Runtime};

use crate::plugins::marshal::OptArg;

fn with_probe_ctx(f: impl FnOnce(&Ctx<'_>)) {
  let rt = Runtime::new().expect("js runtime");
  let context = Context::full(&rt).expect("js context");
  context.with(|ctx| {
    let obj_probe = Function::new(ctx.clone(), |opts: OptArg<Object<'_>>| -> String {
      match opts.0 {
        Some(o) => format!("object:{}", o.get::<_, i32>("n").unwrap_or(-1)),
        None => String::from("none"),
      }
    })
    .expect("object probe");
    let num_probe = Function::new(ctx.clone(), |code: OptArg<u16>| -> String {
      match code.0 {
        Some(n) => format!("num:{n}"),
        None => String::from("none"),
      }
    })
    .expect("number probe");
    let globals = ctx.globals();
    globals.set("objProbe", obj_probe).expect("set objProbe");
    globals.set("numProbe", num_probe).expect("set numProbe");
    f(&ctx);
  });
}

#[test]
fn optional_arg_accepts_absent_undefined_and_null() {
  with_probe_ctx(|ctx| {
    let objects: String = ctx
      .eval(r#"[objProbe(), objProbe(undefined), objProbe(null), objProbe({n: 7})].join(",")"#)
      .expect("object probe calls");
    assert_eq!(objects, "none,none,none,object:7");
    let numbers: String = ctx
      .eval(r#"[numProbe(), numProbe(undefined), numProbe(null), numProbe(1000)].join(",")"#)
      .expect("number probe calls");
    assert_eq!(numbers, "none,none,none,num:1000");
  });
}

#[test]
fn optional_arg_still_rejects_wrong_types() {
  with_probe_ctx(|ctx| {
    let err = ctx.eval::<String, _>("objProbe(5)").catch(ctx);
    assert!(err.is_err(), "a non-object argument must fail conversion");
    let err = ctx.eval::<String, _>("numProbe({})").catch(ctx);
    assert!(err.is_err(), "a non-number argument must fail conversion");
  });
}
