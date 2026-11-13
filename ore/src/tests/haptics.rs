use crate::haptics::{Call, Impact, Intent, Level, Session, Support, Weight, REQUESTED_LOG_CAP};

const WEIGHTS: [Weight; 3] = [Weight::Light, Weight::Medium, Weight::Heavy];

// An impact intensity other than the default, to tell the two apart.
const SOFT_INTENSITY: f32 = 0.25;

// The intensity the API documents as the default: full.
const FULL_INTENSITY: f32 = 1.0;

fn plain(intent: Intent) -> Call {
  Call { intent, impact: None }
}

fn intents(session: &Session) -> Vec<Intent> {
  session.requested().iter().map(|recorded| recorded.call.intent).collect()
}

#[test]
fn a_record_only_session_logs_the_call_and_reports_full_support() {
  let session = Session::new();
  session.set_record_only();
  assert!(session.is_record_only());
  let impact = Impact { weight: Weight::Heavy, intensity: SOFT_INTENSITY };
  session.feedback(plain(Intent::ToggleOn));
  session.feedback(Call { intent: Intent::Impact, impact: Some(impact) });
  session.stop();
  let requested = session.requested();
  assert_eq!(requested.len(), 2);
  assert_eq!(requested[0].call, plain(Intent::ToggleOn));
  assert_eq!(requested[1].call, Call { intent: Intent::Impact, impact: Some(impact) });
  assert!(requested[0].at_ms >= 0.0);
  assert!(requested[1].at_ms >= requested[0].at_ms);
  let support = session.support();
  assert_eq!(support, Support::record_only());
  assert!(support.actuator);
  assert!(support.impact_intensity);
  for intent in Intent::ALL {
    assert_eq!(support.level(intent), Level::Native, "{}", intent.name());
  }
}

#[test]
fn the_log_keeps_the_newest_calls() {
  let session = Session::new();
  session.set_record_only();
  let total = REQUESTED_LOG_CAP * 2;
  for i in 0..total {
    session.feedback(plain(Intent::ALL[i % Intent::ALL.len()]));
  }
  let logged = intents(&session);
  assert_eq!(logged.len(), REQUESTED_LOG_CAP);
  let expected: Vec<Intent> = (total - REQUESTED_LOG_CAP..total).map(|i| Intent::ALL[i % Intent::ALL.len()]).collect();
  assert_eq!(logged, expected);
}

#[test]
fn a_non_impact_intent_drops_the_impact() {
  let session = Session::new();
  session.set_record_only();
  let impact = Impact { weight: Weight::Light, intensity: SOFT_INTENSITY };
  session.feedback(Call { intent: Intent::Success, impact: Some(impact) });
  assert_eq!(session.requested()[0].call, plain(Intent::Success));
}

#[test]
fn an_impact_without_one_gets_the_default() {
  let session = Session::new();
  session.set_record_only();
  session.feedback(plain(Intent::Impact));
  let call = session.requested()[0].call;
  assert_eq!(call.impact, Some(Impact::default()));
  assert_eq!(Impact::default(), Impact { weight: Weight::Medium, intensity: FULL_INTENSITY });
  assert_eq!(Weight::default(), Weight::Medium);
}

#[test]
fn every_intent_round_trips_its_name_in_wire_order() {
  for (index, intent) in Intent::ALL.into_iter().enumerate() {
    assert_eq!(intent.index(), index);
    assert_eq!(Intent::from_name(intent.name()), Some(intent));
  }
  assert_eq!(Intent::ThresholdOn.name(), "thresholdOn");
  assert_eq!(Intent::from_name("vibrate"), None);
  assert_eq!(Intent::from_name("Selection"), None);
}

#[test]
fn every_weight_round_trips_its_name_in_wire_order() {
  for (index, weight) in WEIGHTS.into_iter().enumerate() {
    assert_eq!(weight.index(), index);
    assert_eq!(Weight::from_name(weight.name()), Some(weight));
  }
  assert_eq!(Weight::from_name("huge"), None);
}

#[test]
fn levels_have_their_api_names() {
  assert_eq!(Level::Native.name(), "native");
  assert_eq!(Level::Approximate.name(), "approximate");
  assert_eq!(Level::None.name(), "none");
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_platform_without_an_actuator_plays_nothing_and_still_logs() {
  let session = Session::new();
  assert!(!session.is_record_only());
  assert!(!crate::haptics::available());
  assert_eq!(session.support(), Support::none());
  for intent in Intent::ALL {
    assert_eq!(session.support().level(intent), Level::None, "{}", intent.name());
  }
  session.feedback(plain(Intent::Error));
  session.stop();
  assert_eq!(intents(&session), vec![Intent::Error]);
}
