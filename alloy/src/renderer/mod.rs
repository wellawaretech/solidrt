//! The renderer seam. Alloy describes a frame in the renderer's vocabulary
//! through its recording types, and draws it through one renderer that
//! implements the contract (`contract`); which renderer is a compile-time
//! choice, and a build holds exactly one. That is anvil, its own crate
//! (okf/plans/own-renderer.md). This module is the one place in alloy that
//! names it: it re-exports the crate whole, so nothing outside this module
//! changes with the renderer.
//!
//! In a test build the recording types also keep a plain-data log of what
//! they recorded (`oplog`, anvil's `oplog` feature, on for alloy's tests
//! only), which a list returns through `DisplayList::ops`, so a test reads
//! back what the paint walk drew.
//!
//! The traits are in scope through the prelude:
//!
//! ```ignore
//! use crate::renderer::prelude::*;
//! use crate::renderer::{DisplayListBuilder, Paint, Rect};
//! ```

pub use ::anvil::*;

// The crate under its own name, for the tests that read its pure walk
// (`renderer::anvil::walk`).
#[cfg(test)]
pub(crate) use ::anvil;
