// The input plan: the `/input` event shape expanded into the events that
// are sent, in order, with what passes between them. Shared by the control
// API (go/connection.rs, real time) and by tests (plugins/test.rs, frames),
// so a gesture means the same thing to an agent driving a client and to a
// test (okf/done/test-harness.md, D8 and D37). The event shape itself is
// documented in packages/cli/agents/debugging.md.

/// Reserved pointer id for injected pointer events: far outside anything SDL
/// hands out, so a synthetic pointer never aliases a live one in the router
/// or the runner's input state.
const SYNTHETIC_POINTER_ID: u64 = 1 << 60;
/// Per-event cap on `delayMs`/`holdMs`/`durationMs` and whole-sequence
/// duration cap for an input plan, bounding how long a sequence can run. The
/// dev server sizes its query timeout from the same request, so these two
/// never race.
const INPUT_DELAY_MAX_MS: u64 = 5000;
const INPUT_TOTAL_MAX_MS: u64 = 30_000;
/// Highest slot a synthetic gamepad may name: a cap on the slot vector a
/// driven session can grow, well past any couch.
const GAMEPAD_SLOT_MAX: u64 = 16;

/// How long a drag takes when it names no `durationMs`: a deliberate
/// drag, slow enough to be a pan and not a fling.
const DRAG_DEFAULT_MS: u64 = 300;

/// What a step of the plan sends: an event for the UI thread's input
/// channel, or a synthetic-gamepad command for the alloy loop, where the
/// pads live.
pub(crate) enum Injected {
  Event(alloy::AlloyEvent),
  Gamepad(alloy::GamepadCommand),
}

/// What passes before a step is sent: at least `ms` of time and at least
/// `frames` frames. The frames are what makes a gesture real where time
/// alone does not: the up of a tap comes a frame after its down (a down and
/// up in one batch never render the pressed state), and a mouse is moved to
/// a point a frame ahead of pressing there (its hover is dispatched with
/// the frame). Whoever runs the plan decides what a frame is: a test steps
/// one, the control API waits one frame interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wait {
  pub ms: u64,
  pub frames: u32,
}

impl Wait {
  fn ms(ms: u64) -> Self {
    Self { ms, frames: 0 }
  }

  /// `ms`, and a frame at the least.
  fn frame(ms: u64) -> Self {
    Self { ms, frames: 1 }
  }
}

pub(crate) struct Step {
  pub wait: Wait,
  pub inject: Injected,
}

