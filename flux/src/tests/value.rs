// forge::Value <-> JS: the neutral-value contract documented in
// plugins/value.rs. `echo(v)` decodes a JS value and encodes it straight back,
// so a round trip observed from JS covers both directions; the Rust-side probes
// pin the exact `Value` shape where JS cannot tell (Int vs Float, Map order).

use rquickjs::{CatchResultExt, Context, Ctx, Function, Runtime, Value as JsValue};

use crate::plugins::value::{from_js, Neutral};
use forge::{Elem, Value};

fn with_ctx(f: impl FnOnce(&Ctx<'_>)) {
  let rt = Runtime::new().expect("js runtime");
  let context = Context::full(&rt).expect("js context");
  context.with(|ctx| {
    let echo = Function::new(ctx.clone(), |v: Neutral| -> Neutral { v }).expect("echo");
    ctx.globals().set("echo", echo).expect("set echo");
    f(&ctx);
  });
}

fn decode<'js>(ctx: &Ctx<'js>, src: &str) -> Value {
  let v: JsValue = ctx.eval(src).expect("eval");
  from_js(ctx, v).expect("decode")
}

fn decode_err<'js>(ctx: &Ctx<'js>, src: &str) -> String {
  let v: JsValue = ctx.eval(src).expect("eval");
  match from_js(ctx, v).catch(ctx) {
    Ok(_) => panic!("{src} must not decode"),
    Err(e) => e.to_string(),
  }
}

#[test]
fn scalars_decode_to_the_documented_variants() {
  with_ctx(|ctx| {
    assert_eq!(decode(ctx, "null"), Value::Null);
    assert_eq!(decode(ctx, "undefined"), Value::Null);
    assert_eq!(decode(ctx, "true"), Value::Bool(true));
    assert_eq!(decode(ctx, "42"), Value::Int(42));
    assert_eq!(decode(ctx, "2 ** 40"), Value::Int(1 << 40));
    assert_eq!(decode(ctx, "1.0"), Value::Int(1));
    assert_eq!(decode(ctx, "1.5"), Value::Float(1.5));
    assert_eq!(decode(ctx, "-0"), Value::Float(-0.0));
    assert_eq!(decode(ctx, "2 ** 53 + 2"), Value::Float(9007199254740994.0));
    assert!(matches!(decode(ctx, "NaN"), Value::Float(f) if f.is_nan()));
    assert_eq!(decode(ctx, "'hi'"), Value::String("hi".into()));
  });
}

#[test]
fn buffers_and_views_decode_to_bytes() {
  with_ctx(|ctx| {
    assert_eq!(decode(ctx, "new Uint8Array([1, 2, 3])"), Value::bytes(vec![1, 2, 3]));
    assert_eq!(decode(ctx, "new Uint8Array([1, 2, 3]).buffer"), Value::bytes(vec![1, 2, 3]));
    assert_eq!(decode(ctx, "new Uint8Array([1, 2, 3, 4]).subarray(1, 3)"), Value::bytes(vec![2, 3]));
    assert_eq!(decode(ctx, "new Uint16Array([258])"), Value::Bytes { elem: Elem::U16, data: 258u16.to_ne_bytes().to_vec() });
    assert_eq!(decode(ctx, "new Float64Array(1)"), Value::Bytes { elem: Elem::F64, data: vec![0; 8] });
  });
}

#[test]
fn containers_decode_in_order_with_holes_as_null() {
  with_ctx(|ctx| {
    assert_eq!(
      decode(ctx, "[1, , 'x', [true]]"),
      Value::List(vec![Value::Int(1), Value::Null, Value::String("x".into()), Value::List(vec![Value::Bool(true)])])
    );
    assert_eq!(
      decode(ctx, "({ b: 1, a: undefined, c: { d: null } })"),
      Value::Map(vec![
        ("b".into(), Value::Int(1)),
        ("a".into(), Value::Null),
        ("c".into(), Value::Map(vec![("d".into(), Value::Null)])),
      ])
    );
    assert_eq!(decode(ctx, "Object.create(null)"), Value::Map(vec![]));
    let sym_key = decode(ctx, "({ [Symbol('s')]: 1, k: 2 })");
    assert_eq!(sym_key, Value::Map(vec![("k".into(), Value::Int(2))]));
  });
}

#[test]
fn unsupported_values_throw_type_errors() {
  with_ctx(|ctx| {
    for src in [
      "() => 1",
      "Symbol('s')",
      "10n",
      "new Date(0)",
      "new Map()",
      "new (class A {})()",
      "/x/",
      "new DataView(new ArrayBuffer(1))",
    ] {
      let msg = decode_err(ctx, src);
      assert!(msg.contains("cannot be sent"), "{src}: {msg}");
    }
    let cyclic = decode_err(ctx, "(() => { let a = []; a.push(a); return a; })()");
    assert!(cyclic.contains("too deeply nested"), "{cyclic}");
    let deep = decode_err(ctx, "(() => { let v = 0; for (let i = 0; i < 300; i++) v = [v]; return v; })()");
    assert!(deep.contains("too deeply nested"), "{deep}");
  });
}

#[test]
fn round_trip_through_js_preserves_shape() {
  with_ctx(|ctx| {
    let out: String = ctx
      .eval(
        r#"
        let v = echo({ n: 1.5, i: 7, s: "x", b: true, z: null, u: undefined, l: [1, [2]], bytes: new Uint8Array([9, 8]) });
        [
          JSON.stringify(v),
          Object.keys(v).join(","),
          v.bytes instanceof Uint8Array,
          echo(new Uint8Array([1, 2]).buffer) instanceof Uint8Array,
          echo(new Float32Array([1.5, -2])) instanceof Float32Array,
          echo(new Float32Array([1.5, -2])).join(","),
          echo(new BigInt64Array([-5n])) instanceof BigInt64Array,
          echo(new Int16Array([1, 2, 3, 4]).subarray(1, 3)).join(","),
          Object.getPrototypeOf(echo({})) === Object.prototype,
          Array.isArray(echo([])),
        ].join("|")
        "#,
      )
      .expect("round trip");
    assert_eq!(
      out,
      r#"{"n":1.5,"i":7,"s":"x","b":true,"z":null,"u":null,"l":[1,[2]],"bytes":{"0":9,"1":8}}|n,i,s,b,z,u,l,bytes|true|true|true|1.5,-2|true|2,3|true|true"#
    );
  });
}
