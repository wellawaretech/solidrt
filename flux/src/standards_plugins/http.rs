use rquickjs::{Ctx, JsLifetime};

use forge::fetch::Client;

// The runtime's own product token (FLUX_VERSION is the git-describe build
// stamp, see build.rs). Embedders replace it with their identity via
// `FluxEngineBuilder::user_agent`.
const USER_AGENT: &str = concat!("FluxRT/", env!("FLUX_VERSION"));

/// Builder-provided User-Agent (`FluxEngineBuilder::user_agent`).
#[derive(Clone, JsLifetime)]
pub struct UserAgent(#[qjs(skip_trace)] pub String);

/// The engine's fetch client (`forge::fetch::Client`), one per context.
#[derive(Clone, JsLifetime)]
pub(crate) struct HttpClient(#[qjs(skip_trace)] pub Client);

pub(crate) fn init_http(ctx: &Ctx<'_>) {
  let agent = ctx.userdata::<UserAgent>().map(|ua| ua.0.clone()).unwrap_or_else(|| USER_AGENT.to_string());
  let client = HttpClient(Client::new(&agent).expect("build http client"));
  ctx.store_userdata(client).expect("store http client");
}
