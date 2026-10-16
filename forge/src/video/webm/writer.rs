// WebM mux: the EBML subset the reader beside it walks (webm.rs), written
// from coded VP9 frames and Opus packets as the encoders hand them out.
// Two sinks. A FILE, seekable: the Segment and every Cluster get a real
// size, the Duration and a SeekHead to tail Cues are patched at finish, and
// the clip seeks in this player and in others. A STREAM, write-only: the
// Segment and the Clusters are of unknown size, nothing is patched, no Cues,
// and bytes leave as blocks arrive, the live form the reader's unbounded
// path takes. Clusters open on a video keyframe and never run past
// MAX_CLUSTER_MS, so a seek lands close and the block's relative time fits
// its i16.
//
// Times are the encoders' microseconds, rounded to the millisecond tick
// (TimestampScale 1 ms, the default every player assumes).

use std::io::{Seek, SeekFrom, Write};

use super::ids::*;
use super::OPUS_SAMPLE_RATE;
use crate::video::encode::EncodedFrame;

/// A seekable byte sink: a file, a cursor over memory.
pub trait WriteSeek: Write + Seek {}
impl<T: Write + Seek> WriteSeek for T {}

/// Where the container goes.
pub enum WebmSink {
  /// Seekable: sizes, Duration and Cues are patched in at finish.
  File(Box<dyn WriteSeek + Send>),
  /// Write-only: unknown sizes, no Cues, written as it goes.
  Stream(Box<dyn Write + Send>),
}

/// The VP9 track: BT.601 studio range, what `rgba_to_i420` produces.
pub struct WebmVideoTrack {
  pub width: u32,
  pub height: u32,
}

/// The Opus track, from the encoder: its channels, the rate it was fed
/// at (the OpusHead's input rate, informational) and its pre-skip in
/// 48 kHz samples (`OpusEncoder::pre_skip`).
pub struct WebmAudioTrack {
  pub channels: u16,
  pub input_sample_rate: u32,
  pub pre_skip: u32,
}

const VIDEO_TRACK: u64 = 1;
const AUDIO_TRACK: u64 = 2;
const DOCTYPE: &str = "webm";
// WebM's EBML header: DocTypeVersion 4 (Colour, CodecDelay, SeekPreRoll
// exist), readable by a version 2 reader.
const DOCTYPE_VERSION: u64 = 4;
const DOCTYPE_READ_VERSION: u64 = 2;
const EBML_VERSION: u64 = 1;
const EBML_MAX_ID_LENGTH: u64 = 4;
const EBML_MAX_SIZE_LENGTH: u64 = 8;
const APP: &str = "solidrt";
// A SimpleBlock's time is an i16 of ticks from its cluster's: clusters are
// cut well inside that, and since a seek lands on a cluster, shorter ones
// seek finer.
const MAX_CLUSTER_MS: u64 = 5_000;
// The width of every patched field: a size vint of 8 bytes, an 8-byte
// unsigned integer, an 8-byte float. Written as a placeholder, patched at
// finish in the file form.
const PATCH_BYTES: usize = 8;
const TICK_US: i64 = (DEFAULT_TIMESTAMP_SCALE_NS / 1000) as i64;

struct Cluster {
  /// Where its size placeholder is (file form).
  size_at: u64,
  ticks: u64,
}

/// A track's last two block times, for the Duration: the last block plus
/// the gap before it, the frame length a player would assume.
#[derive(Default, Clone, Copy)]
struct TrackTime {
  last: Option<u64>,
  gap: u64,
}

pub struct WebmWriter {
  sink: WebmSink,
  /// Bytes written: the stream position the next write lands at.
  pos: u64,
  segment_start: u64,
  segment_size_at: u64,
  cues_position_at: u64,
  duration_at: u64,
  video: Option<WebmVideoTrack>,
  audio: Option<WebmAudioTrack>,
  cluster: Option<Cluster>,
  /// (ticks, cluster offset) per cluster, the tail Cues of the file form.
  cues: Vec<(u64, u64)>,
  times: [TrackTime; 2],
}

impl WebmWriter {
  /// Write the header for the given tracks (at least one). Errs on an
  /// empty track set or a sink that fails.
  pub fn new(
    sink: WebmSink,
    video: Option<WebmVideoTrack>,
    audio: Option<WebmAudioTrack>,
  ) -> Result<WebmWriter, String> {
    if video.is_none() && audio.is_none() {
      return Err("webm: no tracks to write".to_string());
    }
    let mut writer = WebmWriter {
      sink,
      pos: 0,
      segment_start: 0,
      segment_size_at: 0,
      cues_position_at: 0,
      duration_at: 0,
      video,
      audio,
      cluster: None,
      cues: Vec::new(),
      times: [TrackTime::default(); 2],
    };
    writer.write_header()?;
    Ok(writer)
  }

