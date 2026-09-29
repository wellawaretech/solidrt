// The KTX2 codec round trip over the vendored Basis Universal: encode with
// each codec, transcode to each target, and check the container facts
// (size, levels, sRGB flag), the payload sizes against the block math and
// the decoded color of a solid image.

use crate::ktx2::{encode, is_ktx2, transcode, Codec, EncodeOptions, MipWrap, Target, DEFAULT_QUALITY};

/// The test image edge: 2x2 blocks, a four-level chain (8, 4, 2, 1).
const SIZE: u32 = 8;
const LEVELS: u32 = 4;
/// The edge of the image the file size test encodes: enough blocks (256)
/// that the container's header does not decide the comparison.
const LARGE_SIZE: u32 = 64;
/// The Khronos fixture: 1024x1024 with its full chain.
const FOREIGN_SIZE: u32 = 1024;
const FOREIGN_LEVELS: u32 = 11;
/// The qualities the measurement compares. For UASTC, 1.0 is the encoder's
/// best blocks and everything below it turns rate-distortion optimization
/// on, stronger the lower it goes.
const MEASURED_QUALITY: [f32; 5] = [0.25, 0.5, 0.75, 0.9, 1.0];
// How many encodes measure_encodes_at_once runs at once.
const MEASURED_AT_ONCE: [usize; 4] = [1, 2, 4, 8];
/// Bytes of the four-level chain in a block format: 4 + 1 + 1 + 1 blocks.
const CHAIN_BLOCK_BYTES: usize = 7 * 16;
/// Bytes of the four-level chain as rgba8: 64 + 16 + 4 + 1 pixels.
const CHAIN_RGBA_BYTES: usize = 85 * 4;
/// How far a decoded channel of a solid color may sit from its source: the
/// codecs quantize endpoints (ETC1S to 5 bits).
const TOLERANCE: i32 = 8;

const SOLID: [u8; 4] = [200, 100, 50, 255];

fn solid() -> Vec<u8> {
  SOLID.repeat((SIZE * SIZE) as usize)
}

fn options(codec: Codec, srgb: bool, mipmap: bool) -> EncodeOptions {
  EncodeOptions { codec, srgb, mipmap, wrap: MipWrap::Clamp, quality: DEFAULT_QUALITY }
}

#[test]
fn round_trips_each_codec_to_each_target() {
  for codec in [Codec::Etc1s, Codec::Uastc] {
    let file = encode(&solid(), SIZE, SIZE, &options(codec, false, true)).expect("encode");
    assert!(is_ktx2(&file), "{codec:?} output is a KTX2 file");
    for (target, bytes, name) in [
      (Target::Etc2Rgba8, CHAIN_BLOCK_BYTES, "etc2-rgba8"),
      (Target::Bc7Rgba8, CHAIN_BLOCK_BYTES, "bc7-rgba8"),
      (Target::Rgba8, CHAIN_RGBA_BYTES, "rgba8"),
    ] {
      let out = transcode(&file, target).expect("transcode");
      assert_eq!((out.width, out.height, out.levels), (SIZE, SIZE, LEVELS), "{codec:?} to {name}");
      assert_eq!(out.data.len(), bytes, "{codec:?} to {name}");
      assert!(!out.srgb);
      assert_eq!(out.format_name(), name);
    }
  }
}

#[test]
fn decodes_the_color_it_encoded() {
  for codec in [Codec::Etc1s, Codec::Uastc] {
    let file = encode(&solid(), SIZE, SIZE, &options(codec, false, false)).expect("encode");
    let out = transcode(&file, Target::Rgba8).expect("transcode");
    assert_eq!(out.levels, 1);
    assert_eq!(out.data.len(), (SIZE * SIZE * 4) as usize);
    for pixel in out.data.chunks_exact(4) {
      for (got, want) in pixel.iter().zip(SOLID) {
        assert!((*got as i32 - want as i32).abs() <= TOLERANCE, "{codec:?}: {pixel:?} against {SOLID:?}");
      }
    }
  }
}

#[test]
fn srgb_flag_travels_in_the_file() {
  let file = encode(&solid(), SIZE, SIZE, &options(Codec::Etc1s, true, true)).expect("encode");
  let out = transcode(&file, Target::Bc7Rgba8).expect("transcode");
  assert!(out.srgb);
  assert_eq!(out.format_name(), "bc7-rgba8-srgb");
  assert_eq!(transcode(&file, Target::Etc2Rgba8).expect("transcode").format_name(), "etc2-rgba8-srgb");
}

