// A kinematic character move over the index's volume queries: Godot's
// CharacterBody3D.move_and_slide and Unity's CharacterController.Move as
// one pure function. Sweep the body along its motion, stop a skin short
// of the first contact, slide the rest along the contact plane, repeat a
// few times - with the two passes both engines wrap around that loop: a
// depenetration first (a body that starts inside something is pushed out
// before it moves) and a floor snap after (a body walking down a ramp is
// pulled back onto it instead of stepping into the air). Per body per
// frame that is one call: the loop's ten-odd sweeps and overlaps never
// cross out of the core.
//
// Pure on purpose: no node, no velocity state. It takes a volume where the
// body IS and the motion it WANTS, and returns the motion it gets plus
// what it touched. Gravity is the caller's: fold it into `motion`; a
// walkable floor absorbs the vertical part (a body does not creep down a
// slope it can stand on), a steep one slides it.

use super::collide::{add, scale, sub};
use super::pick::dot;
use super::{Impact, QueryFilter, Spatial, Volume};

type V3 = [f32; 3];

/// Gap the body keeps from every surface, world units (Unity's skinWidth,
/// Godot's safe_margin): what keeps float error from sinking a resting
/// body into its floor.
const SKIN: f32 = 0.01;
/// Contacts within this angle of `up` are floor: 45 degrees, Godot's
/// floor_max_angle and Unity's slopeLimit.
const FLOOR_MAX_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
/// Most contacts one call slides along before it stops where it is
/// (Godot's max_slides).
const MAX_SLIDES: u32 = 6;
/// How far below the body a floor is pulled to while it is not rising
/// (Godot's floor_snap_length).
const FLOOR_SNAP: f32 = 0.1;
/// Passes of the depenetration loop, one contact set pushed out per pass
/// (Godot's recovery attempts).
const MAX_RECOVERIES: u32 = 4;
/// Motion below which a slide stops: nothing left worth sweeping.
const MIN_MOVE: f32 = 1e-6;
/// Floor of the approach cosine the skin is divided by to retreat along
/// the motion: a grazing contact retreats a few skins, not the whole step.
const SKIN_MIN_COS: f32 = 0.1;

/// The mover's tuning; `Default` is the engines' common setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveOptions {
  /// The direction floors face.
  pub up: V3,
  /// Largest angle (radians) between a contact normal and `up` that still
  /// counts as floor. Steeper contacts are walls, and the body slides
  /// down them.
  pub floor_max_angle: f32,
  /// Most contacts one call slides along.
  pub max_slides: u32,
  /// Gap kept from every surface, world units.
  pub skin: f32,
  /// How far below the body a floor is pulled to when the motion does not
  /// rise (0 disables): what keeps a walker on a ramp going down, and what
  /// decides `floor` at the end of the move - a body that ends higher than
  /// this above its floor is airborne, whatever it touched on the way. A
  /// rising motion (a jump) never snaps.
  pub floor_snap: f32,
}