  fn seekable(&self) -> bool {
    matches!(self.sink, WebmSink::File(_))
  }

  fn write_header(&mut self) -> Result<(), String> {
    let mut buf = Vec::new();
    master(&mut buf, ID_EBML, |b| {
      uint(b, ID_EBML_VERSION, EBML_VERSION);
      uint(b, ID_EBML_READ_VERSION, EBML_VERSION);
      uint(b, ID_EBML_MAX_ID_LENGTH, EBML_MAX_ID_LENGTH);
      uint(b, ID_EBML_MAX_SIZE_LENGTH, EBML_MAX_SIZE_LENGTH);
      string(b, ID_DOCTYPE, DOCTYPE);
      uint(b, ID_DOCTYPE_VERSION, DOCTYPE_VERSION);
      uint(b, ID_DOCTYPE_READ_VERSION, DOCTYPE_READ_VERSION);
    });
    put_id(&mut buf, ID_SEGMENT);
    self.segment_size_at = self.pos + buf.len() as u64;
    if self.seekable() {
      put_vint(&mut buf, 0, PATCH_BYTES);
    } else {
      put_unknown_size(&mut buf);
    }
    self.segment_start = self.pos + buf.len() as u64;

    if self.seekable() {
      // One entry, the Cues; its position is the SeekHead's last 8 bytes.
      master(&mut buf, ID_SEEK_HEAD, |b| {
        master(b, ID_SEEK, |b| {
          element(b, ID_SEEK_ID, &id_bytes(ID_CUES));
          put_id(b, ID_SEEK_POSITION);
          put_size(b, PATCH_BYTES as u64);
          b.extend_from_slice(&[0; PATCH_BYTES]);
        });
      });
      self.cues_position_at = self.pos + (buf.len() - PATCH_BYTES) as u64;
    }

    let seekable = self.seekable();
    master(&mut buf, ID_INFO, |b| {
      uint(b, ID_TIMESTAMP_SCALE, DEFAULT_TIMESTAMP_SCALE_NS);
      string(b, ID_MUXING_APP, APP);
      string(b, ID_WRITING_APP, APP);
      if seekable {
        // Last in Info, so it is the element's last 8 bytes.
        put_id(b, ID_DURATION);
        put_size(b, PATCH_BYTES as u64);
        b.extend_from_slice(&0f64.to_be_bytes());
      }
    });
    if seekable {
      self.duration_at = self.pos + (buf.len() - PATCH_BYTES) as u64;
    }

    let video = self.video.as_ref();
    let audio = self.audio.as_ref();
    master(&mut buf, ID_TRACKS, |b| {
      if let Some(video) = video {
        master(b, ID_TRACK_ENTRY, |b| {
          uint(b, ID_TRACK_NUMBER, VIDEO_TRACK);
          uint(b, ID_TRACK_UID, VIDEO_TRACK);
          uint(b, ID_TRACK_TYPE, TRACK_TYPE_VIDEO);
          string(b, ID_CODEC_ID, CODEC_VP9);
          master(b, ID_VIDEO, |b| {
            uint(b, ID_PIXEL_WIDTH, video.width as u64);
            uint(b, ID_PIXEL_HEIGHT, video.height as u64);
            master(b, ID_COLOUR, |b| {
              uint(b, ID_MATRIX_COEFFICIENTS, MATRIX_BT601);
              uint(b, ID_RANGE, RANGE_STUDIO);
            });
          });
        });
      }
      if let Some(audio) = audio {
        master(b, ID_TRACK_ENTRY, |b| {
          uint(b, ID_TRACK_NUMBER, AUDIO_TRACK);
          uint(b, ID_TRACK_UID, AUDIO_TRACK);
          uint(b, ID_TRACK_TYPE, TRACK_TYPE_AUDIO);
          string(b, ID_CODEC_ID, CODEC_OPUS);
          uint(b, ID_CODEC_DELAY, audio.pre_skip as u64 * 1_000_000_000 / OPUS_SAMPLE_RATE as u64);
          uint(b, ID_SEEK_PRE_ROLL, OPUS_SEEK_PREROLL_NS);
          element(b, ID_CODEC_PRIVATE, &opus_head(audio));
          master(b, ID_AUDIO, |b| {
            float(b, ID_SAMPLING_FREQUENCY, OPUS_SAMPLE_RATE as f64);
            uint(b, ID_CHANNELS, audio.channels as u64);
          });
        });
      }
    });
    self.write_bytes(&buf)
  }

  /// Write one coded VP9 frame.
  pub fn write_video(&mut self, frame: &EncodedFrame) -> Result<(), String> {
    if self.video.is_none() {
      return Err("webm: no video track".to_string());
    }
    self.write_block(VIDEO_TRACK, frame.pts_us, frame.keyframe, &frame.data)
  }

