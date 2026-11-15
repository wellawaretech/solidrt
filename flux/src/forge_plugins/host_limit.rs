//! `getHostLimit` and `setHostLimit` of `flux:http`: the per-host cap of the
//! app's fetch limits (`forge::fetch::HostLimits`, the `FetchLimits` every
//! engine of the app shares), read and set. Marshalling only: a host is
//! resolved by `forge::fetch::parse_host_key`, and JS's `Infinity` is
//! forge's `None` (unlimited).

use rquickjs::{Ctx, Exception, Value};

use crate::plugins::marshal::OptArg;
use crate::standards_plugins::http::FetchLimits;

fn limits(ctx: &Ctx<'_>) -> std::sync::Arc<forge::fetch::HostLimits> {
  ctx.userdata::<FetchLimits>().expect("fetch limits").0.clone()
}

/// The host key of an optional `host` argument: a "host:port" key or a URL.
fn read_host(ctx: &Ctx<'_>, api: &str, host: OptArg<Value<'_>>) -> rquickjs::Result<Option<String>> {
  let Some(host) = host.0 else { return Ok(None) };
  let key = match host.as_string() {
    Some(s) => forge::fetch::parse_host_key(&s.to_string()?),
    None => None,
  };
  match key {
    Some(key) => Ok(Some(key)),
    None => Err(Exception::throw_type(ctx, &format!("{api}: host must be a \"host:port\" key or a URL"))),
  }
}

/// `getHostLimit(host?)` -> the cap of `host`, or the default; `Infinity`
/// for unlimited.
pub(crate) fn get_host_limit<'js>(ctx: Ctx<'js>, host: OptArg<Value<'js>>) -> rquickjs::Result<f64> {
  let host = read_host(&ctx, "getHostLimit", host)?;
  Ok(match limits(&ctx).limit(host.as_deref()) {
    Some(cap) => cap as f64,
    None => f64::INFINITY,
  })
}

/// `setHostLimit(limit, host?)`: a positive integer or `Infinity`, for
/// `host` or, without one, as the default.
pub(crate) fn set_host_limit<'js>(
  ctx: Ctx<'js>,
  limit: OptArg<Value<'js>>,
  host: OptArg<Value<'js>>,
) -> rquickjs::Result<()> {
  let limit = match limit.0.and_then(|v| v.as_number()) {
    Some(n) if n == f64::INFINITY => None,
    Some(n) if n >= 1.0 && n.fract() == 0.0 && n <= forge::fetch::MAX_HOST_LIMIT as f64 => Some(n as usize),
    _ => {
      return Err(Exception::throw_type(
        &ctx,
        "setHostLimit: limit must be a positive integer within the runtime's maximum, or Infinity",
      ))
    }
  };
  let host = read_host(&ctx, "setHostLimit", host)?;
  limits(&ctx).set_limit(host.as_deref(), limit);
  Ok(())
}
