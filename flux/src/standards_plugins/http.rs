use rquickjs::{Ctx, JsLifetime};

use forge::fetch::Client;

// The runtime's own product token (FLUX_VERSION is the git-describe build
// stamp, see build.rs). Embedders replace it with their identity via
// `FluxEngineBuilder::user_agent`.
const USER_AGENT: &str = concat!("FluxRT/", env!("FLUX_VERSION"));

/// The engine's fetch client (`forge::fetch::Client`), one per context.
#[derive(Clone, JsLifetime)]
pub(crate) struct HttpClient(#[qjs(skip_trace)] pub Client);

/// The user agent every HTTP request from this engine carries:
/// `FluxEngineBuilder::user_agent`, read off the stored engine config, else
/// the runtime's own token.
pub(crate) fn user_agent(ctx: &Ctx<'_>) -> String {
  ctx
    .userdata::<crate::engine::EngineConfig>()
    .and_then(|config| config.user_agent.clone())
    .unwrap_or_else(|| USER_AGENT.to_string())
}

pub(crate) fn init_http(ctx: &Ctx<'_>) {
  let agent = user_agent(ctx);
  let client = HttpClient(Client::new(&agent).expect("build http client"));
  ctx.store_userdata(client).expect("store http client");
}
