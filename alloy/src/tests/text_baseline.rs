// The layout contract of text over a fixed corpus (data/text-corpus.txt:
// the gallery's strings, a changelog release, the layout probe's samples,
// and what apps show besides - numbers, paths, hashes, diacritics, Greek,
// Cyrillic, CJK) in the styles apps draw in: every wrap unit's advance, ink
// width, ascent and descent, and the line breaks of every paragraph at
// three widths. Pinned by data/text-baseline.txt, the engine's own numbers,
// so a harfrust or skrifa bump that moves a break anywhere fails here and
// not in an app. Breaks are pinned in every style, word metrics in one
// style per face (the breaks of the other styles pin their advances
// indirectly), the vertical metrics once per style. Regenerate with
//
//   cargo test -p alloy --lib write_text_baseline -- --ignored
//
// and read the diff: every changed line is a word or a paragraph whose
// layout moved.
use crate::impellers::{FontStyle, FontWeight};
use crate::rendertree::text::layout::{self, Align, LineCursor, LineExtent, Run, Wrap};
use crate::rendertree::text::{prepare_units, Fallback, PreparedUnit, RunStyle};
use crate::rendertree::{FontPayload, PaintState, PlatformContext};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

const CORPUS: &str = include_str!("data/text-corpus.txt");
const BASELINE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/text-baseline.txt");

const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans.ttf");
const NOTO_SERIF: &[u8] = include_bytes!("../../assets/fonts/NotoSerif.ttf");
const NOTO_SANS_MONO: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono.ttf");

// A pinned number may differ from a fresh one by the formatting's rounding,
// never by a visible amount.
const BASELINE_TOLERANCE_PX: f32 = 0.001;
// Wrap widths the breaks are pinned at, logical px: a phone column, a card,
// a reading column.
const WIDTHS: [f32; 3] = [240.0, 400.0, 640.0];
// Decimals a pinned number is written with.
const DECIMALS: usize = 4;

// (family, size, weight, style, line height): body, strong, a muted italic
// caption, a heading, serif prose, code and the HUD's mono, small print.
const STYLES: [(&str, f32, FontWeight, FontStyle, f32); 7] = [
  ("sans", 16.0, FontWeight::Regular, FontStyle::Normal, 0.0),
  ("sans", 16.0, FontWeight::Bold, FontStyle::Normal, 0.0),
  ("sans", 14.0, FontWeight::Medium, FontStyle::Italic, 1.45),
  ("sans", 24.0, FontWeight::SemiBold, FontStyle::Normal, 1.2),
  ("serif", 20.0, FontWeight::Regular, FontStyle::Normal, 0.0),
  ("mono", 14.0, FontWeight::Bold, FontStyle::Normal, 1.55),
  ("sans", 11.0, FontWeight::Regular, FontStyle::Normal, 0.0),
];
// The styles whose word metrics are pinned: one per face.
const WORD_STYLES: [usize; 3] = [0, 4, 5];

fn fonts() -> Vec<FontPayload> {
  let payload =
    |alias: &str, bytes: &'static [u8]| FontPayload { alias: Some(alias.to_string()), bytes: Cow::Borrowed(bytes) };
  vec![payload("sans", NOTO_SANS), payload("serif", NOTO_SERIF), payload("mono", NOTO_SANS_MONO)]
}

fn style(index: usize) -> RunStyle {
  let (family, size, weight, font_style, line_height) = STYLES[index];
  RunStyle {
    font_family: family.to_string(),
    font_size: size,
    font_style,
    font_weight: weight,
    font_stretch: 100.0,
    line_height,
    letter_spacing: 0.0,
    paint: PaintState::default(),
  }
}

fn paragraphs() -> Vec<&'static str> {
  CORPUS.split("\n\n").map(|p| p.trim_matches('\n')).filter(|p| !p.is_empty()).collect()
}

fn runs_of(units: &[PreparedUnit]) -> Vec<Run> {
  units
    .iter()
    .map(|u| Run { metrics: u.metrics, hard_break: u.hard_break, glue: u.glue, float: None, clear: None })
    .collect()
}

