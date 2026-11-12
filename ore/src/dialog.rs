//! The system file dialog vocabulary, plain data: what an open, save or
//! folder picker is asked for, what the user picked, and how a request is
//! answered. Part of ore's window part (the `window` feature): a dialog
//! shows with the app window as its parent. What comes back is a location
//! string a file handle opens later: a path on desktop, a `content://` URI
//! on Android. A cancel is an empty answer, never an error.

/// Which dialog to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogKind {
  /// Pick one file, or several when `multiple`.
  OpenFile { multiple: bool },
  /// Pick a place and name to write to; `suggested_name` pre-fills the name.
  SaveFile { suggested_name: Option<String> },
  /// Pick one folder.
  OpenDirectory,
}

/// A file type the dialog offers to narrow the listing to: a label and the
/// extensions (without the dot) it covers. No filters means every file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogFilter {
  pub name: String,
  pub extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogRequest {
  pub kind: DialogKind,
  pub filters: Vec<DialogFilter>,
  /// Where the dialog opens: a directory path on desktop, a `content://`
  /// URI on Android (a plain path is ignored there). None: the platform's
  /// default.
  pub start_in: Option<String>,
}

/// One entry the user picked: where it is (the string a `flux:fs` handle
/// opens: a path, or a `content://` URI on Android) and the name to show
/// for it (the file name of a path, the provider's display name of a URI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
  pub location: String,
  pub name: String,
}

/// Why a request got no picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogError {
  /// A dialog is already open: the platform's are modal, one at a time.
  Busy,
  /// This platform (or surface) cannot show a dialog.
  Unavailable,
  /// The platform could not show or finish the dialog.
  Failed(String),
}

impl std::fmt::Display for DialogError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DialogError::Busy => f.write_str("a dialog is already open"),
      DialogError::Unavailable => f.write_str("no file dialog on this platform"),
      DialogError::Failed(message) => f.write_str(message),
    }
  }
}

/// A request's outcome: the picked entries, empty when the user cancelled
/// (a cancel is an outcome, never an error), or why there are none.
pub type DialogAnswer = Result<Vec<Picked>, DialogError>;

/// What a request is answered through, once, from whatever thread the
/// answer arrives on.
pub type DialogResponder = Box<dyn FnOnce(DialogAnswer) + Send>;
