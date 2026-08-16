use crate::rendertree::text_layout::{
  layout, max_intrinsic_width, min_intrinsic_width, segments, Align, Run, RunMetrics,
};

fn word(advance: f32, ink: f32) -> Run {
  Run { metrics: RunMetrics { advance, ink_width: ink, ascent: 8.0, descent: 2.0 }, hard_break: false, glue: false }
}

fn glued(advance: f32, ink: f32) -> Run {
  Run { glue: true, ..word(advance, ink) }
}

fn hard(advance: f32, ink: f32) -> Run {
  Run { hard_break: true, ..word(advance, ink) }
}

#[test]
fn segments_keep_trailing_whitespace_and_flag_hard_breaks() {
  let text = "Hello world\nfoo";
  let segs = segments(text);
  let parts: Vec<(&str, bool)> = segs.iter().map(|s| (&text[s.start..s.end], s.hard_break)).collect();
  assert_eq!(parts, vec![("Hello ", false), ("world\n", true), ("foo", false)]);
}

#[test]
fn segments_split_between_ideographs() {
  let text = "\u{4f60}\u{597d}";
  assert_eq!(segments(text).len(), 2);
  assert!(segments("").is_empty());
}

#[test]
fn greedy_breaks_on_ink_not_advance() {
  // Three words of ink 10 with a 2px gap: two fit in 22 only because the
  // trailing gap of the second hangs past the edge.
  let runs = [word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, 22.0, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[1].x, 12.0);
  assert_eq!(l.runs[2].x, 0.0);
  assert_eq!(l.runs[2].y, 10.0);
  assert_eq!(l.width, 22.0);
  assert_eq!(l.height, 20.0);
}

#[test]
fn hard_break_ends_line_and_max_lines_caps() {
  let runs = [hard(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, 100.0, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[1].y, 10.0);
  let capped = layout(&runs, 100.0, Align::Left, 1, None);
  assert_eq!(capped.lines.len(), 1);
  assert_eq!(capped.runs.len(), 1);
}

#[test]
fn oversized_run_gets_its_own_line() {
  let runs = [word(5.0, 4.0), word(50.0, 50.0), word(5.0, 4.0)];
  let l = layout(&runs, 20.0, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 3);
}

#[test]
fn baseline_alignment_and_right_align() {
  let tall =
    Run { metrics: RunMetrics { advance: 10.0, ink_width: 10.0, ascent: 16.0, descent: 4.0 }, ..word(0.0, 0.0) };
  let runs = [word(10.0, 10.0), tall];
  let l = layout(&runs, 40.0, Align::Right, 0, None);
  assert_eq!(l.lines[0].height, 20.0);
  // Small run's top drops so its baseline meets the tall run's.
  assert_eq!(l.runs[0].y, 8.0);
  assert_eq!(l.runs[1].y, 0.0);
  assert_eq!(l.runs[0].x, 20.0);
}

#[test]
fn intrinsic_widths() {
  let runs = [word(12.0, 10.0), hard(12.0, 10.0), word(30.0, 30.0)];
  assert_eq!(max_intrinsic_width(&runs), 30.0);
  assert_eq!(min_intrinsic_width(&runs), 30.0);
  let one_line = [word(12.0, 10.0), word(12.0, 10.0)];
  assert_eq!(max_intrinsic_width(&one_line), 22.0);
}

#[test]
fn glued_pieces_break_as_one_unit() {
  // "foo" + "," (glued) + "bar": the unit foo, is 14 wide; at 20 the second
  // unit must wrap even though "foo" alone would leave room for "bar"'s ink.
  let runs = [word(10.0, 10.0), glued(6.0, 4.0), word(10.0, 8.0)];
  let l = layout(&runs, 20.0, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[1].x, 10.0);
  assert_eq!(l.runs[2].y, 10.0);
  // A glued piece never starts a line, even when its own ink does not fit.
  let runs = [word(10.0, 10.0), glued(20.0, 20.0)];
  let l = layout(&runs, 12.0, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 1);
  assert_eq!(min_intrinsic_width(&runs), 30.0);
}

#[test]
fn justify_spreads_slack_over_wrapped_lines_only() {
  // Two lines of two units each; the first wraps (justified), the second is
  // last (left).
  let runs = [word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, 30.0, Align::Justify, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[0].x, 0.0);
  assert_eq!(l.runs[1].x, 20.0);
  assert_eq!(l.lines[0].width, 30.0);
  assert_eq!(l.runs[2].x, 0.0);
  assert_eq!(l.runs[3].x, 12.0);
  // A glued piece moves with its unit, it does not open a gap.
  let runs = [word(10.0, 10.0), glued(2.0, 2.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, 30.0, Align::Justify, 0, None);
  assert_eq!(l.runs[1].x, 10.0);
  assert_eq!(l.runs[2].x, 20.0);
}

#[test]
fn max_lines_truncates_and_ellipsis_trims_last_line() {
  let ell = RunMetrics { advance: 6.0, ink_width: 6.0, ascent: 8.0, descent: 2.0 };
  let runs = [word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  // Two units per line at 22; capped at one line, "w w" + ellipsis needs 28.
  let l = layout(&runs, 22.0, Align::Left, 1, Some(ell));
  assert!(l.truncated);
  assert_eq!(l.runs.len(), 1);
  assert_eq!(l.ellipsis, Some((10.0, 0.0)));
  assert_eq!(l.lines[0].width, 16.0);
  // Right-aligned: the trimmed line plus ellipsis moves as one.
  let l = layout(&runs, 22.0, Align::Right, 1, Some(ell));
  assert_eq!(l.runs[0].x, 6.0);
  assert_eq!(l.ellipsis, Some((16.0, 0.0)));
  // Without an ellipsis the line is simply cut.
  let l = layout(&runs, 22.0, Align::Left, 1, None);
  assert!(l.truncated);
  assert_eq!(l.runs.len(), 2);
  assert_eq!(l.ellipsis, None);
  // Everything fits: not truncated, no ellipsis.
  let l = layout(&runs, 100.0, Align::Left, 1, Some(ell));
  assert!(!l.truncated);
  assert_eq!(l.ellipsis, None);
  // A hard break at the cap with more text after it truncates too.
  let runs = [hard(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, 100.0, Align::Left, 1, Some(ell));
  assert!(l.truncated);
  assert_eq!(l.ellipsis, Some((10.0, 0.0)));
}

#[test]
fn oversized_units_are_reported() {
  let runs = [word(5.0, 4.0), word(50.0, 50.0), glued(3.0, 3.0), word(5.0, 4.0)];
  let l = layout(&runs, 20.0, Align::Left, 0, None);
  assert_eq!(l.overflowing, vec![1]);
  let l = layout(&runs, 60.0, Align::Left, 0, None);
  assert!(l.overflowing.is_empty());
}
