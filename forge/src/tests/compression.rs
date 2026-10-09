use std::io::{Read, Write};

use crate::compression::{Codec, Direction, Format, Step, OUTPUT_CHUNK_BYTES};

const TEXT: &[u8] = b"hello, compression streams";

/// Every piece the codec has for `steps`, concatenated: each step, then
/// `Next` while the codec is pending.
fn drain(codec: &mut Codec, steps: Vec<Step>) -> Result<Vec<u8>, String> {
  let mut out = Vec::new();
  for step in steps {
    out.extend(codec.step(step)?);
    while codec.pending() {
      out.extend(codec.step(Step::Next)?);
    }
  }
  Ok(out)
}

fn compress(format: Format, input: &[u8]) -> Vec<u8> {
  let mut codec = Codec::new(format, Direction::Compress);
  drain(&mut codec, vec![Step::Push(input.to_vec()), Step::Finish]).expect("compress")
}

fn decompress(format: Format, input: &[u8]) -> Result<Vec<u8>, String> {
  let mut codec = Codec::new(format, Direction::Decompress);
  drain(&mut codec, vec![Step::Push(input.to_vec()), Step::Finish])
}

/// Decompress `input` one byte per push: every header, trailer and symbol
/// boundary lands between steps.
fn decompress_bytewise(format: Format, input: &[u8]) -> Result<Vec<u8>, String> {
  let mut codec = Codec::new(format, Direction::Decompress);
  let mut steps: Vec<Step> = input.iter().map(|&b| Step::Push(vec![b])).collect();
  steps.push(Step::Finish);
  drain(&mut codec, steps)
}

#[test]
fn formats_parse_by_their_standard_names() {
  assert_eq!(Format::parse("gzip"), Some(Format::Gzip));
  assert_eq!(Format::parse("deflate"), Some(Format::Deflate));
  assert_eq!(Format::parse("deflate-raw"), Some(Format::DeflateRaw));
  assert_eq!(Format::parse("br"), None);
  assert_eq!(Format::parse("GZIP"), None);
}

#[test]
fn every_format_round_trips_in_pieces() {
  let input: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
  for format in [Format::Gzip, Format::Deflate, Format::DeflateRaw] {
    let packed = compress(format, &input);
    assert!(packed.len() < input.len(), "{} compresses", format.name());
    assert_eq!(decompress(format, &packed).expect("decompress"), input, "{}", format.name());
    // Finish without input ends an empty stream, which decompresses to nothing.
    let empty = compress(format, b"");
    assert_eq!(decompress(format, &empty).expect("decompress empty"), b"", "{}", format.name());
  }
}

#[test]
fn gzip_output_is_what_other_tools_read() {
  let packed = compress(Format::Gzip, TEXT);
  assert_eq!(&packed[..3], &[0x1f, 0x8b, 8]);
  let mut plain = Vec::new();
  flate2::read::GzDecoder::new(packed.as_slice()).read_to_end(&mut plain).expect("a standard gzip reader reads it");
  assert_eq!(plain, TEXT);
  // The zlib format likewise, through flate2's zlib reader.
  let zlib = compress(Format::Deflate, TEXT);
  let mut plain = Vec::new();
  flate2::read::ZlibDecoder::new(zlib.as_slice()).read_to_end(&mut plain).expect("a zlib reader reads it");
  assert_eq!(plain, TEXT);
}

#[test]
fn gzip_input_from_other_tools_is_read_whatever_its_header_fields() {
  // Every optional header field the format has, and a modification time.
  let mut writer = flate2::GzBuilder::new()
    .filename("hello.txt")
    .comment("a comment")
    .extra(vec![1, 2, 3, 4])
    .mtime(1_700_000_000)
    .write(Vec::new(), flate2::Compression::best());
  writer.write_all(TEXT).expect("write");
  let packed = writer.finish().expect("finish");
  assert_eq!(decompress(Format::Gzip, &packed).expect("decompress"), TEXT);
  // Byte by byte too: the header and trailer complete across pushes.
  assert_eq!(decompress_bytewise(Format::Gzip, &packed).expect("decompress bytewise"), TEXT);
  let zlib = {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(TEXT).expect("write");
    e.finish().expect("finish")
  };
  assert_eq!(decompress_bytewise(Format::Deflate, &zlib).expect("decompress bytewise"), TEXT);
}

#[test]
fn corrupt_input_fails() {
  let mut packed = compress(Format::Gzip, TEXT);
  let body = packed.len() / 2;
  packed[body] ^= 0xff;
  let err = decompress(Format::Gzip, &packed).expect_err("corrupt");
  assert!(err.contains("corrupt"), "{err}");
  // A header that is not gzip at all, told from its first byte.
  let mut codec = Codec::new(Format::Gzip, Direction::Decompress);
  let err = codec.step(Step::Push(vec![0x78])).expect_err("not gzip");
  assert_eq!(err, "the gzip data is corrupt (not a gzip stream)");
  // A trailer whose CRC does not match the data.
  let mut packed = compress(Format::Gzip, TEXT);
  let crc = packed.len() - 8;
  packed[crc] ^= 1;
  let err = decompress(Format::Gzip, &packed).expect_err("bad crc");
  assert_eq!(err, "the gzip data is corrupt (CRC mismatch)");
}

