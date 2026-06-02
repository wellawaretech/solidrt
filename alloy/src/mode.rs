use crate::record::RecordConfig;

// Operating mode of the app. Run is the normal interactive loop driven by the
// display; Record drives a deterministic lockstep capture loop with its own
// config. More modes are expected, so prefer matching over assuming two cases.
pub enum Mode {
  Run,
  Record(RecordConfig),
}

impl Mode {
  pub fn is_record(&self) -> bool {
    matches!(self, Mode::Record(_))
  }
}