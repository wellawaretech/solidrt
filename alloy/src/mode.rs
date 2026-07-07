use crate::playback::PlaybackConfig;

// Operating mode of the app. Run is the normal interactive loop driven by the
// display; Playback drives a deterministic lockstep capture loop (optionally
// replaying a scripted input timeline) with its own config. More modes are
// expected, so prefer matching over assuming two cases.
pub enum Mode {
  Run,
  Playback(PlaybackConfig),
}

impl Mode {
  pub fn is_playback(&self) -> bool {
    matches!(self, Mode::Playback(_))
  }
}
