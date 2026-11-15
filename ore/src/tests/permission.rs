use std::sync::{Arc, Mutex, PoisonError};

use super::facts_lock;
use crate::permission::{
  self, android_state, answered_state, request, unix_state, windows_state, Permission, PermissionState,
};

#[test]
fn every_permission_and_state_has_its_name() {
  let names: Vec<&str> = Permission::ALL.iter().map(|p| p.name()).collect();
  assert_eq!(names, vec!["camera", "microphone"]);
  let states: Vec<&str> = PermissionState::ALL.iter().map(|s| s.name()).collect();
  assert_eq!(states, vec!["granted", "denied", "prompt"]);
}

// A platform with no prompt of its own answers with the state before
// `request` returns, so a caller can open the device straight away.
#[cfg(not(target_os = "android"))]
#[test]
fn a_platform_without_a_prompt_answers_the_state_at_once() {
  let _facts = facts_lock();
  let answer = Arc::new(Mutex::new(None));
  let answered = answer.clone();
  request(Permission::Microphone, move |state| {
    *answered.lock().unwrap_or_else(PoisonError::into_inner) = Some(state);
  })
  .expect("request is made");
  let answer = *answer.lock().unwrap_or_else(PoisonError::into_inner);
  assert_eq!(answer, Some(permission::state(Permission::Microphone)));
}

#[test]
fn a_change_is_recorded_and_queued_once_and_a_repeat_not_at_all() {
  let _facts = facts_lock();
  let before = permission::state(Permission::Camera);
  permission::record(Permission::Camera, PermissionState::Prompt);
  permission::drain();

  permission::record(Permission::Camera, PermissionState::Granted);
  assert_eq!(permission::state(Permission::Camera), PermissionState::Granted);
  permission::record(Permission::Camera, PermissionState::Granted);
  permission::record(Permission::Camera, PermissionState::Denied);
  assert_eq!(permission::state(Permission::Camera), PermissionState::Denied);
  assert_eq!(
    permission::drain(),
    vec![(Permission::Camera, PermissionState::Granted), (Permission::Camera, PermissionState::Denied)]
  );
  assert!(permission::drain().is_empty());

  permission::record(Permission::Camera, before);
  permission::drain();
}

// Android cannot tell "never asked" from "denied for good" on its own, so
// the asked-before bit decides that row; the rationale wins over it (denied
// once, a request still shows the dialog), and a grant wins over both.
#[test]
fn the_android_facts_map_to_every_state() {
  use PermissionState::{Denied, Granted, Prompt};
  // (granted, rationale, asked) -> state, every row of the truth table.
  let rows = [
    ((false, false, false), Prompt),
    ((false, false, true), Denied),
    ((false, true, false), Prompt),
    ((false, true, true), Prompt),
    ((true, false, false), Granted),
    ((true, false, true), Granted),
    ((true, true, false), Granted),
    ((true, true, true), Granted),
  ];
  for ((granted, rationale, asked), state) in rows {
    assert_eq!(
      android_state(granted, rationale, asked),
      state,
      "granted {granted}, rationale {rationale}, asked {asked}"
    );
  }
}

// An Android request's answer records what the foreground re-read would:
// a grant is granted without a read; a refusal is the mapping over fresh
// reads (prompt after a first denial, denied after "don't ask again"), and
// denied only when the activity could not be read.
#[test]
fn an_android_answer_records_the_same_mapping_as_the_resume() {
  use PermissionState::{Denied, Granted, Prompt};
  assert_eq!(answered_state(true, || panic!("a grant reads nothing")), Granted);
  // A first denial: not granted, a rationale to show, asked before.
  assert_eq!(answered_state(false, || Some(android_state(false, true, true))), Prompt);
  // "Don't ask again", or the second denial: no rationale, asked before.
  assert_eq!(answered_state(false, || Some(android_state(false, false, true))), Denied);
  assert_eq!(answered_state(false, || None), Denied);
}

// Either privacy switch set to "Deny" denies; anything else, absent
// included, grants. Windows never reads prompt.
#[test]
fn the_windows_switches_map_to_granted_or_denied() {
  use PermissionState::{Denied, Granted};
  let values = [None, Some("Allow"), Some("Deny")];
  for consent in values {
    for non_packaged in values {
      let expected = if consent == Some("Deny") || non_packaged == Some("Deny") { Denied } else { Granted };
      assert_eq!(windows_state(consent, non_packaged), expected, "consent {consent:?}, non-packaged {non_packaged:?}");
    }
  }
}

// Outside a sandbox there is no permission model; inside Flatpak the camera
// portal asks at the open.
#[test]
fn linux_grants_outside_a_sandbox_and_the_flatpak_camera_prompts() {
  assert_eq!(unix_state(Permission::Camera, false), PermissionState::Granted);
  assert_eq!(unix_state(Permission::Microphone, false), PermissionState::Granted);
  assert_eq!(unix_state(Permission::Camera, true), PermissionState::Prompt);
  assert_eq!(unix_state(Permission::Microphone, true), PermissionState::Granted);
}
