use crate::power::info;

/// The charge SDL reports is a percentage.
const MAX_PERCENT: u8 = 100;

#[test]
fn info_answers_without_sdl_init() {
  let power = info();
  if let Some(percent) = power.percent {
    assert!(percent <= MAX_PERCENT, "percent {percent} above {MAX_PERCENT}");
  }
}
