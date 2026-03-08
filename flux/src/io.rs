use rquickjs::{
    function::MutFn,
    promise::Promised,
    AsyncContext, Ctx, Function, IntoJs, JsLifetime, Object, TypedArray, Value,
};
use std::cell::Cell;
use std::io;
use std::rc::Rc;

use crate::engine::PendingOps;

const USER_AGENT: &str = concat!("qjsrt/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, JsLifetime)]
struct HttpClient(#[qjs(skip_trace)] Rc<reqwest::Client>);

fn reqwest_err(e: reqwest::Error) -> rquickjs::Error {
    rquickjs::Error::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
}

fn http_client() -> HttpClient {
    HttpClient(Rc::new(
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap(),
    ))
}

struct JsBytes(Vec<u8>);

impl<'js> IntoJs<'js> for JsBytes {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        TypedArray::<u8>::new(ctx.clone(), self.0).map(|ta| ta.into_value())
    }
}

struct JsonValue(String);

impl<'js> IntoJs<'js> for JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        ctx.json_parse(self.0)
    }
}

fn throw_consumed(ctx: &Ctx<'_>) -> rquickjs::Error {
    ctx.throw(
        rquickjs::String::from_str(ctx.clone(), "Body already consumed")
            .unwrap()
            .into(),
    )
}

fn io_source<'js>(ctx: Ctx<'js>, target: String) -> rquickjs::Result<Value<'js>> {
    if target.starts_with("http://") || target.starts_with("https://") {
        let client = ctx.userdata::<HttpClient>().unwrap().0.clone();
        create_http_source(ctx, target, client)
    } else {
        create_file_source(ctx, target)
    }
}

fn create_file_source<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Value<'js>> {
    let consumed = Rc::new(Cell::new(false));
    let path = Rc::new(path);

    let text_fn = Function::new(
        ctx.clone(),
        MutFn::from({
            let consumed = consumed.clone();
            let path = path.clone();
            move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
                if consumed.get() {
                    return Err(throw_consumed(&ctx));
                }
                consumed.set(true);
                let pending = ctx.userdata::<PendingOps>().unwrap().clone();
                let path = path.clone();
                Ok(Promised(async move {
                    pending.hold();
                    let r = tokio::fs::read_to_string(&*path)
                        .await
                        .map_err(rquickjs::Error::Io);
                    pending.release();
                    r
                }))
            }
        }),
    )
    .unwrap();

    let bytes_fn = Function::new(
        ctx.clone(),
        MutFn::from({
            let consumed = consumed.clone();
            let path = path.clone();
            move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
                if consumed.get() {
                    return Err(throw_consumed(&ctx));
                }
                consumed.set(true);
                let pending = ctx.userdata::<PendingOps>().unwrap().clone();
                let path = path.clone();
                Ok(Promised(async move {
                    pending.hold();
                    let r = tokio::fs::read(&*path)
                        .await
                        .map(JsBytes)
                        .map_err(rquickjs::Error::Io);
                    pending.release();
                    r
                }))
            }
        }),
    )
    .unwrap();

    let json_fn = Function::new(
        ctx.clone(),
        MutFn::from({
            let consumed = consumed.clone();
            let path = path.clone();
            move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
                if consumed.get() {
                    return Err(throw_consumed(&ctx));
                }
                consumed.set(true);
                let pending = ctx.userdata::<PendingOps>().unwrap().clone();
                let path = path.clone();
                Ok(Promised(async move {
                    pending.hold();
                    let r = tokio::fs::read_to_string(&*path)
                        .await
                        .map(JsonValue)
                        .map_err(rquickjs::Error::Io);
                    pending.release();
                    r
                }))
            }
        }),
    )
    .unwrap();

    let obj = Object::new(ctx.clone())?;
    obj.set("path", path.as_ref().clone())?;
    obj.set("text", text_fn)?;
    obj.set("bytes", bytes_fn)?;
    obj.set("json", json_fn)?;

    Ok(obj.into_value())
}

fn create_http_source<'js>(
    ctx: Ctx<'js>,
    url: String,
    client: Rc<reqwest::Client>,
) -> rquickjs::Result<Value<'js>> {
    let consumed = Rc::new(Cell::new(false));
    let url = Rc::new(url);

    let text_fn = Function::new(
        ctx.clone(),
        MutFn::from({
            let consumed = consumed.clone();
            let url = url.clone();
            let client = client.clone();
            move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
                if consumed.get() {
                    return Err(throw_consumed(&ctx));
                }
                consumed.set(true);
                let pending = ctx.userdata::<PendingOps>().unwrap().clone();
                let url = url.clone();
                let client = client.clone();
                Ok(Promised(async move {
                    pending.hold();
                    let r = client
                        .get(url.as_ref())
                        .send()
                        .await
                        .map_err(reqwest_err)?
                        .text()
                        .await
                        .map_err(reqwest_err);
                    pending.release();
                    r
                }))
            }
        }),
    )
    .unwrap();

    let bytes_fn = Function::new(
        ctx.clone(),
        MutFn::from({
            let consumed = consumed.clone();
            let url = url.clone();
            let client = client.clone();
            move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
                if consumed.get() {
                    return Err(throw_consumed(&ctx));
                }
                consumed.set(true);
                let pending = ctx.userdata::<PendingOps>().unwrap().clone();
                let url = url.clone();
                let client = client.clone();
                Ok(Promised(async move {
                    pending.hold();
                    let r = client
                        .get(url.as_ref())
                        .send()
                        .await
                        .map_err(reqwest_err)?
                        .bytes()
                        .await
                        .map(|b| JsBytes(b.to_vec()))
                        .map_err(reqwest_err);
                    pending.release();
                    r
                }))
            }
        }),
    )
    .unwrap();

    let json_fn = Function::new(
        ctx.clone(),
        MutFn::from({
            let consumed = consumed.clone();
            let url = url.clone();
            let client = client.clone();
            move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
                if consumed.get() {
                    return Err(throw_consumed(&ctx));
                }
                consumed.set(true);
                let pending = ctx.userdata::<PendingOps>().unwrap().clone();
                let url = url.clone();
                let client = client.clone();
                Ok(Promised(async move {
                    pending.hold();
                    let r = client
                        .get(url.as_ref())
                        .send()
                        .await
                        .map_err(reqwest_err)?
                        .text()
                        .await
                        .map(JsonValue)
                        .map_err(reqwest_err);
                    pending.release();
                    r
                }))
            }
        }),
    )
    .unwrap();

    let obj = Object::new(ctx.clone())?;
    obj.set("url", url.as_ref().clone())?;
    obj.set("text", text_fn)?;
    obj.set("bytes", bytes_fn)?;
    obj.set("json", json_fn)?;

    Ok(obj.into_value())
}

pub(crate) async fn init_io(context: &AsyncContext) {
    context
        .with(|ctx| {
            ctx.store_userdata(http_client()).unwrap();

            let globals = ctx.globals();

            let source_fn = Function::new(ctx.clone(), io_source).unwrap();

            let io = Object::new(ctx.clone()).unwrap();
            io.set("source", source_fn).unwrap();

            globals.set("io", io).unwrap();
        })
        .await;
}
