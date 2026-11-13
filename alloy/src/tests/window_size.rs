use crate::window_size::size_to_apply;

/// A declaration equal to the last declared size sends nothing.
#[test]
fn same_size_is_skipped() {
  let mut declared = Some((900, 600));
  assert_eq!(size_to_apply(&mut declared, Some((900, 600))), None);
  assert_eq!(declared, Some((900, 600)));
}

/// A different size is applied and becomes the declared size.
#[test]
fn different_size_is_applied_and_remembered() {
  let mut declared = Some((900, 600));
  assert_eq!(size_to_apply(&mut declared, Some((320, 600))), Some((320, 600)));
  assert_eq!(declared, Some((320, 600)));
  assert_eq!(size_to_apply(&mut declared, Some((320, 600))), None);
}

/// Removing the prop forgets the declared size, so the same size declared
/// again applies.
#[test]
fn none_forgets_so_the_same_size_applies_again() {
  let mut declared = Some((900, 600));
  assert_eq!(size_to_apply(&mut declared, None), None);
  assert_eq!(declared, None);
  assert_eq!(size_to_apply(&mut declared, Some((900, 600))), Some((900, 600)));
  assert_eq!(declared, Some((900, 600)));
}

/// Seeded with the size the window opened at, a mount-time declaration
/// equal to it is a no-op, and one that differs is the resize asked for.
#[test]
fn seed_makes_a_mount_time_declaration_of_the_open_size_a_no_op() {
  let open_size = (800, 600);
  let mut declared = Some(open_size);
  assert_eq!(size_to_apply(&mut declared, Some(open_size)), None);
  let mut declared = Some(open_size);
  assert_eq!(size_to_apply(&mut declared, Some((640, 480))), Some((640, 480)));
}
