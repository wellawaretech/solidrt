mod context;

pub use context::{LayoutContext, LayoutData};

use taffy::{Position, Style};

/// The style every layout kind starts from (each kind's `initial_style`
/// spreads it): taffy's defaults, except the position. taffy starts a box
/// static, CSS's initial value, under which an absolute box is placed
/// against its nearest positioned ancestor. SolidRT's initial position is
/// relative, as in React Native: every node is the containing block of its
/// absolute children, which are placed in their parent's frame like any
/// other child (what LayoutContext's out-of-flow pass relies on).
/// `LayoutData::positioning_context` separately records a
/// `position="relative"` the app wrote itself.
pub fn base_style() -> Style {
  Style { position: Position::Relative, ..Style::default() }
}

/// Slack a measured size may be exceeded by and still fit, logical px: the
/// tolerance every "does this fit" decision between two derived layout
/// floats compares with. A size solved from a measure is not the measure
/// bit for bit: a shrink-wrapped leaf's width comes back through its
/// parent's border box (measure plus padding, minus padding again) a float
/// ulp off, and two sums of the same advances in a different order differ
/// by one too. An exact comparison then wraps a word inside its own box
/// (okf/done/shrink-wrapped-text-breaks-own-word.md). 1/64 px is a browser
/// layout unit: far above any ulp accumulation, far below a visible
/// overhang. The JS breakers (core's layoutNextLine, 2d's text layout)
/// carry the same value.
pub(crate) const FIT_SLACK: f32 = 1.0 / 64.0;
