use rquickjs::{function::MutFn, promise::Promised, Ctx, Function, Object, TypedArray, Value};

use crate::pending::PendingOps;

pub(crate) fn init_write<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  let write_fn = Function::new(
    ctx.clone(),
    MutFn::from(
      |ctx: Ctx<'_>, path: String, data: Value<'_>| -> rquickjs::Result<Promised<_>> {
        let bytes = if let Some(s) = data.as_string() {
          s.to_string()?.into_bytes()
        } else if let Ok(ta) = TypedArray::<u8>::from_value(data.clone()) {
          ta.as_bytes()
            .map(|b| b.to_vec())
            .unwrap_or_default()
        } else {
          return Err(ctx.throw(
            rquickjs::String::from_str(
              ctx.clone(),
              "Flux.write: data must be string or Uint8Array",
            )
            .expect("create error string")
            .into(),
          ));
        };
        let pending = ctx
          .userdata::<PendingOps>()
          .expect("pending ops")
          .clone();
        Ok(Promised(async move {
          pending.hold();
          let r = tokio::fs::write(&path, &bytes)
            .await
            .map_err(rquickjs::Error::Io);
          pending.release();
          r
        }))
      },
    ),
  )
  .expect("create Flux.write function");
  flux.set("write", write_fn).expect("set Flux.write");
}