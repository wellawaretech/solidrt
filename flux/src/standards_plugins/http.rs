// The user agent is every HTTP request's: fetch's, and a media source's
// (flux:video). The fetch client itself is behind the `http` feature.
#[cfg(any(feature = "http", feature = "video"))]
use rquickjs::Ctx;
#[cfg(feature = "http")]
use rquickjs::JsLifetime;

#[cfg(feature = "http")]
use forge::fetch::{Client, HostLimits};
#[cfg(feature = "http")]
use std::sync::Arc;

// The runtime's own product token (FLUX_VERSION is the git-describe build
// stamp, see build.rs). Embedders replace it with their identity via
// `FluxEngineBuilder::user_agent`.
#[cfg(any(feature = "http", feature = "video"))]
const USER_AGENT: &str = concat!("FluxRT/", env!("FLUX_VERSION"));

/// The engine's fetch client (`forge::fetch::Client`), one per context.
#[cfg(feature = "http")]
#[derive(Clone, JsLifetime)]
pub(crate) struct HttpClient(#[qjs(skip_trace)] pub Client);

/// The app's per-host fetch limits (`EngineConfig::fetch_limits`), stored
/// per context: every client of every engine of the app, isolates included,
/// sends through the same ones, so a server sees one app.
#[cfg(feature = "http")]
#[derive(Clone, JsLifetime)]
pub(crate) struct FetchLimits(#[qjs(skip_trace)] pub Arc<HostLimits>);

/// The user agent every HTTP request from this engine carries:
/// `FluxEngineBuilder::user_agent`, read off the stored engine config, else
/// the runtime's own token.
#[cfg(any(feature = "http", feature = "video"))]
pub(crate) fn user_agent(ctx: &Ctx<'_>) -> String {
  ctx
    .userdata::<crate::engine::EngineConfig>()
    .and_then(|config| config.user_agent.clone())
    .unwrap_or_else(|| USER_AGENT.to_string())
}

#[cfg(feature = "http")]
pub(crate) fn init_http(ctx: &Ctx<'_>) {
  let agent = user_agent(ctx);
  let client = HttpClient(Client::new(&agent).expect("build http client"));
  ctx.store_userdata(client).expect("store http client");
  let limits = ctx.userdata::<crate::engine::EngineConfig>().expect("engine config userdata").fetch_limits.clone();
  ctx.store_userdata(FetchLimits(limits)).expect("store fetch limits");
}
