//! The text input session's configuration, plain data: what the OS keyboard
//! and the IME are asked for when a text input session starts. Part of
//! ore's window part (the `window` feature); a session belongs to a window.

/// IME/session configuration for starting text input, mirroring SDL's
/// text-input properties; a None knob keeps SDL's default. Only meaningful
/// when activating - deactivation ignores it.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextInputOptions {
  pub input_type: Option<TextInputType>,
  pub capitalize: Option<TextCapitalization>,
  pub autocorrect: Option<bool>,
  pub multiline: Option<bool>,
}

/// SDL_TextInputType, minus nothing: the OS keyboard's semantic layout and
/// masking for the session.
#[derive(Clone, Copy, Debug)]
pub enum TextInputType {
  Text,
  Name,
  Email,
  Username,
  PasswordHidden,
  PasswordVisible,
  Number,
  NumberPasswordHidden,
  NumberPasswordVisible,
}

/// SDL_Capitalization: what the IME auto-capitalizes. SDL's default for plain
/// text is Sentences, which is exactly wrong for identifiers and terminals.
#[derive(Clone, Copy, Debug)]
pub enum TextCapitalization {
  None,
  Sentences,
  Words,
  Letters,
}