/// Expand an `events` array (the `/input` event shape) into a flat plan of
/// steps, each sent after its wait. This is the one place a gesture is
/// spelled out, for the control API and for tests alike: a `tap` is down
/// and up, the up a frame and its `holdMs` later; a `drag` is a down, one
/// move per frame along the line to `to` over its `durationMs`, and an up;
/// a mouse tap or drag starts with a move to the point, a frame ahead; a
/// gamepad `set` with `holdMs` is the state and the neutral state after
/// it. `frame_ms` is the frame interval of whoever runs the plan, which
/// sets how many moves a drag has. Everything is validated upfront: any
/// invalid event rejects the whole sequence before a single event is sent.
pub(crate) fn plan(events: Option<&serde_json::Value>, frame_ms: f64) -> Result<Vec<Step>, String> {
  use alloy::{AlloyEvent, GamepadCommand, Modifiers, PointerType};
  let arr = events.and_then(|e| e.as_array()).ok_or("events must be an array")?;
  if arr.is_empty() {
    return Err("events must not be empty".into());
  }
  let mut out = Vec::new();
  let mut total: u64 = 0;
  for (i, ev) in arr.iter().enumerate() {
    let field_ms = |name: &str| -> Result<u64, String> {
      match ev.get(name) {
        None => Ok(0),
        Some(v) => match v.as_u64() {
          Some(ms) if ms <= INPUT_DELAY_MAX_MS => Ok(ms),
          _ => Err(format!("events[{i}]: {name} must be an integer 0..={INPUT_DELAY_MAX_MS}")),
        },
      }
    };
    let str_field = |name: &str| ev.get(name).and_then(|v| v.as_str());
    let num_field = |name: &str| -> Result<f32, String> {
      ev.get(name)
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite())
        .map(|v| v as f32)
        .ok_or_else(|| format!("events[{i}]: {name} must be a finite number"))
    };
    let flag = |name: &str| ev.get(name).and_then(|v| v.as_bool()).unwrap_or(false);
    let modifiers = Modifiers { shift: flag("shift"), ctrl: flag("ctrl"), alt: flag("alt"), meta: flag("meta") };
    let delay = field_ms("delayMs")?;
    let hold = field_ms("holdMs")?;
    let ty = str_field("type").ok_or_else(|| format!("events[{i}]: missing type"))?;
    let action = str_field("action");
    let holds = action == Some("tap") || (ty == "gamepad" && action == Some("set"));
    if ev.get("holdMs").is_some() && !holds {
      return Err(format!("events[{i}]: holdMs only applies to action \"tap\" and a gamepad \"set\""));
    }
    let mut push = |wait: Wait, event: AlloyEvent| out.push(Step { wait, inject: Injected::Event(event) });
    match ty {
      "key" => {
        let key = str_field("key")
          .filter(|k| !k.is_empty())
          .ok_or_else(|| format!("events[{i}]: key events need a non-empty key name"))?;
        let make = |down: bool| AlloyEvent::Key {
          down,
          key: key.to_string(),
          code: alloy::w3c_code_for_key(key),
          modifiers,
          repeat: false,
        };
        match action {
          Some("down") => push(Wait::ms(delay), make(true)),
          Some("up") => push(Wait::ms(delay), make(false)),
          Some("tap") => {
            push(Wait::ms(delay), make(true));
            push(Wait::frame(hold), make(false));
          }
          _ => return Err(format!("events[{i}]: key action must be down, up or tap")),
        }
      }
      "pointer" => {
        let x = num_field("x")?;
        let y = num_field("y")?;
        let pointer_type = match str_field("pointerType") {
          None | Some("mouse") => PointerType::Mouse,
          Some("touch") => PointerType::Touch,
          Some(_) => return Err(format!("events[{i}]: pointerType must be mouse or touch")),
        };
        let button = match ev.get("button") {
          None => 0u8,
          Some(v) => match v.as_u64() {
            Some(b) if b <= 4 => b as u8,
            _ => return Err(format!("events[{i}]: button must be an integer 0..=4")),
          },
        };
        let down =
          || AlloyEvent::PointerDown { pointer_id: SYNTHETIC_POINTER_ID, pointer_type, button, x, y, modifiers };
        let up = |x: f32, y: f32| AlloyEvent::PointerUp {
          pointer_id: SYNTHETIC_POINTER_ID,
          pointer_type,
          button,
          x,
          y,
          modifiers,
        };
        // No hardware delta for synthetic moves: movement derives from the
        // position diff, the honest synthetic delta.
        let mv = |x: f32, y: f32| AlloyEvent::PointerMove {
          pointer_id: SYNTHETIC_POINTER_ID,
          pointer_type,
          x,
          y,
          rel: None,
          modifiers,
        };
        // The down of a gesture. A mouse is somewhere before it presses:
        // the move ahead of the down is what puts the hover there.
        let press = |push: &mut dyn FnMut(Wait, AlloyEvent)| {
          if pointer_type == PointerType::Mouse {
            push(Wait::ms(delay), mv(x, y));
            push(Wait::frame(0), down());
          } else {
            push(Wait::ms(delay), down());
          }
        };
        match action {
          Some("move") => push(Wait::ms(delay), mv(x, y)),
          Some("down") => push(Wait::ms(delay), down()),
          Some("up") => push(Wait::ms(delay), up(x, y)),
          Some("tap") => {
            press(&mut push);
            push(Wait::frame(hold), up(x, y));
          }
          Some("drag") => {
            let to = ev.get("to").ok_or_else(|| format!("events[{i}]: a drag needs to: {{ x, y }}"))?;
            let to_field = |name: &str| -> Result<f32, String> {
              to.get(name)
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite())
                .map(|v| v as f32)
                .ok_or_else(|| format!("events[{i}]: to.{name} must be a finite number"))
            };
            let (to_x, to_y) = (to_field("x")?, to_field("y")?);
            let duration = if ev.get("durationMs").is_some() { field_ms("durationMs")? } else { DRAG_DEFAULT_MS };
            press(&mut push);
            // One move per frame, the last one at the end point.
            let moves = ((duration as f64 / frame_ms).round() as u32).max(1);
            for step in 1..=moves {
              let t = step as f32 / moves as f32;
              push(Wait::frame(0), mv(x + (to_x - x) * t, y + (to_y - y) * t));
            }
            push(Wait::frame(0), up(to_x, to_y));
            total += duration;
          }
          _ => return Err(format!("events[{i}]: pointer action must be down, up, move, tap or drag")),
        }
      }
      "wheel" => {
        let x = num_field("x")?;
        let y = num_field("y")?;
        let delta_x = num_field("deltaX")?;
        let delta_y = num_field("deltaY")?;
        push(
          Wait::ms(delay),
          AlloyEvent::Wheel {
            pointer_id: SYNTHETIC_POINTER_ID,
            pointer_type: PointerType::Mouse,
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
          },
        );
      }
      "text" => {
        let text = str_field("text")
          .filter(|t| !t.is_empty())
          .ok_or_else(|| format!("events[{i}]: text events need a non-empty text"))?;
        push(Wait::ms(delay), AlloyEvent::TextInput { text: text.to_string() });
      }
      "gamepad" => {
        // A synthetic pad: level state held until the next set, names
        // from the mapped vocabulary (see alloy::gamepad).
        let slot_field = || -> Result<usize, String> {
          ev.get("slot")
            .and_then(|v| v.as_u64())
            .filter(|s| *s < GAMEPAD_SLOT_MAX)
            .map(|s| s as usize)
            .ok_or_else(|| format!("events[{i}]: slot must be an integer 0..{GAMEPAD_SLOT_MAX}"))
        };
        let cmd = match action {
          Some("connect") => {
            let slot = if ev.get("slot").is_some() { Some(slot_field()?) } else { None };
            let name = str_field("name").filter(|n| !n.is_empty()).unwrap_or("Synthetic gamepad").to_string();
            GamepadCommand::Connect { slot, name }
          }
          Some("set") => {
            let slot = slot_field()?;
            let mut buttons = Vec::new();
            if let Some(list) = ev.get("buttons") {
              let list = list.as_array().ok_or_else(|| format!("events[{i}]: buttons must be an array of names"))?;
              for b in list {
                let name = b
                  .as_str()
                  .and_then(alloy::synthetic_button_name)
                  .ok_or_else(|| format!("events[{i}]: unknown gamepad button {b} (south, east, west, north, back, guide, start, leftStick, rightStick, leftShoulder, rightShoulder, dpadUp, dpadDown, dpadLeft, dpadRight)"))?;
                if !buttons.contains(&name) {
                  buttons.push(name);
                }
              }
            }
            let mut axes = Vec::new();
            if let Some(map) = ev.get("axes") {
              let map =
                map.as_object().ok_or_else(|| format!("events[{i}]: axes must be an object of name to value"))?;
              for (name, value) in map {
                let name = alloy::synthetic_axis_name(name).ok_or_else(|| {
                  format!("events[{i}]: unknown gamepad axis {name} (leftX, leftY, rightX, rightY, leftTrigger, rightTrigger)")
                })?;
                let value = value
                  .as_f64()
                  .filter(|v| v.is_finite() && (-1.0..=1.0).contains(v))
                  .ok_or_else(|| format!("events[{i}]: axis {name} must be a number in -1..1"))?;
                axes.push((name, value as f32));
              }
            }
            if ev.get("holdMs").is_some() {
              out.push(Step {
                wait: Wait::ms(delay),
                inject: Injected::Gamepad(GamepadCommand::Set { slot, buttons, axes }),
              });
              GamepadCommand::Set { slot, buttons: Vec::new(), axes: Vec::new() }
            } else {
              GamepadCommand::Set { slot, buttons, axes }
            }
          }
          Some("disconnect") => GamepadCommand::Disconnect { slot: slot_field()? },
          _ => return Err(format!("events[{i}]: gamepad action must be connect, set or disconnect")),
        };
        let at = if ev.get("holdMs").is_some() { hold } else { delay };
        out.push(Step { wait: Wait::ms(at), inject: Injected::Gamepad(cmd) });
      }
      _ => return Err(format!("events[{i}]: type must be key, pointer, wheel, text or gamepad")),
    }
    total += delay + hold;
    if total > INPUT_TOTAL_MAX_MS {
      return Err(format!("Sequence too long: delays and holds total over {INPUT_TOTAL_MAX_MS} ms"));
    }
  }
  Ok(out)
}