  /// Write one Opus packet.
  pub fn write_audio(&mut self, pts_us: i64, packet: &[u8]) -> Result<(), String> {
    if self.audio.is_none() {
      return Err("webm: no audio track".to_string());
    }
    // Every audio packet decodes on its own: a keyframe, as muxers flag it.
    self.write_block(AUDIO_TRACK, pts_us, true, packet)
  }

  /// The media written so far, in microseconds: the last block's time plus
  /// the frame length before it.
  pub fn duration_us(&self) -> i64 {
    self.duration_ticks() as i64 * TICK_US
  }

  fn duration_ticks(&self) -> u64 {
    self.times.iter().filter_map(|t| t.last.map(|last| last + t.gap)).max().unwrap_or(0)
  }

  fn write_block(&mut self, track: u64, pts_us: i64, keyframe: bool, data: &[u8]) -> Result<(), String> {
    let ticks = ((pts_us.max(0) + TICK_US / 2) / TICK_US) as u64;
    let cut = match &self.cluster {
      None => true,
      Some(cluster) => (track == VIDEO_TRACK && keyframe) || ticks >= cluster.ticks + MAX_CLUSTER_MS,
    };
    if cut {
      self.close_cluster()?;
      self.open_cluster(ticks)?;
    }
    let cluster_ticks = self.cluster.as_ref().map_or(0, |c| c.ticks);
    let rel = ticks as i64 - cluster_ticks as i64;
    let rel = i16::try_from(rel).map_err(|_| format!("webm: block {rel} ms from its cluster"))?;

    let mut buf = Vec::with_capacity(data.len() + 16);
    put_id(&mut buf, ID_SIMPLE_BLOCK);
    put_size(&mut buf, (data.len() + 4) as u64);
    put_vint(&mut buf, track, 1);
    buf.extend_from_slice(&rel.to_be_bytes());
    buf.push(if keyframe { BLOCK_FLAG_KEYFRAME } else { 0 });
    buf.extend_from_slice(data);
    self.write_bytes(&buf)?;

    let time = &mut self.times[(track - 1) as usize];
    if let Some(last) = time.last {
      time.gap = ticks.saturating_sub(last);
    }
    time.last = Some(ticks);
    Ok(())
  }

  fn open_cluster(&mut self, ticks: u64) -> Result<(), String> {
    let offset = self.pos - self.segment_start;
    let mut buf = Vec::new();
    put_id(&mut buf, ID_CLUSTER);
    let size_at = self.pos + buf.len() as u64;
    if self.seekable() {
      put_vint(&mut buf, 0, PATCH_BYTES);
    } else {
      put_unknown_size(&mut buf);
    }
    uint(&mut buf, ID_CLUSTER_TIMESTAMP, ticks);
    self.write_bytes(&buf)?;
    self.cluster = Some(Cluster { size_at, ticks });
    self.cues.push((ticks, offset));
    Ok(())
  }

  fn close_cluster(&mut self) -> Result<(), String> {
    let Some(cluster) = self.cluster.take() else { return Ok(()) };
    if self.seekable() {
      let size = self.pos - (cluster.size_at + PATCH_BYTES as u64);
      self.patch(cluster.size_at, &size_vint(size))?;
    }
    Ok(())
  }

  /// Close the last cluster and, for a file, write the Cues and patch the
  /// Segment size, the Duration and the SeekHead. Flushes the sink.
  pub fn finish(mut self) -> Result<(), String> {
    self.close_cluster()?;
    if self.seekable() {
      let cues_at = self.pos - self.segment_start;
      let cue_track = if self.video.is_some() { VIDEO_TRACK } else { AUDIO_TRACK };
      let cues = std::mem::take(&mut self.cues);
      let mut buf = Vec::new();
      master(&mut buf, ID_CUES, |b| {
        for &(ticks, offset) in &cues {
          master(b, ID_CUE_POINT, |b| {
            uint(b, ID_CUE_TIME, ticks);
            master(b, ID_CUE_TRACK_POSITIONS, |b| {
              uint(b, ID_CUE_TRACK, cue_track);
              uint(b, ID_CUE_CLUSTER_POSITION, offset);
            });
          });
        }
      });
      self.write_bytes(&buf)?;
      let segment_size = self.pos - self.segment_start;
      self.patch(self.segment_size_at, &size_vint(segment_size))?;
      self.patch(self.cues_position_at, &cues_at.to_be_bytes())?;
      let duration = self.duration_ticks() as f64;
      self.patch(self.duration_at, &duration.to_be_bytes())?;
    }
    match &mut self.sink {
      WebmSink::File(sink) => sink.flush(),
      WebmSink::Stream(sink) => sink.flush(),
    }
    .map_err(|e| format!("webm flush: {e}"))
  }

  fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
    match &mut self.sink {
      WebmSink::File(sink) => sink.write_all(bytes),
      WebmSink::Stream(sink) => sink.write_all(bytes),
    }
    .map_err(|e| format!("webm write at {}: {e}", self.pos))?;
    self.pos += bytes.len() as u64;
    Ok(())
  }

  /// Overwrite `bytes` at `at` and come back to the end (file form only).
  fn patch(&mut self, at: u64, bytes: &[u8]) -> Result<(), String> {
    let WebmSink::File(sink) = &mut self.sink else {
      return Ok(());
    };
    let end = self.pos;
    sink
      .seek(SeekFrom::Start(at))
      .and_then(|_| sink.write_all(bytes))
      .and_then(|()| sink.seek(SeekFrom::Start(end)))
      .map_err(|e| format!("webm patch at {at}: {e}"))?;
    Ok(())
  }
}

/// The OpusHead (RFC 7845) for a track: what the reader's `opus_info`
/// parses back.
fn opus_head(audio: &WebmAudioTrack) -> Vec<u8> {
  let mut head = vec![0u8; OPUS_HEAD_LEN];
  head[..OPUS_HEAD_MAGIC.len()].copy_from_slice(OPUS_HEAD_MAGIC);
  head[OPUS_HEAD_MAGIC.len()] = OPUS_HEAD_VERSION;
  head[OPUS_HEAD_CHANNELS] = audio.channels as u8;
  head[OPUS_HEAD_PRE_SKIP..OPUS_HEAD_PRE_SKIP + 2]
    .copy_from_slice(&(audio.pre_skip.min(u16::MAX as u32) as u16).to_le_bytes());
  head[OPUS_HEAD_INPUT_RATE..OPUS_HEAD_INPUT_RATE + 4].copy_from_slice(&audio.input_sample_rate.to_le_bytes());
  head[OPUS_HEAD_OUTPUT_GAIN..OPUS_HEAD_OUTPUT_GAIN + 2].copy_from_slice(&0i16.to_le_bytes());
  head[OPUS_HEAD_MAPPING_FAMILY] = 0;
  head
}

// EBML primitives over a byte buffer.

/// An element id as stored: as many bytes as its length marker says.
fn id_bytes(id: u32) -> Vec<u8> {
  let len = 4 - (id.leading_zeros() / 8) as usize;
  id.to_be_bytes()[4 - len..].to_vec()
}

fn put_id(buf: &mut Vec<u8>, id: u32) {
  buf.extend_from_slice(&id_bytes(id));
}

/// A vint of exactly `len` bytes: the length marker, then the value.
fn put_vint(buf: &mut Vec<u8>, value: u64, len: usize) {
  let marked = (1u64 << (7 * len)) | value;
  buf.extend_from_slice(&marked.to_be_bytes()[8 - len..]);
}

/// The shortest vint that holds `value` (the all-ones pattern of each
/// length means "unknown", so a value needs one more).
fn vint_len(value: u64) -> usize {
  (1..=8).find(|&len| value < (1u64 << (7 * len)) - 1).unwrap_or(8)
}

fn put_size(buf: &mut Vec<u8>, size: u64) {
  put_vint(buf, size, vint_len(size));
}

/// The 8-byte size vint for a patched field.
fn size_vint(size: u64) -> Vec<u8> {
  let mut buf = Vec::with_capacity(PATCH_BYTES);
  put_vint(&mut buf, size, PATCH_BYTES);
  buf
}

/// The 8-byte "unknown size" vint: every value bit set.
fn put_unknown_size(buf: &mut Vec<u8>) {
  put_vint(buf, (1u64 << 56) - 1, PATCH_BYTES);
}

fn element(buf: &mut Vec<u8>, id: u32, payload: &[u8]) {
  put_id(buf, id);
  put_size(buf, payload.len() as u64);
  buf.extend_from_slice(payload);
}

fn uint(buf: &mut Vec<u8>, id: u32, value: u64) {
  let bytes = value.to_be_bytes();
  let skip = (value.leading_zeros() / 8).min(7) as usize;
  element(buf, id, &bytes[skip..]);
}

fn float(buf: &mut Vec<u8>, id: u32, value: f64) {
  element(buf, id, &value.to_be_bytes());
}

fn string(buf: &mut Vec<u8>, id: u32, value: &str) {
  element(buf, id, value.as_bytes());
}

/// A master element whose children `fill` appends.
fn master(buf: &mut Vec<u8>, id: u32, fill: impl FnOnce(&mut Vec<u8>)) {
  let mut inner = Vec::new();
  fill(&mut inner);
  element(buf, id, &inner);
}