#[test]
fn input_cut_short_fails_at_finish() {
  for format in [Format::Gzip, Format::Deflate, Format::DeflateRaw] {
    let packed = compress(format, TEXT);
    let cut = &packed[..packed.len() - 3];
    let err = decompress(format, cut).expect_err("cut short");
    assert_eq!(err, format!("the {} data is cut short", format.name()));
  }
  // Nothing at all, and a gzip header alone.
  assert_eq!(decompress(Format::Gzip, b"").expect_err("empty"), "the gzip data is cut short");
  assert_eq!(decompress(Format::Gzip, &[0x1f, 0x8b, 8]).expect_err("header only"), "the gzip data is cut short");
}

#[test]
fn data_after_the_stream_fails_a_second_gzip_member_included() {
  let packed = compress(Format::Gzip, TEXT);
  let mut trailing = packed.clone();
  trailing.push(b'x');
  let err = decompress(Format::Gzip, &trailing).expect_err("trailing");
  assert_eq!(err, "trailing data after the end of the gzip stream");
  let mut members = packed.clone();
  members.extend_from_slice(&packed);
  let err = decompress(Format::Gzip, &members).expect_err("second member");
  assert_eq!(err, "trailing data after the end of the gzip stream");
  // The trailing byte in a push of its own, after the stream ended.
  let mut codec = Codec::new(Format::DeflateRaw, Direction::Decompress);
  let raw = compress(Format::DeflateRaw, TEXT);
  drain(&mut codec, vec![Step::Push(raw)]).expect("the stream");
  let err = codec.step(Step::Push(vec![0])).expect_err("trailing push");
  assert_eq!(err, "trailing data after the end of the deflate-raw stream");
}

#[test]
fn output_comes_in_capped_pieces_while_pending() {
  // Four pieces' worth of one byte: a few dozen compressed bytes.
  let plain = vec![0u8; 4 * OUTPUT_CHUNK_BYTES];
  let packed = compress(Format::Gzip, &plain);
  assert!(packed.len() < 1024, "{} bytes", packed.len());
  let mut codec = Codec::new(Format::Gzip, Direction::Decompress);
  let mut pieces = vec![codec.step(Step::Push(packed)).expect("first piece")];
  while codec.pending() {
    pieces.push(codec.step(Step::Next).expect("next piece"));
  }
  let last = codec.step(Step::Finish).expect("finish");
  assert!(last.is_empty());
  assert!(!codec.pending());
  assert_eq!(pieces.len(), 4);
  assert!(pieces.iter().all(|p| p.len() == OUTPUT_CHUNK_BYTES));
  assert_eq!(pieces.concat(), plain);
  // Compressing incompressible input: the pieces cap too, and finish
  // flushes what the engine holds in as many pieces as it takes.
  let noise: Vec<u8> =
    (0..3 * OUTPUT_CHUNK_BYTES as u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
  let mut codec = Codec::new(Format::DeflateRaw, Direction::Compress);
  let mut pieces = vec![codec.step(Step::Push(noise.clone())).expect("push")];
  while codec.pending() {
    pieces.push(codec.step(Step::Next).expect("next"));
  }
  pieces.push(codec.step(Step::Finish).expect("finish"));
  while codec.pending() {
    pieces.push(codec.step(Step::Next).expect("next"));
  }
  assert!(pieces.iter().all(|p| p.len() <= OUTPUT_CHUNK_BYTES));
  assert!(pieces.len() > 1);
  assert_eq!(decompress(Format::DeflateRaw, &pieces.concat()).expect("decompress"), noise);
}

#[tokio::test]
async fn a_queued_step_returns_the_codec_with_its_piece() {
  let codec = Codec::new(Format::Deflate, Direction::Compress);
  let (codec, first) = codec.step_queued(Step::Push(TEXT.to_vec())).await.expect("push");
  let (codec, rest) = codec.step_queued(Step::Finish).await.expect("finish");
  assert!(!codec.pending());
  let packed = [first, rest].concat();
  assert_eq!(decompress(Format::Deflate, &packed).expect("decompress"), TEXT);
  // A failing step takes the codec with it.
  let codec = Codec::new(Format::Gzip, Direction::Decompress);
  let err = match codec.step_queued(Step::Push(vec![0, 0])).await {
    Ok(_) => panic!("not gzip"),
    Err(e) => e,
  };
  assert_eq!(err, "the gzip data is corrupt (not a gzip stream)");
}