impl Default for MoveOptions {
  fn default() -> Self {
    MoveOptions {
      up: [0.0, 1.0, 0.0],
      floor_max_angle: FLOOR_MAX_ANGLE,
      max_slides: MAX_SLIDES,
      skin: SKIN,
      floor_snap: FLOOR_SNAP,
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveResult {
  /// The displacement the body gets: add it to the body's position.
  pub motion: V3,
  /// The unit normal of the floor the body ends the move on - within
  /// `floor_snap` below it, snapped onto - else None (airborne, or on a
  /// slope too steep to stand on). With `floor_snap` 0 it is a floor met
  /// during the move instead.
  pub floor: Option<V3>,
  /// Whether a wall (a contact steeper than a floor and flatter than a
  /// ceiling) or a ceiling was met.
  pub wall: bool,
  pub ceiling: bool,
  /// Every contact met, in order, the floor snap's last.
  pub hits: Vec<Impact>,
}

impl Spatial {
  /// Move a body `motion` through the nodes `filter` admits, sliding along
  /// what it hits: a capsule for a character, a sphere for a ball, a box
  /// for a crate. The body first pushes out of anything it starts inside,
  /// then sweeps and slides up to `max_slides` times, then, unless the
  /// motion rises, snaps down onto a floor within `floor_snap` - the floor
  /// it reports is the one it ends on. Same index contract as `sweep`.
  pub fn move_and_slide(
    &mut self,
    volume: &Volume,
    motion: V3,
    opts: &MoveOptions,
    filter: &QueryFilter,
  ) -> Result<MoveResult, String> {
    let up = opts.up;
    let floor_cos = opts.floor_max_angle.cos();
    let mut offset: V3 = [0.0; 3];
    let mut hits = Vec::new();
    let mut floor: Option<V3> = None;
    let mut wall = false;
    let mut ceiling = false;

    // Depenetration: every contact's push-out, each only as far as the
    // pushes so far have not already covered along its normal, so two
    // walls of a corner do not push twice.
    for _ in 0..MAX_RECOVERIES {
      let mut push: V3 = [0.0; 3];
      let mut any = false;
      for c in self.overlap(&shifted(volume, offset), filter)? {
        if c.depth <= 0.0 {
          continue;
        }
        any = true;
        let extra = c.depth + opts.skin - dot(push, c.normal);
        if extra > 0.0 {
          push = add(push, scale(c.normal, extra));
        }
      }
      if !any {
        break;
      }
      offset = add(offset, push);
    }

    let mut remaining = motion;
    for _ in 0..opts.max_slides {
      let len = length(remaining);
      if len < MIN_MOVE {
        break;
      }
      let Some(hit) = self.sweep(&shifted(volume, offset), remaining, filter)?.into_iter().next() else {
        offset = add(offset, remaining);
        break;
      };
      let dir = scale(remaining, 1.0 / len);
      let n = hit.normal;
      // Stop where the gap to the surface, measured along its normal, is
      // one skin; the sweep guarantees the approach is positive.
      let retreat = opts.skin / (-dot(dir, n)).max(SKIN_MIN_COS);
      let advance = (hit.time * len - retreat).max(0.0);
      offset = add(offset, scale(dir, advance));
      remaining = scale(dir, len - advance);
      let rise = dot(n, up);
      if rise >= floor_cos {
        // A floor takes the vertical part (the fall, a landing) and passes
        // the horizontal part along its plane.
        floor = Some(n);
        let level = sub(remaining, scale(up, dot(remaining, up)));
        remaining = sub(level, scale(n, dot(level, n)));
      } else {
        if rise <= -floor_cos {
          ceiling = true;
        } else {
          wall = true;
        }
        remaining = sub(remaining, scale(n, dot(remaining, n)));
      }
      hits.push(hit);
    }

    // Where the body ends decides the floor: a contact met on the way
    // (a ramp's top before a big step past its crest) is not one to stand
    // on unless it is still within reach below.
    if opts.floor_snap > 0.0 && dot(motion, up) <= 0.0 {
      floor = None;
      let snap = opts.floor_snap;
      if let Some(hit) = self.sweep(&shifted(volume, offset), scale(up, -snap), filter)?.into_iter().next() {
        if dot(hit.normal, up) >= floor_cos {
          let retreat = opts.skin / dot(hit.normal, up).max(SKIN_MIN_COS);
          offset = add(offset, scale(up, -(hit.time * snap - retreat).max(0.0)));
          floor = Some(hit.normal);
          hits.push(hit);
        }
      }
    }
    Ok(MoveResult { motion: offset, floor, wall, ceiling, hits })
  }
}

/// The volume displaced by `d`.
fn shifted(volume: &Volume, d: V3) -> Volume {
  match *volume {
    Volume::Capsule { a, b, radius } => Volume::Capsule { a: add(a, d), b: add(b, d), radius },
    Volume::Box { center, half, rotation } => Volume::Box { center: add(center, d), half, rotation },
  }
}

fn length(v: V3) -> f32 {
  dot(v, v).sqrt()
}
