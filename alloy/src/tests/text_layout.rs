use crate::rendertree::text_layout::{
  layout, max_intrinsic_width, min_intrinsic_width, segments, Align, LineCursor, LineExtent, PlacedRun, Run, RunMetrics,
  Clear, Side,
};

fn full(width: f32) -> impl Fn(LineCursor) -> Vec<LineExtent> {
  move |_| vec![LineExtent::full(width)]
}

fn word(advance: f32, ink: f32) -> Run {
  Run { metrics: RunMetrics { advance, ink_width: ink, ascent: 8.0, descent: 2.0 }, hard_break: false, glue: false, float: None, clear: None }
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
  let l = layout(&runs, &full(22.0), Align::Left, 0, None);
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
  let l = layout(&runs, &full(100.0), Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[1].y, 10.0);
  let capped = layout(&runs, &full(100.0), Align::Left, 1, None);
  assert_eq!(capped.lines.len(), 1);
  assert_eq!(capped.runs.len(), 1);
}

#[test]
fn oversized_run_gets_its_own_line() {
  let runs = [word(5.0, 4.0), word(50.0, 50.0), word(5.0, 4.0)];
  let l = layout(&runs, &full(20.0), Align::Left, 0, None);
  assert_eq!(l.lines.len(), 3);
}

#[test]
fn baseline_alignment_and_right_align() {
  let tall =
    Run { metrics: RunMetrics { advance: 10.0, ink_width: 10.0, ascent: 16.0, descent: 4.0 }, ..word(0.0, 0.0) };
  let runs = [word(10.0, 10.0), tall];
  let l = layout(&runs, &full(40.0), Align::Right, 0, None);
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
  let l = layout(&runs, &full(20.0), Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[1].x, 10.0);
  assert_eq!(l.runs[2].y, 10.0);
  // A glued piece never starts a line, even when its own ink does not fit.
  let runs = [word(10.0, 10.0), glued(20.0, 20.0)];
  let l = layout(&runs, &full(12.0), Align::Left, 0, None);
  assert_eq!(l.lines.len(), 1);
  assert_eq!(min_intrinsic_width(&runs), 30.0);
}

