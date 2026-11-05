// The cadence-hold policy layers (cadence.rs): which policy is in force,
// and which changes reach alloy.

use std::sync::mpsc::{channel, Receiver};

use crate::cadence::{auto, CadencePolicy};
use alloy::{AlloyCommand, CadenceHold};

// What the layers sent alloy since the last read, in order: Some(policy)
// for a SetCadenceHold, None for a ResetCadenceHold.
fn sent(rx: &Receiver<AlloyCommand>) -> Vec<Option<CadenceHold>> {
  rx.try_iter()
    .map(|c| match c {
      AlloyCommand::SetCadenceHold(p) => Some(p),
      AlloyCommand::ResetCadenceHold => None,
      _ => panic!("the cadence policy sent a command that is not a cadence command"),
    })
    .collect()
}

// The policy in force: the app's choice over the default, and it survives
// the default moving (an input device plugged in); "default" hands the
// choice back. The measurement override goes over both.
#[test]
fn the_app_policy_beats_the_default_and_the_override_beats_both() {
  let (tx, _rx) = channel();
  let mut p = CadencePolicy::new(tx);
  assert_eq!(p.effective(), CadenceHold::Off);
  p.set_default(auto());
  assert_eq!(p.effective(), auto());
  p.set_app(Some(CadenceHold::Off));
  assert_eq!(p.effective(), CadenceHold::Off);
  // A mouse arrives, then leaves: the default moves, the app's choice holds.
  p.set_default(CadenceHold::Off);
  p.set_default(auto());
  assert_eq!(p.effective(), CadenceHold::Off);
  p.set_app(None);
  assert_eq!(p.effective(), auto());
  p.set_app(Some(CadenceHold::Fixed(2)));
  p.set_override(CadenceHold::Off);
  assert_eq!(p.effective(), CadenceHold::Off);
}

// The default is re-sent on every input-device event: only a change of the
// policy in force reaches alloy, and none while the app's choice is in force.
#[test]
fn a_default_reaches_alloy_only_when_it_changes_the_policy_in_force() {
  let (tx, rx) = channel();
  let mut p = CadencePolicy::new(tx);
  p.set_default(CadenceHold::Off);
  assert_eq!(sent(&rx), vec![]);
  p.set_default(auto());
  p.set_default(auto());
  assert_eq!(sent(&rx), vec![Some(auto())]);
  p.set_app(Some(CadenceHold::Fixed(2)));
  sent(&rx);
  p.set_default(CadenceHold::Off);
  assert_eq!(sent(&rx), vec![]);
}

// The app's choice restarts the controller even when it is the policy
// already in force: a call starts unheld.
#[test]
fn the_app_choice_reaches_alloy_even_when_the_policy_is_unchanged() {
  let (tx, rx) = channel();
  let mut p = CadencePolicy::new(tx);
  p.set_default(auto());
  sent(&rx);
  p.set_app(Some(auto()));
  p.set_app(None);
  assert_eq!(sent(&rx), vec![Some(auto()), Some(auto())]);
}

// Under the override the app's choice waits: nothing reaches alloy, and the
// override stays in force.
#[test]
fn the_app_choice_waits_under_the_override() {
  let (tx, rx) = channel();
  let mut p = CadencePolicy::new(tx);
  p.set_override(CadenceHold::Fixed(3));
  assert_eq!(sent(&rx), vec![Some(CadenceHold::Fixed(3))]);
  p.set_app(Some(CadenceHold::Off));
  p.set_app(None);
  p.set_default(auto());
  assert_eq!(sent(&rx), vec![]);
  assert_eq!(p.effective(), CadenceHold::Fixed(3));
  p.reset();
  assert_eq!(sent(&rx), vec![None]);
}