// UASTC is 16 bytes per block in the file as on the GPU; what makes the file
// smaller than the payload is the zstd over its levels, which a solid image
// shows at its plainest. The compressed file must still transcode.
#[test]
fn uastc_ships_smaller_than_it_transcodes() {
  let pixels = SOLID.repeat((LARGE_SIZE * LARGE_SIZE) as usize);
  let file = encode(&pixels, LARGE_SIZE, LARGE_SIZE, &options(Codec::Uastc, false, true)).expect("encode");
  let out = transcode(&file, Target::Etc2Rgba8).expect("transcode");
  assert_eq!((out.width, out.height), (LARGE_SIZE, LARGE_SIZE));
  assert!(file.len() < out.data.len(), "file {} bytes against a payload of {}", file.len(), out.data.len());
  let rgba = transcode(&file, Target::Rgba8).expect("transcode");
  for pixel in rgba.data[..(LARGE_SIZE * LARGE_SIZE * 4) as usize].chunks_exact(4) {
    for (got, want) in pixel.iter().zip(SOLID) {
      assert!((*got as i32 - want as i32).abs() <= TOLERANCE, "{pixel:?} against {SOLID:?}");
    }
  }
}

// A file we did not write: `ktx_document_uastc_rdo_4_zstd_5.ktx2` from the
// Khronos KTX-Software test resources (tests/resources/ktx2, Copyright The
// Khronos Group Inc., Apache-2.0), written by `ktx create` of libktx 5.0 as
// UASTC with RDO and zstd. It pins the container parsing and the zstd path
// against another writer.
#[test]
fn transcodes_a_file_from_another_writer() {
  let file = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/khronos_uastc_zstd.ktx2"))
    .expect("read fixture");
  assert!(is_ktx2(&file));
  for (target, texel_bytes) in [(Target::Etc2Rgba8, 1), (Target::Bc7Rgba8, 1), (Target::Rgba8, 4)] {
    let out = transcode(&file, target).expect("transcode");
    assert_eq!((out.width, out.height, out.levels), (FOREIGN_SIZE, FOREIGN_SIZE, FOREIGN_LEVELS), "{target:?}");
    let base = (FOREIGN_SIZE * FOREIGN_SIZE) as usize * texel_bytes;
    assert!(out.data.len() > base, "{target:?}: the payload holds the chain");
    assert!(file.len() < base, "{target:?}: the file is smaller than its base level");
  }
  // The image is a document page: not one flat color.
  let rgba = transcode(&file, Target::Rgba8).expect("transcode");
  let first = &rgba.data[..4];
  assert!(rgba.data[..(FOREIGN_SIZE * FOREIGN_SIZE * 4) as usize].chunks_exact(4).any(|pixel| pixel != first));
}

// Not a test: the measurement behind the quality a bake should use. Encodes
// each image named in KTX2_MEASURE_IMAGES (paths, ':' separated) with the
// codec named in KTX2_MEASURE_CODEC ("uastc" when unset) at each quality and
// prints file size and error against the source.
// Run it optimized and with output:
//   KTX2_MEASURE_IMAGES=a.jpg:b.jpg cargo test --release -p forge --lib \
//     --features ktx2 measure_quality -- --ignored --nocapture
#[test]
#[ignore]
fn measure_quality() {
  let paths = std::env::var("KTX2_MEASURE_IMAGES").expect("KTX2_MEASURE_IMAGES names the images");
  let codec = Codec::parse(&std::env::var("KTX2_MEASURE_CODEC").unwrap_or("uastc".to_string())).expect("codec");
  for path in paths.split(':') {
    let source = std::fs::read(path).expect("read image");
    let image = crate::image::decode(&source, true).expect("decode image");
    let base = (image.width * image.height * 4) as usize;
    println!("{path}: {}x{}, source {} bytes", image.width, image.height, source.len());
    for quality in MEASURED_QUALITY {
      let started = std::time::Instant::now();
      let options = EncodeOptions { wrap: MipWrap::Repeat, quality, ..options(codec, false, true) };
      let file = encode(&image.data, image.width, image.height, &options).expect("encode");
      let seconds = started.elapsed().as_secs_f32();
      let out = transcode(&file, Target::Rgba8).expect("transcode");
      let payload = transcode(&file, Target::Etc2Rgba8).expect("transcode").data.len();
      // Error over the color channels of the base level.
      let (mut squared, mut worst, mut count) = (0f64, 0i32, 0f64);
      for (got, want) in out.data[..base].chunks_exact(4).zip(image.data.chunks_exact(4)) {
        for channel in 0..3 {
          let error = (got[channel] as i32 - want[channel] as i32).abs();
          squared += (error * error) as f64;
          worst = worst.max(error);
          count += 1.0;
        }
      }
      let psnr = 10.0 * (255.0 * 255.0 / (squared / count)).log10();
      println!(
        "  {codec:?} quality {quality}: file {} bytes ({:.2} bits per texel), payload {payload}, psnr {psnr:.2} dB, worst {worst}, {seconds:.2} s",
        file.len(),
        file.len() as f64 * 8.0 / (image.width * image.height) as f64
      );
    }
  }
}

