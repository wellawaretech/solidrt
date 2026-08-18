//! The `flux:mdns` module: zero-config (Bonjour/Avahi) discovery.
//!
//! Marshalling only: decode JS args into the engine-free `forge::mdns` core, drive
//! its queries, and encode the results back to JS. The protocol mechanics (the
//! multicast socket, the DNS wire codec, the response correlation) live in
//! `forge::mdns`, and its result types encode themselves as `forge::Value`.
//!
//! Surface:
//! - `resolve(ips, opts?)` -> `[{ ip, host }]`: reverse-resolve IPv4 addresses to
//!   their mDNS `.local` hostnames (the immediate consumer of this module).
//! - `browse(service, opts?)` -> `[ServiceInstance]`: the DNS-SD instances of a
//!   service type, e.g. `"_http._tcp"`.
//! - `services(opts?)` -> `[string]`: the service types advertised on the LAN.
//!
//! All three take an optional `{ timeoutMs }` (default 1500) bounding how long the
//! query collects answers, and resolve to an empty array on a LAN with no
//! responders rather than rejecting. They are plain async functions (no JS classes
//! to build), so each is a `with_pending` future rejecting through the `JsResult`
//! path (a clean `Error`, no `IO Error:` prefix).

use std::future::Future;

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Function, Object};

use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{with_pending, OptArg};
use crate::plugins::value::Neutral;
use forge::Value;

/// Default window (ms) a query waits for multicast answers.
const DEFAULT_TIMEOUT_MS: u64 = 1500;

// ---- free functions ---------------------------------------------------------

/// `resolve(ips, { timeoutMs? })` -> `Promise<{ ip, host }[]>`. IPv6 inputs and
/// addresses that do not answer are simply absent from the result.
fn mdns_resolve<'js>(
  ctx: Ctx<'js>,
  ips: Vec<String>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Neutral>>>> {
  let timeout_ms = opt_timeout(&opts)?;
  Ok(with_pending(
    &ctx,
    async move { forge::mdns::resolve(ips, timeout_ms).await.map(|hosts| Neutral(Value::list(hosts))) },
  ))
}

/// `browse(service, { timeoutMs? })` -> `Promise<ServiceInstance[]>`. `service`
/// may be bare (`"_http._tcp"`) or fully qualified.
fn mdns_browse<'js>(
  ctx: Ctx<'js>,
  service: String,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Neutral>>>> {
  let timeout_ms = opt_timeout(&opts)?;
  Ok(with_pending(&ctx, async move {
    forge::mdns::browse(service, timeout_ms).await.map(|found| Neutral(Value::list(found)))
  }))
}

/// `services({ timeoutMs? })` -> `Promise<string[]>`: the service types on the LAN.
fn mdns_services<'js>(
  ctx: Ctx<'js>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Vec<String>>>>> {
  let timeout_ms = opt_timeout(&opts)?;
  Ok(with_pending(&ctx, async move { forge::mdns::services(timeout_ms).await }))
}

// ---- module + helpers -------------------------------------------------------

pub struct MdnsModule;

impl ModuleDef for MdnsModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("resolve")?;
    decl.declare("browse")?;
    decl.declare("services")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("resolve", Function::new(ctx.clone(), mdns_resolve)?)?;
    exports.export("browse", Function::new(ctx.clone(), mdns_browse)?)?;
    exports.export("services", Function::new(ctx.clone(), mdns_services)?)?;
    Ok(())
  }
}

/// Read `timeoutMs` from the optional options object, defaulting to 1500 ms.
fn opt_timeout(opts: &OptArg<Object<'_>>) -> rquickjs::Result<u64> {
  match opts.0.as_ref() {
    Some(obj) => Ok(obj.get::<_, Option<f64>>("timeoutMs")?.map(|v| v.max(0.0) as u64).unwrap_or(DEFAULT_TIMEOUT_MS)),
    None => Ok(DEFAULT_TIMEOUT_MS),
  }
}
