use rquickjs::{Ctx, JsLifetime};
use std::rc::Rc;

const USER_AGENT: &str = concat!("flux/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, JsLifetime)]
pub(crate) struct HttpClient(#[qjs(skip_trace)] pub Rc<reqwest::Client>);

pub(crate) fn init_http(ctx: &Ctx<'_>) {
  let client =
    HttpClient(Rc::new(reqwest::Client::builder().user_agent(USER_AGENT).build().expect("build http client")));
  ctx.store_userdata(client).expect("store http client");
}
