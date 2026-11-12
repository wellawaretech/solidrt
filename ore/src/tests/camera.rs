use crate::camera::prompts_for_driver;

// The Linux backends never prompt, so a long Pending there is a wedge.
#[test]
fn linux_backends_show_no_prompt() {
  assert!(!prompts_for_driver(Some("v4l2")));
  assert!(!prompts_for_driver(Some("pipewire")));
}

// The OS backends with a permission dialog may be waiting on the user.
#[test]
fn os_backends_may_prompt() {
  assert!(prompts_for_driver(Some("android")));
  assert!(prompts_for_driver(Some("coremedia")));
  assert!(prompts_for_driver(Some("mediafoundation")));
}

// No driver yet (the subsystem not up) leans toward a prompt: no deadline
// rather than a wrong one.
#[test]
fn no_driver_may_prompt() {
  assert!(prompts_for_driver(None));
}