// The one-payload rule rests on this: the same input encodes to the same
// bytes, run after run (and the threaded encoder does not reorder output).
// Not a test of correctness: measures how long the images named in
// KTX2_MEASURE_IMAGES take to encode, all of them, with 1 to 8 encodes
// running at once, and the most memory the process held meanwhile. What
// ENCODE_THREADS rests on. The codec is KTX2_MEASURE_CODEC ("uastc" when
// unset), at the quality in KTX2_MEASURE_QUALITY (the default when unset).
//   KTX2_MEASURE_IMAGES=a.jpg:b.jpg cargo test --release -p forge --lib \
//     --features ktx2 measure_encodes_at_once -- --ignored --nocapture
#[test]
#[ignore]
fn measure_encodes_at_once() {
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::sync::Arc;

  let paths = std::env::var("KTX2_MEASURE_IMAGES").expect("KTX2_MEASURE_IMAGES names the images");
  let codec = Codec::parse(&std::env::var("KTX2_MEASURE_CODEC").unwrap_or("uastc".to_string())).expect("codec");
  let quality = match std::env::var("KTX2_MEASURE_QUALITY") {
    Ok(quality) => quality.parse().expect("KTX2_MEASURE_QUALITY is a number"),
    Err(_) => DEFAULT_QUALITY,
  };
  let images: Vec<_> = paths
    .split(':')
    .map(|path| crate::image::decode(&std::fs::read(path).expect("read image"), true).expect("decode image"))
    .collect();
  println!("{} images, {codec:?} at quality {quality}", images.len());
  let images = Arc::new(images);
  for at_once in MEASURED_AT_ONCE {
    // Linux: start the high-water mark of the process over.
    let _ = std::fs::write("/proc/self/clear_refs", "5");
    let next = Arc::new(AtomicUsize::new(0));
    let started = std::time::Instant::now();
    let threads: Vec<_> = (0..at_once)
      .map(|_| {
        let (images, next) = (images.clone(), next.clone());
        std::thread::spawn(move || {
          while let Some(image) = images.get(next.fetch_add(1, Ordering::SeqCst)) {
            let options = EncodeOptions { wrap: MipWrap::Repeat, quality, ..options(codec, false, true) };
            encode(&image.data, image.width, image.height, &options).expect("encode");
          }
        })
      })
      .collect();
    for thread in threads {
      thread.join().expect("the encode thread ran");
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let most = status.lines().find(|line| line.starts_with("VmHWM")).unwrap_or("VmHWM: not read").to_string();
    println!("  {at_once} at once: {:.1} s, {most}", started.elapsed().as_secs_f32());
  }
}

#[test]
fn encoding_is_deterministic() {
  let pixels: Vec<u8> = (0..SIZE * SIZE).flat_map(|i| [(i * 3) as u8, (i * 5) as u8, (i * 7) as u8, 255]).collect();
  for codec in [Codec::Etc1s, Codec::Uastc] {
    let first = encode(&pixels, SIZE, SIZE, &options(codec, true, true)).expect("encode");
    let second = encode(&pixels, SIZE, SIZE, &options(codec, true, true)).expect("encode");
    assert_eq!(first, second, "{codec:?}");
  }
}

#[test]
fn refuses_what_it_cannot_read_or_write() {
  assert!(!is_ktx2(b"\x89PNG\r\n\x1a\n"));
  assert!(transcode(b"\x89PNG\r\n\x1a\n", Target::Etc2Rgba8).expect_err("png refused").contains("not a KTX2"));
  let mut truncated = encode(&solid(), SIZE, SIZE, &options(Codec::Etc1s, false, false)).expect("encode");
  truncated.truncate(40);
  assert!(transcode(&truncated, Target::Etc2Rgba8).is_err());
  assert!(encode(&solid()[..16], SIZE, SIZE, &options(Codec::Etc1s, false, false)).expect_err("short").contains("expected 256"));
  let bad = EncodeOptions { quality: 2.0, ..options(Codec::Etc1s, false, false) };
  assert!(encode(&solid(), SIZE, SIZE, &bad).expect_err("quality").contains("out of range"));
  assert!(Target::parse("astc").is_err());
  assert!(Codec::parse("jpeg").is_err());
}