#[test]
fn justify_spreads_slack_over_wrapped_lines_only() {
  // Two lines of two units each; the first wraps (justified), the second is
  // last (left).
  let runs = [word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, &full(30.0), Align::Justify, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.runs[0].x, 0.0);
  assert_eq!(l.runs[1].x, 20.0);
  assert_eq!(l.lines[0].segments[0].ink, 30.0);
  assert_eq!(l.runs[2].x, 0.0);
  assert_eq!(l.runs[3].x, 12.0);
  // A glued piece moves with its unit, it does not open a gap.
  let runs = [word(10.0, 10.0), glued(2.0, 2.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, &full(30.0), Align::Justify, 0, None);
  assert_eq!(l.runs[1].x, 10.0);
  assert_eq!(l.runs[2].x, 20.0);
}

#[test]
fn max_lines_truncates_and_ellipsis_trims_last_line() {
  let ell = RunMetrics { advance: 6.0, ink_width: 6.0, ascent: 8.0, descent: 2.0 };
  let runs = [word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  // Two units per line at 22; capped at one line, "w w" + ellipsis needs 28.
  let l = layout(&runs, &full(22.0), Align::Left, 1, Some(ell));
  assert!(l.truncated);
  assert_eq!(l.runs.len(), 1);
  assert_eq!(l.ellipsis, Some((10.0, 0.0)));
  assert_eq!(l.lines[0].segments[0].ink, 16.0);
  // Right-aligned: the trimmed line plus ellipsis moves as one.
  let l = layout(&runs, &full(22.0), Align::Right, 1, Some(ell));
  assert_eq!(l.runs[0].x, 6.0);
  assert_eq!(l.ellipsis, Some((16.0, 0.0)));
  // Without an ellipsis the line is simply cut.
  let l = layout(&runs, &full(22.0), Align::Left, 1, None);
  assert!(l.truncated);
  assert_eq!(l.runs.len(), 2);
  assert_eq!(l.ellipsis, None);
  // Everything fits: not truncated, no ellipsis.
  let l = layout(&runs, &full(100.0), Align::Left, 1, Some(ell));
  assert!(!l.truncated);
  assert_eq!(l.ellipsis, None);
  // A hard break at the cap with more text after it truncates too.
  let runs = [hard(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, &full(100.0), Align::Left, 1, Some(ell));
  assert!(l.truncated);
  assert_eq!(l.ellipsis, Some((10.0, 0.0)));
}

#[test]
fn oversized_units_are_reported() {
  let runs = [word(5.0, 4.0), word(50.0, 50.0), glued(3.0, 3.0), word(5.0, 4.0)];
  let l = layout(&runs, &full(20.0), Align::Left, 0, None);
  assert_eq!(l.overflowing, vec![1]);
  let l = layout(&runs, &full(60.0), Align::Left, 0, None);
  assert!(l.overflowing.is_empty());
}

#[test]
fn per_line_extent_shifts_and_narrows_lines() {
  // First line indented by 5 (width 25); the rest use the full 34. Ink 10,
  // gap 2: two units fit the indented line (22 <= 25), three the full one.
  let runs = [word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  let asked = std::cell::RefCell::new(Vec::new());
  let extent = |c: LineCursor| {
    asked.borrow_mut().push((c.index, c.y, c.height));
    if c.index == 0 {
      vec![LineExtent { x: 5.0, width: 25.0 }]
    } else {
      vec![LineExtent::full(34.0)]
    }
  };
  let l = layout(&runs, &extent, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.lines[0].segments[0].x, 5.0);
  assert_eq!(l.runs[0].x, 5.0);
  assert_eq!(l.runs[1].x, 17.0);
  assert_eq!(l.runs[2].x, 0.0);
  assert_eq!(l.runs[4].x, 24.0);
  assert_eq!(l.width, 34.0);
  // Asked once per line, at the line's top, with the opening run's height.
  assert_eq!(*asked.borrow(), vec![(0, 0.0, 10.0), (1, 10.0, 10.0)]);
  // Alignment works inside the line's extent: right-aligned, the indented
  // line's last ink ends at 30 and the full line's at 34.
  let l = layout(&runs, &extent, Align::Right, 0, None);
  assert_eq!(l.runs[1].x + 10.0, 30.0);
  assert_eq!(l.runs[4].x + 10.0, 34.0);
}

#[test]
fn segments_split_a_line_around_an_exclusion() {
  // Line 0 is cut in two around a box at 24..40: segments 0..24 and 40..64.
  // Line 1 has no room at all (skipped, y advances by the cursor height) and
  // line 2 is the full 64. Ink 10, gap 2: two units per 24-wide segment.
  let runs: Vec<Run> = (0..7).map(|_| word(12.0, 10.0)).collect();
  let extent = |c: LineCursor| match c.index {
    0 => vec![LineExtent { x: 0.0, width: 24.0 }, LineExtent { x: 40.0, width: 24.0 }],
    1 if c.y < 20.0 => Vec::new(),
    _ => vec![LineExtent::full(64.0)],
  };
  let l = layout(&runs, &extent, Align::Left, 0, None);
  assert_eq!(l.lines.len(), 2);
  assert_eq!(l.lines[0].segments.len(), 2);
  assert_eq!(l.runs[1].x, 12.0);
  assert_eq!(l.runs[2].x, 40.0);
  assert_eq!(l.runs[3].x, 52.0);
  // Both segments share the line's y; the segments record their runs.
  assert_eq!(l.runs[3].y, 0.0);
  assert_eq!(l.lines[0].segments[1].first, 2);
  assert_eq!(l.lines[0].segments[1].end, 4);
  // The skipped line: line 1 opens at y 20 after one 10-high skip.
  assert_eq!(l.lines[1].y, 20.0);
  assert_eq!(l.runs[4].x, 0.0);
  assert_eq!(l.runs[6].x, 24.0);
  assert_eq!(l.height, 30.0);
  assert_eq!(l.width, 62.0);
  // Justify works per segment: the first segment (overflowed) spreads its
  // slack, so does the second when the line wraps after it.
  let l = layout(&runs, &extent, Align::Justify, 0, None);
  assert_eq!(l.runs[1].x, 14.0);
  assert_eq!(l.runs[3].x, 54.0);
  // A unit too wide for the first segment skips to the second even when the
  // first is empty; wider than every segment, it overflows the last one.
  let wide_second = |_: LineCursor| vec![LineExtent { x: 0.0, width: 24.0 }, LineExtent { x: 40.0, width: 30.0 }];
  let runs = [word(30.0, 30.0), word(70.0, 70.0)];
  let l = layout(&runs, &wide_second, Align::Left, 0, None);
  assert_eq!(l.runs[0].x, 40.0);
  assert_eq!(l.lines[0].segments[0].end, 0);
  assert_eq!(l.runs[1].x, 40.0);
  assert_eq!(l.runs[1].y, 10.0);
  assert_eq!(l.overflowing, vec![1]);
}

fn floated(width: f32, height: f32, side: Side) -> Run {
  Run {
    metrics: RunMetrics { advance: width, ink_width: width, ascent: height, descent: 0.0 },
    float: Some(side),
    ..word(0.0, 0.0)
  }
}

#[test]
fn floats_leave_the_flow_and_cut_the_lines_they_overlap() {
  // A 20x25 left float first, then words of ink 10 (gap 2) in a 64 column:
  // the float sits at (0,0); lines 0-2 (10 high each) start at 20 and hold
  // three units (20 + 34 <= 64), line 3 has the full width again.
  let mut runs = vec![floated(20.0, 25.0, Side::Left)];
  runs.extend((0..11).map(|_| word(12.0, 10.0)));
  let l = layout(&runs, &full(64.0), Align::Left, 0, None);
  assert_eq!(l.floats, vec![PlacedRun { run: 0, x: 0.0, y: 0.0 }]);
  assert_eq!(l.lines.len(), 4);
  assert_eq!(l.runs[0].x, 20.0);
  assert_eq!(l.runs[2].x, 44.0);
  assert_eq!(l.runs[3].y, 10.0);
  assert_eq!(l.runs[6].x, 20.0);
  assert_eq!(l.runs[6].y, 20.0);
  assert_eq!(l.lines[3].segments[0].x, 0.0);
  assert_eq!(l.runs[9].x, 0.0);
  assert_eq!(l.runs[9].y, 30.0);
  assert_eq!(l.height, 40.0);
  // The float and a line's runs are not the same list.
  assert_eq!(l.runs.len(), 11);

  // A right float met mid-line waits for the next line top; the text grows
  // to its bottom when it hangs below the last line.
  let runs = [word(12.0, 10.0), floated(20.0, 30.0, Side::Right), word(12.0, 10.0), word(12.0, 10.0), word(12.0, 10.0)];
  let l = layout(&runs, &full(30.0), Align::Left, 0, None);
  assert_eq!(l.floats, vec![PlacedRun { run: 1, x: 10.0, y: 10.0 }]);
  assert_eq!(l.runs[1].x, 12.0);
  assert_eq!(l.runs[2].x, 0.0);
  assert_eq!(l.runs[2].y, 10.0);
  // 10 wide left of the float: one unit per line.
  assert_eq!(l.runs[3].y, 20.0);
  assert_eq!(l.height, 40.0);

  // Two left floats on one line top sit beside each other; a line with no
  // room between them and a right float is skipped past the shorter one.
  let runs = [floated(20.0, 20.0, Side::Left), floated(20.0, 10.0, Side::Left), floated(30.0, 10.0, Side::Right), word(12.0, 10.0)];
  let l = layout(&runs, &full(70.0), Align::Left, 0, None);
  assert_eq!(l.floats[1].x, 20.0);
  assert_eq!(l.floats[2].x, 40.0);
  // y 0..10 leaves 40..40: nothing; y 10..20 leaves 20..70.
  assert_eq!(l.runs[0].x, 20.0);
  assert_eq!(l.runs[0].y, 10.0);
}

#[test]
fn clear_starts_below_earlier_floats() {
  // A 20x30 left float, one word beside it, then a cleared word: it starts a
  // new line at the float's bottom, full width. A cleared float goes below
  // the earlier one instead of beside it.
  let cleared = Run { clear: Some(Clear::Left), ..word(12.0, 10.0) };
  let runs = [floated(20.0, 30.0, Side::Left), word(12.0, 10.0), cleared, word(12.0, 10.0)];
  let l = layout(&runs, &full(64.0), Align::Left, 0, None);
  assert_eq!(l.runs[0].x, 20.0);
  assert_eq!(l.runs[1].x, 0.0);
  assert_eq!(l.runs[1].y, 30.0);
  assert_eq!(l.runs[2].x, 12.0);
  // Clearing the other side still starts a new line, just not lower.
  let other = Run { clear: Some(Clear::Right), ..word(12.0, 10.0) };
  let runs = [floated(20.0, 30.0, Side::Left), word(12.0, 10.0), other];
  let l = layout(&runs, &full(64.0), Align::Left, 0, None);
  assert_eq!(l.runs[1].y, 10.0);
  assert_eq!(l.runs[1].x, 20.0);
  let stacked = Run { clear: Some(Clear::Both), ..floated(20.0, 10.0, Side::Left) };
  let runs = [floated(20.0, 30.0, Side::Left), stacked, word(12.0, 10.0)];
  let l = layout(&runs, &full(64.0), Align::Left, 0, None);
  assert_eq!(l.floats[1], PlacedRun { run: 1, x: 0.0, y: 30.0 });
  assert_eq!(l.runs[0].x, 20.0);
  assert_eq!(l.runs[0].y, 30.0);
}
