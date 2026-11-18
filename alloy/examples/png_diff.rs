//! Counts the pixels that differ between two PNGs, or between the frames of
//! two `sol render` output directories paired by name, and writes a diff
//! image that marks them (okf/plans/own-renderer.md, stage 1: the pixels a
//! renderer draws are diffed against main's binary, reported, never
//! asserted). Needs no GL context.
//!
//! A pixel differs when any channel's absolute difference exceeds the
//! tolerance. Per pair it prints the count, the share of the frame, the
//! largest channel difference and the bounding box of the differing pixels.
//! The diff image is the first image in dimmed grey with every differing
//! pixel marked, so the marks read at a glance. The exit code is 0 whenever
//! the comparison ran, whatever it counted: 1 means a file could not be read
//! or written, 2 a bad invocation.
//!
//! Run: cargo run -p alloy --example png_diff -- <a> <b> [diff] [--tolerance N]

use image::RgbaImage;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// How many levels a channel may differ by before its pixel counts; 0 counts
/// every change, as a strict comparison does.
const DEFAULT_TOLERANCE: u8 = 0;
/// How far the unchanged pixels of the diff image recede (a factor on their
/// luma), so the marks read against them.
const DIM_FACTOR: f32 = 0.35;
/// The color a differing pixel is marked with in the diff image.
const MARKER: [u8; 4] = [255, 0, 0, 255];
/// The frame files `sol render` writes (`frame-000000.png`, ...).
const FRAME_PREFIX: &str = "frame-";
const USAGE: &str = "Usage: cargo run -p alloy --example png_diff -- <a> <b> [diff] [--tolerance N]";

struct Args {
  a: PathBuf,
  b: PathBuf,
  diff: Option<PathBuf>,
  tolerance: u8,
}

struct Outcome {
  differing: u64,
  total: u64,
  max_difference: u8,
  /// Inclusive pixel ranges of the differing pixels: (x min, x max, y min, y max).
  bounds: Option<(u32, u32, u32, u32)>,
}

fn parse_args() -> Result<Args, String> {
  let mut positional = Vec::new();
  let mut tolerance = DEFAULT_TOLERANCE;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    if arg == "--tolerance" {
      let value = args.next().ok_or("Missing value for --tolerance")?;
      tolerance = value.parse().map_err(|_| format!("Invalid tolerance {value}: expected 0 to 255"))?;
    } else if arg.starts_with("--") {
      return Err(format!("Unknown option {arg}"));
    } else {
      positional.push(PathBuf::from(arg));
    }
  }
  if positional.len() < 2 || positional.len() > 3 {
    return Err("Expected two inputs and an optional diff output".into());
  }
  let diff = positional.get(2).cloned();
  let b = positional.remove(1);
  let a = positional.remove(0);
  Ok(Args { a, b, diff, tolerance })
}

fn load(path: &Path) -> Result<RgbaImage, String> {
  image::open(path).map(|image| image.to_rgba8()).map_err(|e| format!("Cannot read {}: {e}", path.display()))
}

/// The diff image's base: `a` as dimmed grey, opaque.
fn dimmed(a: &RgbaImage) -> RgbaImage {
  let mut base = a.clone();
  for pixel in base.pixels_mut() {
    let [r, g, b, _] = pixel.0;
    // Rec. 709 luma weights.
    let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    let level = (luma * DIM_FACTOR).round() as u8;
    pixel.0 = [level, level, level, u8::MAX];
  }
  base
}

