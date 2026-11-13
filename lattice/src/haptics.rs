// The haptics session (`ore::haptics::Session`, reached through
// `flux::haptics::session`) as the control API and the test verb spell
// it: the feedback calls the app made, encoded to JSON. The words are
// flux:haptics' own (`intent`, `weight`, `intensity`).

use ore::haptics::Recorded;

/// One call: `{ intent, weight, intensity, at }`, `weight` and `intensity`
/// null for every intent but impact, `at` in ms since the session began.
pub fn recorded_json(recorded: &Recorded) -> serde_json::Value {
  let weight = recorded.call.impact.map(|impact| impact.weight.name());
  let intensity = recorded.call.impact.map(|impact| decimal(impact.intensity));
  serde_json::json!({
    "intent": recorded.call.intent.name(),
    "weight": weight,
    "intensity": intensity,
    "at": recorded.at_ms,
  })
}

/// The feedback calls the app made in this engine, oldest first; empty
/// without a haptics session.
pub fn requested_json(ctx: &flux::rquickjs::Ctx<'_>) -> serde_json::Value {
  let requested = flux::haptics::session(ctx).map(|session| session.requested()).unwrap_or_default();
  serde_json::Value::Array(requested.iter().map(recorded_json).collect())
}

/// `value` as the f64 its shortest decimal spells, so an intensity the app
/// passed as 0.8 reads back as 0.8, not as the f32's widened 0.800000011..
fn decimal(value: f32) -> f64 {
  value.to_string().parse().unwrap_or(f64::from(value))
}
