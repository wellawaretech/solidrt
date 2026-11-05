use std::sync::mpsc::Sender;

use alloy::{AlloyCommand, CadenceHold};

// The cadence-hold policy (tier 3 of okf/design/frame-timing.md): which
// policy is in force, chosen here; alloy's controller carries out the one it
// is sent (AlloyCommand::SetCadenceHold) and reports the hold it applies
// (AlloyEvent::CadenceHold).

// The longest frame slot the automatic hold may choose, ms: no hold slower
// than 20 fps (Android's Frame Pacing library's default too); a workload
// that needs more is held at this slot and its longer intervals count as
// misses.
const AUTO_MAX_MS: u32 = 50;

/// The automatic hold at the runtime's maximum slot.
pub fn auto() -> CadenceHold {
  CadenceHold::Auto { max_ms: AUTO_MAX_MS }
}

/// The runtime's default from the input-modality fact: held where fluency
/// beats frame rate - touch clients, which run one frame in flight anyway,
/// and remote-driven clients (no touch, no mouse) - and off where a mouse
/// is present, because a pointer-driven client pipelines under swap pacing
/// and pays more than the boundary rounding for a hold, and cursor motion
/// reads better at a higher uneven rate.
pub fn default_for(mouse: bool) -> CadenceHold {
  if mouse {
    CadenceHold::Off
  } else {
    auto()
  }
}

/// `SOLIDRT_CADENCE_HOLD` (`off`, `auto` or a whole number of refreshes):
/// the measurement override, over the default and the app's choice, for
/// probes and censuses. Read once, at the UI thread's start.
pub fn env_override() -> Option<CadenceHold> {
  let v = std::env::var("SOLIDRT_CADENCE_HOLD").ok()?;
  match v.trim() {
    "off" => Some(CadenceHold::Off),
    "auto" => Some(auto()),
    s => match s.parse::<u32>() {
      Ok(k) if k >= 1 => Some(CadenceHold::Fixed(k)),
      _ => {
        log::warn!("[sol] SOLIDRT_CADENCE_HOLD={v}: expected off, auto or a whole number of refreshes; ignored");
        None
      }
    },
  }
}

/// Who chooses the cadence-hold policy, in three layers: the runtime's
/// default (from the input modality, re-sent on every input-device event),
/// the app's choice over it (`setCadenceHold` in sol:app), and the
/// environment override over both. An input device change moves the
/// default and leaves the app's choice in force. Every change of the
/// policy in force is sent to alloy, whose controller restarts on it.
pub struct CadencePolicy {
  default: CadenceHold,
  app: Option<CadenceHold>,
  overridden: Option<CadenceHold>,
  alloy_cmd_tx: Sender<AlloyCommand>,
}

impl CadencePolicy {
  /// Off until the default arrives, which is alloy's own starting policy.
  pub fn new(alloy_cmd_tx: Sender<AlloyCommand>) -> Self {
    CadencePolicy { default: CadenceHold::Off, app: None, overridden: None, alloy_cmd_tx }
  }

  /// The policy in force.
  pub fn effective(&self) -> CadenceHold {
    self.overridden.or(self.app).unwrap_or(self.default)
  }

  /// The runtime's default. Re-sent on every input-device event, so only a
  /// change of the policy in force reaches alloy.
  pub fn set_default(&mut self, policy: CadenceHold) {
    let before = self.effective();
    self.default = policy;
    if self.effective() != before {
      self.apply();
    }
  }

  /// The app's choice; None hands it back to the default. It applies at
  /// once and starts unheld, even when the policy in force does not change;
  /// under the override it waits.
  pub fn set_app(&mut self, policy: Option<CadenceHold>) {
    self.app = policy;
    match (self.overridden, policy) {
      (Some(o), Some(p)) => log::info!("[sol] cadence hold: the app's policy {p:?} waits under the override {o:?}"),
      (Some(_), None) => {}
      (None, _) => self.apply(),
    }
  }

  /// The measurement override, for the process's whole life.
  pub fn set_override(&mut self, policy: CadenceHold) {
    self.overridden = Some(policy);
    self.apply();
  }

  /// End the hold alloy's controller learned, keeping the policy: the app's
  /// at a phase start, the runtime's when a new engine starts.
  pub fn reset(&self) {
    self.alloy_cmd_tx.send(AlloyCommand::ResetCadenceHold).ok();
  }

  fn apply(&self) {
    self.alloy_cmd_tx.send(AlloyCommand::SetCadenceHold(self.effective())).ok();
  }
}