fn compare(a: &RgbaImage, b: &RgbaImage, tolerance: u8, mut diff: Option<&mut RgbaImage>) -> Outcome {
  let mut outcome =
    Outcome { differing: 0, total: u64::from(a.width()) * u64::from(a.height()), max_difference: 0, bounds: None };
  for (x, y, pa) in a.enumerate_pixels() {
    let pb = b.get_pixel(x, y);
    let difference = pa.0.iter().zip(pb.0.iter()).map(|(ca, cb)| ca.abs_diff(*cb)).max().unwrap_or(0);
    outcome.max_difference = outcome.max_difference.max(difference);
    if difference <= tolerance {
      continue;
    }
    outcome.differing += 1;
    outcome.bounds = Some(match outcome.bounds {
      None => (x, x, y, y),
      Some((x0, x1, y0, y1)) => (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
    });
    if let Some(image) = diff.as_deref_mut() {
      image.put_pixel(x, y, image::Rgba(MARKER));
    }
  }
  outcome
}

fn report(name: &str, outcome: &Outcome) {
  let percent = outcome.differing as f64 * 100.0 / outcome.total.max(1) as f64;
  let mut line = format!(
    "{name}: {} of {} pixels differ ({percent:.2}%), max channel difference {}",
    outcome.differing, outcome.total, outcome.max_difference
  );
  if let Some((x0, x1, y0, y1)) = outcome.bounds {
    line += &format!(", within x {x0}..{x1}, y {y0}..{y1}");
  }
  println!("{line}");
}

/// The sorted names of the frame files in `dir`.
fn frames(dir: &Path) -> Result<Vec<String>, String> {
  let entries = std::fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
  let mut names: Vec<String> = entries
    .filter_map(|entry| entry.ok())
    .filter_map(|entry| entry.file_name().into_string().ok())
    .filter(|name| name.starts_with(FRAME_PREFIX) && name.ends_with(".png"))
    .collect();
  names.sort();
  Ok(names)
}

/// Compares one pair and writes its diff image; `None` when the sizes differ.
fn pair(name: &str, a: &Path, b: &Path, diff: Option<&Path>, tolerance: u8) -> Result<Option<Outcome>, String> {
  let (a, b) = (load(a)?, load(b)?);
  if a.dimensions() != b.dimensions() {
    println!("{name}: sizes differ: {}x{} vs {}x{}", a.width(), a.height(), b.width(), b.height());
    return Ok(None);
  }
  let mut image = diff.map(|_| dimmed(&a));
  let outcome = compare(&a, &b, tolerance, image.as_mut());
  report(name, &outcome);
  if let (Some(path), Some(image)) = (diff, image) {
    image.save(path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
  }
  Ok(Some(outcome))
}

fn directories(args: &Args) -> Result<(), String> {
  let (in_a, in_b) = (frames(&args.a)?, frames(&args.b)?);
  if let Some(dir) = &args.diff {
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
  }
  let mut names: Vec<&String> = in_a.iter().chain(in_b.iter()).collect();
  names.sort();
  names.dedup();
  let (mut compared, mut differing, mut total) = (0u64, 0u64, 0u64);
  for name in names {
    let missing = if !in_a.contains(name) { Some(&args.a) } else { in_b.iter().all(|n| n != name).then_some(&args.b) };
    if let Some(dir) = missing {
      println!("{name}: missing in {}", dir.display());
      continue;
    }
    let diff = args.diff.as_ref().map(|dir| dir.join(name));
    if let Some(outcome) = pair(name, &args.a.join(name), &args.b.join(name), diff.as_deref(), args.tolerance)? {
      compared += 1;
      differing += outcome.differing;
      total += outcome.total;
    }
  }
  println!("Total: {compared} frames, {differing} pixels differ of {total}");
  Ok(())
}

fn main() -> ExitCode {
  let args = match parse_args() {
    Ok(args) => args,
    Err(message) => {
      eprintln!("{message}\n{USAGE}");
      return ExitCode::from(2);
    }
  };
  let result = match (args.a.is_dir(), args.b.is_dir()) {
    (true, true) => directories(&args),
    (false, false) => {
      let name = args.a.file_name().map_or_else(|| args.a.display().to_string(), |n| n.to_string_lossy().into_owned());
      pair(&name, &args.a, &args.b, args.diff.as_deref(), args.tolerance).map(|_| ())
    }
    _ => {
      eprintln!("Cannot compare a file with a directory\n{USAGE}");
      return ExitCode::from(2);
    }
  };
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(message) => {
      eprintln!("{message}");
      ExitCode::FAILURE
    }
  }
}
