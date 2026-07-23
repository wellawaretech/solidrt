use rquickjs::{Ctx, JsLifetime};
use std::rc::Rc;

// The runtime's own product token (FLUX_VERSION is the git-describe build
// stamp, see build.rs). Embedders replace it with their identity via
// `FluxEngineBuilder::user_agent`.
const USER_AGENT: &str = concat!("FluxRT/", env!("FLUX_VERSION"));

/// Builder-provided User-Agent (`FluxEngineBuilder::user_agent`).
#[derive(Clone, JsLifetime)]
pub struct UserAgent(#[qjs(skip_trace)] pub String);

#[derive(Clone, JsLifetime)]
pub(crate) struct HttpClient(#[qjs(skip_trace)] pub Rc<reqwest::Client>);

pub(crate) fn init_http(ctx: &Ctx<'_>) {
  let agent = ctx.userdata::<UserAgent>().map(|ua| ua.0.clone()).unwrap_or_else(|| USER_AGENT.to_string());
  let client = HttpClient(Rc::new(reqwest::Client::builder().user_agent(agent).build().expect("build http client")));
  ctx.store_userdata(client).expect("store http client");
}
