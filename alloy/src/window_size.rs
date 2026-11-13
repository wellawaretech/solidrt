//! The app's declared window size (the `<window size>` prop), deduplicated
//! in the platform loop.
//!
//! Set, not enforce: a declaration asks the OS for a size once, when its
//! value changes. A user drag does not write back and is not fought; the
//! prop is not a constraint. The memory of the last declared size lives in
//! the platform loop next to the window, not on the element, so it
//! outlives the engine: an object prop that re-fires with the same numbers,
//! a reload whose fresh engine declares the same size again, and a
//! mount-time declaration equal to the size the window opened at (the
//! memory's seed) all send nothing to the OS. The other side of the rule:
//! an app cannot put a dragged window back by declaring the same size,
//! because the value did not change. Removing the prop forgets the declared
//! size, so the next declaration always applies.

/// The size to ask the OS for, if any, for a declaration `request` against
/// the last declared size; updates `declared`. None forgets the declared
/// size and applies nothing; a size equal to the declared one applies
/// nothing; any other size is remembered and applied.
pub(crate) fn size_to_apply(declared: &mut Option<(u32, u32)>, request: Option<(u32, u32)>) -> Option<(u32, u32)> {
  match request {
    None => {
      *declared = None;
      None
    }
    Some(size) if *declared == Some(size) => None,
    Some(size) => {
      *declared = Some(size);
      Some(size)
    }
  }
}