// The index of the first unit on each line when `units` wrap at `width`.
fn breaks(units: &[PreparedUnit], width: f32) -> Vec<usize> {
  let extent = move |_: LineCursor| vec![LineExtent::full(width)];
  let layout = layout::layout_wrap(&runs_of(units), &extent, Align::Left, 0, None, Wrap::Wrap);
  layout.lines.iter().map(|l| l.first).collect()
}

// The baseline's lines for the engine's shaping of the corpus: `m <style>\t
// <ascent> <descent>` once per style, `u <style> <word>\t<advance> <ink>`
// once per distinct word of a word style, then `l <style> <paragraph>
// <width>\t<first unit of every line>`.
fn baseline_lines(platform: &PlatformContext) -> Vec<String> {
  let mut lines = Vec::new();
  for (s, _) in STYLES.iter().enumerate() {
    let style = style(s);
    let pin_words = WORD_STYLES.contains(&s);
    let mut seen: HashSet<String> = HashSet::new();
    let mut layouts = Vec::new();
    for (p, text) in paragraphs().iter().enumerate() {
      let units = prepare_units(platform, text, &style, &[], false, Fallback::Registered);
      if let (0, Some(unit)) = (p, units.first()) {
        let m = unit.metrics;
        lines.push(format!("m {s}\t{:.*} {:.*}", DECIMALS, m.ascent, DECIMALS, m.descent));
      }
      if pin_words {
        for unit in &units {
          if seen.insert(unit.text.clone()) {
            let m = unit.metrics;
            lines.push(format!("u {s} {:?}\t{:.*} {:.*}", unit.text, DECIMALS, m.advance, DECIMALS, m.ink_width));
          }
        }
      }
      for width in WIDTHS {
        let firsts: Vec<String> = breaks(&units, width).iter().map(usize::to_string).collect();
        layouts.push(format!("l {s} {p} {width}\t{}", firsts.join(" ")));
      }
    }
    lines.extend(layouts);
  }
  lines
}

fn split_line(line: &str) -> (&str, &str) {
  line.split_once('\t').unwrap_or_else(|| panic!("baseline line without a tab: {line}"))
}

fn numbers(values: &str) -> Vec<f32> {
  values.split(' ').map(|v| v.parse::<f32>().unwrap_or_else(|_| panic!("not a number: {v}"))).collect()
}

/// Write the baseline for the current engine. Ignored: run by hand after a
/// deliberate change, and commit the diff with the change that caused it.
#[test]
#[ignore]
fn write_text_baseline() {
  let platform = PlatformContext::new(fonts());
  let mut out = String::new();
  for line in baseline_lines(&platform) {
    writeln!(out, "{line}").expect("write to a string");
  }
  std::fs::write(BASELINE_PATH, out).expect("the baseline file is writable");
}

#[test]
fn engine_matches_its_baseline() {
  let platform = PlatformContext::new(fonts());
  let pinned = std::fs::read_to_string(BASELINE_PATH)
    .unwrap_or_else(|e| panic!("no text baseline at {BASELINE_PATH} ({e}); run write_text_baseline"));
  let pinned: HashMap<&str, &str> = pinned.lines().map(split_line).collect();
  let fresh = baseline_lines(&platform);
  let mut mismatches = Vec::new();
  for line in &fresh {
    let (key, values) = split_line(line);
    let Some(expected) = pinned.get(key) else {
      mismatches.push(format!("missing from the baseline: {key}"));
      continue;
    };
    let same = if key.starts_with("u ") || key.starts_with("m ") {
      numbers(values).iter().zip(numbers(expected)).all(|(a, b)| (a - b).abs() <= BASELINE_TOLERANCE_PX)
    } else {
      values == *expected
    };
    if !same {
      mismatches.push(format!("{key}: pinned {expected}, now {values}"));
    }
  }
  if fresh.len() != pinned.len() {
    mismatches.push(format!("{} lines pinned, {} now", pinned.len(), fresh.len()));
  }
  assert!(
    mismatches.is_empty(),
    "{} of {} baseline lines moved (a deliberate change regenerates the baseline, see the file's header):\n{}",
    mismatches.len(),
    fresh.len(),
    mismatches.join("\n")
  );
}
