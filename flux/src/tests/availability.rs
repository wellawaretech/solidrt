// The names `Flux.availability` and the embedder's verbs take: every
// facility and every state round-trips through its name, and another name
// maps to nothing.

use ore::availability::{Availability, Facility};

use crate::availability::{facility_named, state_named};

#[test]
fn every_facility_maps_from_its_name() {
  for facility in Facility::ALL {
    assert_eq!(facility_named(facility.name()), Some(*facility));
  }
  assert_eq!(facility_named("clipboard-image"), Some(Facility::ClipboardImage));
  assert_eq!(facility_named("audio"), Some(Facility::AudioOutput));
}

#[test]
fn another_facility_name_maps_to_nothing() {
  assert_eq!(facility_named("teleport"), None);
  assert_eq!(facility_named("Link"), None);
  assert_eq!(facility_named(""), None);
}

#[test]
fn every_state_maps_from_its_name() {
  for state in [Availability::Available, Availability::Unavailable, Availability::Unknown] {
    assert_eq!(state_named(state.name()), Some(state));
  }
  assert_eq!(state_named("maybe"), None);
}
