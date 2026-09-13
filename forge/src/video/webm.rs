// WebM demux: a reader for the EBML subset our streams use (VP9 video, Opus
// audio, as ffmpeg and our own writer produce them), nothing more. The
// container never reaches a decoder: a SimpleBlock's payload is one coded
// VP9 frame (superframes included) or one Opus packet, exactly as the
// decoders want it, and VP9 has no out-of-band parameter sets. Written
// rather than taken from a crate because the subset is a few hundred lines
// and a general Matroska reader links at more than half a VP9 decoder
// (okf/plans/android-video-punch-through.md).
//
// The byte source is any `SeekableReader` plus the facts about it (a local
// file, or a streamed source from `forge::source`): it may be unbounded (an
// unknown-size Segment of unknown-size Clusters, what a live producer
// writes) and may not seek. The walk never needs an element's end unless it
// skips it, and it descends into Segment and Cluster instead of skipping
// them. Nothing here trusts the source: a size is checked against
// MAX_ELEMENT_BYTES before it reaches an allocation, and a read error is
// sticky until a seek. Every epoch (open, seek) starts on a keyframe: video
// before the first sync block is dropped and audio is held back so the
// preroll before that keyframe still plays.
//
// Seeking needs Cues and a seekable source. Tail Cues (ffmpeg's default
// placement) are loaded by the first seek, never by open, so opening a
// streamed source costs one request.

use std::collections::VecDeque;
use std::io::{self, BufReader, Read, Seek};

use super::{AudioInfo, AudioPacket, Demuxer, ErrorKind, MediaInfo, Packet, StreamError, VideoAu};
use crate::seek::SeekableReader;
use crate::source::{open_file, Facts};

// EBML and Matroska element ids, as they appear in the file (the id's own
// length bits included).
const ID_EBML: u32 = 0x1A45DFA3;
const ID_DOCTYPE: u32 = 0x4282;
const ID_SEGMENT: u32 = 0x18538067;
const ID_SEEK_HEAD: u32 = 0x114D9B74;
const ID_SEEK: u32 = 0x4DBB;
const ID_SEEK_ID: u32 = 0x53AB;
const ID_SEEK_POSITION: u32 = 0x53AC;
const ID_INFO: u32 = 0x1549A966;
const ID_TIMESTAMP_SCALE: u32 = 0x2AD7B1;
const ID_DURATION: u32 = 0x4489;
const ID_TRACKS: u32 = 0x1654AE6B;
const ID_TRACK_ENTRY: u32 = 0xAE;
const ID_TRACK_NUMBER: u32 = 0xD7;
const ID_TRACK_TYPE: u32 = 0x83;
const ID_CODEC_ID: u32 = 0x86;
const ID_CODEC_PRIVATE: u32 = 0x63A2;
const ID_SEEK_PRE_ROLL: u32 = 0x56BB;
const ID_VIDEO: u32 = 0xE0;
const ID_PIXEL_WIDTH: u32 = 0xB0;
const ID_PIXEL_HEIGHT: u32 = 0xBA;
const ID_COLOUR: u32 = 0x55B0;
const ID_MATRIX_COEFFICIENTS: u32 = 0x55B1;
const ID_RANGE: u32 = 0x55B9;
const ID_AUDIO: u32 = 0xE1;
const ID_CHANNELS: u32 = 0x9F;
const ID_CUES: u32 = 0x1C53BB6B;
const ID_CUE_POINT: u32 = 0xBB;
const ID_CUE_TIME: u32 = 0xB3;
const ID_CUE_TRACK_POSITIONS: u32 = 0xB7;
const ID_CUE_CLUSTER_POSITION: u32 = 0xF1;
const ID_CLUSTER: u32 = 0x1F43B675;
const ID_CLUSTER_TIMESTAMP: u32 = 0xE7;
const ID_SIMPLE_BLOCK: u32 = 0xA3;
const ID_BLOCK_GROUP: u32 = 0xA0;
const ID_BLOCK: u32 = 0xA1;
const ID_REFERENCE_BLOCK: u32 = 0xFB;

// The first four bytes of an MP4 are a box size, 0 0 0 <n> for the ftyp box
// in every muxer's output: named in the error so a stale clip is obvious.
const MP4_MAGIC_PREFIX: &[u8] = &[0, 0, 0, 0x1C];
const TRACK_TYPE_VIDEO: u64 = 1;
const TRACK_TYPE_AUDIO: u64 = 2;
const CODEC_VP9: &str = "V_VP9";
const CODEC_OPUS: &str = "A_OPUS";
// TimestampScale default: cluster and block times in milliseconds.
const DEFAULT_TIMESTAMP_SCALE_NS: u64 = 1_000_000;
// Block header flags: keyframe (SimpleBlock only) and the lacing field.
const BLOCK_FLAG_KEYFRAME: u8 = 0x80;
const BLOCK_FLAG_LACING: u8 = 0x06;
// Colour: MatrixCoefficients codes (ITU-T H.273) BT.709 and the two
// BT.601 codes; anything else (2 = unspecified, what ffmpeg writes) falls
// back to the resolution default. Range: 1 = broadcast (studio), 2 = full.
const MATRIX_BT709: u64 = 1;
const MATRIX_BT470BG: u64 = 5;
const MATRIX_BT601: u64 = 6;
const RANGE_FULL: u64 = 2;
// The resolution default when the container does not say: HD content (this
// many lines and up) is BT.709, SD is BT.601.
const HD_LINES: u32 = 720;
// OpusHead (RFC 7845): magic, version, channels, pre-skip (u16 LE), input
// sample rate (u32 LE), output gain, channel mapping family.
const OPUS_HEAD_MAGIC: &[u8] = b"OpusHead";
const OPUS_HEAD_LEN: usize = 19;
const OPUS_HEAD_CHANNELS: usize = 9;
const OPUS_HEAD_PRE_SKIP: usize = 10;
const OPUS_HEAD_MAPPING_FAMILY: usize = 18;
// Opus decodes at 48 kHz whatever the input rate was; the WebM Audio
// element says so too, this is the rate a sink is opened at.
pub const OPUS_SAMPLE_RATE: u32 = 48_000;
// The most channels libopus's plain decoder takes (mapping family 0).
const OPUS_MAX_CHANNELS: u16 = 2;
// A VP9 uncompressed header opens with frame_marker 0b10, then the profile's
// low and high bits: the pipeline takes profile 0 (8-bit 4:2:0) only.
const VP9_FRAME_MARKER: u8 = 0b10;
const VP9_PROFILE_MASK: u8 = 0b0011_0000;
// The largest element read into memory, or skipped by reading on a source
// that cannot seek: a coded 4K VP9 keyframe is a few MB, Cues of a long
// file a few hundred KB. A size past this is garbage from the wire, refused
// before any allocation.
const MAX_ELEMENT_BYTES: u64 = 32 * 1024 * 1024;
// How much audio is held back while an epoch waits for its first keyframe,
// so the preroll before that keyframe still plays. Muxers interleave audio
// at most a cluster ahead of video; this covers that with room to spare.
const GATING_HOLD_US: i64 = 2_000_000;

/// A WebM stream: VP9 video, optional Opus audio, read block by block.
pub struct WebmDemuxer {
  reader: Ebml,
  info: MediaInfo,
  timestamp_scale_ns: u64,
  // Byte offset of the Segment's data, what cue positions are relative to.
  segment_start: u64,
  video_track: u64,
  audio_track: Option<u64>,
  play_video: bool,
  play_audio: bool,
  // Keyframe cue points as (time in us, cluster offset from segment_start),
  // ascending: what `seek` binary-searches. Empty until loaded or when the
  // file has none.
  cues: Vec<(i64, u64)>,
  // Where the SeekHead says tail Cues are, until the first seek visits them.
  cues_position: Option<u64>,
  cluster_ts: Option<u64>,
  // Blocks read past what the caller asked for on the other track.
  video_q: VecDeque<VideoAu>,
  audio_q: VecDeque<AudioPacket>,
  // The epoch's start: where playback was asked to resume.
  epoch_target_us: i64,
  // Where it does resume: the target, or the first keyframe when that comes
  // later.
  resume_us: i64,
  // Waiting for the epoch's first keyframe: video is dropped and audio held.
  gating: bool,
  held_audio: VecDeque<AudioPacket>,
  // Audio packets before this pts are dropped (they precede the resume
  // position by more than the track's seek preroll).
  audio_skip_until: Option<i64>,
  ended: bool,
  // A stream or read error, repeated until a seek.
  failed: Option<StreamError>,
}

impl WebmDemuxer {
  /// Open a local WebM file (tests, the texture player). The path resolves
  /// like every forge file read (through the assets mount when one is set).
  pub fn open_path(path: &str) -> Result<Self, StreamError> {
    let source = open_file(path).map_err(|e| StreamError::network(e.to_string()))?;
    let facts = source.facts().map_err(|e| StreamError::network(e.to_string()))?;
    Self::open(source.into_reader(), facts).map_err(|e| e.context(path))
  }

  /// Open a stream and read its header up to the first keyframe. Errs when
  /// the stream is not WebM, has no VP9 track, or its VP9 is not profile 0.
  /// A missing or non-Opus audio track is not an error: `info().audio` is
  /// None and playback is silent.
  pub fn open(reader: SeekableReader, facts: Facts) -> Result<Self, StreamError> {
    let mut reader = Ebml::new(reader, facts.seekable);

    // The magic first, before any EBML interpretation: an MP4 or an empty
    // file must say what it is, not fail on a malformed element id. The
    // four bytes are the EBML element's id, so no rewind.
    let magic = reader.magic()?;
    if u32::from_be_bytes(magic) != ID_EBML {
      let what = if &magic[..] == MP4_MAGIC_PREFIX { "an MP4" } else { "no EBML header" };
      return Err(StreamError::unsupported(format!(
        "not a webm file ({what}; video plays WebM with VP9 and Opus only)"
      )));
    }
    let size = reader.vint()?;
    let end = reader.end_of(size).ok_or("EBML header of unknown size")?;
    let mut doctype = String::new();
    while reader.pos < end {
      let (id, size) = reader.header()?.ok_or("truncated EBML header")?;
      match id {
        ID_DOCTYPE => doctype = reader.string(size)?,
        _ => reader.skip(size)?,
      }
    }
    if doctype != "webm" && doctype != "matroska" {
      return Err(StreamError::unsupported(format!("not a webm file (doctype \"{doctype}\")")));
    }

    let (id, segment_size) = reader.header()?.ok_or("no segment")?;
    if id != ID_SEGMENT {
      return Err(StreamError::unsupported("not a webm file (no segment)"));
    }
    let segment_start = reader.pos;

    // The Segment's header children, up to the first Cluster. ffmpeg puts
    // Cues after the clusters and points at them from the SeekHead; the
    // first seek visits them.
    let mut timestamp_scale_ns = DEFAULT_TIMESTAMP_SCALE_NS;
    let mut duration_ticks: Option<f64> = None;
    let mut tracks: Vec<TrackEntry> = Vec::new();
    let mut cues: Vec<(u64, u64)> = Vec::new();
    let mut cues_position: Option<u64> = None;
    loop {
      let Some((id, size)) = reader.header()? else {
        return Err("no clusters in file".into());
      };
      match id {
        ID_CLUSTER => break,
        ID_SEEK_HEAD => cues_position = cues_position.or(read_seek_head(&mut reader, size)?),
        ID_INFO => {
          let end = reader.end_of(size).ok_or("info of unknown size")?;
          while reader.pos < end {
            let (id, size) = reader.header()?.ok_or("truncated info")?;
            match id {
              ID_TIMESTAMP_SCALE => timestamp_scale_ns = reader.uint(size)?,
              ID_DURATION => duration_ticks = Some(reader.float(size)?),
              _ => reader.skip(size)?,
            }
          }
        }
        ID_TRACKS => tracks = read_tracks(&mut reader, size)?,
        ID_CUES => cues = read_cues(&mut reader, size)?,
        _ => reader.skip(size)?,
      }
    }

    let video = tracks
      .iter()
      .find(|t| t.kind == TRACK_TYPE_VIDEO && t.codec == CODEC_VP9)
      .ok_or_else(|| StreamError::unsupported("no VP9 video track"))?;
    let (width, height) = (video.width, video.height);
    if width == 0 || height == 0 {
      return Err("VP9 track has no pixel size".into());
    }
    let bt709 = match video.matrix {
      Some(MATRIX_BT709) => true,
      Some(MATRIX_BT470BG) | Some(MATRIX_BT601) => false,
      _ => height >= HD_LINES,
    };
    let full_range = video.range == Some(RANGE_FULL);
    let audio_track = tracks.iter().find(|t| t.kind == TRACK_TYPE_AUDIO && t.codec == CODEC_OPUS);
    let audio = match audio_track.map(opus_info) {
      Some(Ok(info)) => Some(info),
      Some(Err(e)) => {
        log::warn!("[forge::video] ignoring audio track: {e}");
        None
      }
      None => None,
    };
    // Duration is a fact only when the Segment has a known size: a muxer
    // writing to a pipe leaves a Duration it could not know.
    let duration_us = match segment_size {
      Some(_) => duration_ticks.map(|d| ((d * timestamp_scale_ns as f64) / 1000.0).round() as i64),
      None => None,
    };
    let to_us = |ticks: u64| (ticks as u128 * timestamp_scale_ns as u128 / 1000) as i64;

    let mut demux = WebmDemuxer {
      reader,
      info: MediaInfo { width, height, duration_us, start_us: 0, seekable: facts.seekable, bt709, full_range, audio },
      timestamp_scale_ns,
      segment_start,
      video_track: video.number,
      audio_track: audio_track.map(|t| t.number),
      play_video: true,
      play_audio: true,
      cues: cues.iter().map(|&(t, pos)| (to_us(t), pos)).collect(),
      cues_position,
      cluster_ts: None,
      video_q: VecDeque::new(),
      audio_q: VecDeque::new(),
      epoch_target_us: 0,
      resume_us: 0,
      gating: true,
      held_audio: VecDeque::new(),
      audio_skip_until: None,
      ended: false,
      failed: None,
    };
    // The first keyframe: the stream's start, and the frame whose header
    // says the profile (WebM does not always carry VP9 codec metadata, the
    // bitstream always does).
    while demux.gating && demux.read_block()? {}
    let first = demux.video_q.front().ok_or("video track has no keyframe")?;
    let marker = first.data.first().copied().unwrap_or(0);
    if marker >> 6 != VP9_FRAME_MARKER {
      return Err(StreamError::unsupported("video track is not VP9 (no frame marker)"));
    }
    if marker & VP9_PROFILE_MASK != 0 {
      return Err(StreamError::unsupported("unsupported VP9 stream: only profile 0 (8-bit 4:2:0) plays"));
    }
    demux.info.start_us = demux.resume_us;
    Ok(demux)
  }

  /// Which tracks are played: blocks of the other are dropped at the
  /// source instead of queueing for a reader that never comes.
  pub fn set_tracks(&mut self, video: bool, audio: bool) {
    self.play_video = video;
    self.play_audio = audio;
    if !video {
      self.video_q.clear();
    }
    if !audio {
      self.audio_q.clear();
      self.held_audio.clear();
    }
  }

  /// Read elements until one block has been queued (on either track);
  /// false at the end of the stream. Errors stick until a seek.
  fn read_block(&mut self) -> Result<bool, StreamError> {
    if let Some(e) = &self.failed {
      return Err(e.clone());
    }
    if self.ended {
      return Ok(false);
    }
    match self.read_block_inner() {
      Ok(queued) => Ok(queued),
      Err(e) => {
        // An interrupted read (a command for the reading thread) leaves the
        // walk mid-element: the next call must be a seek, not sticky.
        if e.kind != ErrorKind::Interrupted {
          self.failed = Some(e.clone());
        }
        Err(e)
      }
    }
  }

  fn read_block_inner(&mut self) -> Result<bool, StreamError> {
    loop {
      let Some((id, size)) = self.reader.header()? else {
        self.ended = true;
        return Ok(false);
      };
      match id {
        ID_CLUSTER => self.cluster_ts = None,
        ID_CLUSTER_TIMESTAMP => self.cluster_ts = Some(self.reader.uint(size)?),
        ID_SIMPLE_BLOCK => {
          let data = self.reader.bytes(size)?;
          if self.queue_block(&data, None)? {
            return Ok(true);
          }
        }
        ID_BLOCK_GROUP => {
          let end = self.reader.end_of(size).ok_or("block group of unknown size")?;
          let mut block = None;
          let mut referenced = false;
          while self.reader.pos < end {
            let (id, size) = self.reader.header()?.ok_or("truncated block group")?;
            match id {
              ID_BLOCK => block = Some(self.reader.bytes(size)?),
              ID_REFERENCE_BLOCK => {
                referenced = true;
                self.reader.skip(size)?;
              }
              _ => self.reader.skip(size)?,
            }
          }
          if let Some(data) = block {
            if self.queue_block(&data, Some(!referenced))? {
              return Ok(true);
            }
          }
        }
        // A producer started a new stream on the same bytes (a live relay
        // switching sources): not something the decoders can follow yet.
        ID_EBML => {
          return Err(StreamError::unsupported(format!("unsupported: a new stream starts at {}", self.reader.pos)))
        }
        // Anything after the clusters (Cues, Tags) or unknown: skip when
        // the size allows, else the stream is over for us.
        _ => match size {
          Some(_) => self.reader.skip(size)?,
          None => {
            self.ended = true;
            return Ok(false);
          }
        },
      }
    }
  }

  /// Queue one block's frame on its track; true when it was queued (a
  /// block on a track we do not play, a pre-keyframe video block, a held
  /// or dropped audio packet is not). `keyframe` is known for a
  /// BlockGroup; a SimpleBlock says so in its flags.
  fn queue_block(&mut self, data: &[u8], keyframe: Option<bool>) -> Result<bool, StreamError> {
    let (track, rel_ts, flags, header_len) = block_header(data)?;
    if flags & BLOCK_FLAG_LACING != 0 {
      // Lacing packs several frames per block; neither ffmpeg's WebM muxer
      // nor ours writes it for VP9 or Opus. Fail-soft: skip the block.
      log::warn!("[forge::video] skipping a laced block (unsupported)");
      return Ok(false);
    }
    let cluster_ts = self.cluster_ts.ok_or("block before its cluster timestamp")?;
    let ticks = cluster_ts as i64 + rel_ts as i64;
    let pts_us = (ticks as i128 * self.timestamp_scale_ns as i128 / 1000) as i64;
    if track == self.video_track {
      if !self.play_video {
        return Ok(false);
      }
      let sync = keyframe.unwrap_or(flags & BLOCK_FLAG_KEYFRAME != 0);
      if self.gating {
        if !sync {
          return Ok(false);
        }
        self.open_gate(pts_us);
      }
      self.video_q.push_back(VideoAu { pts_us, sync, data: data[header_len..].to_vec() });
      Ok(true)
    } else if Some(track) == self.audio_track {
      if !self.play_audio {
        return Ok(false);
      }
      let packet = AudioPacket { pts_us, data: data[header_len..].to_vec() };
      if self.gating {
        self.held_audio.push_back(packet);
        while self.held_audio.front().is_some_and(|p| p.pts_us < pts_us - GATING_HOLD_US) {
          self.held_audio.pop_front();
        }
        return Ok(false);
      }
      Ok(self.queue_audio(packet))
    } else {
      Ok(false)
    }
  }

  /// The epoch's first keyframe at `keyframe_us`: playback resumes there or
  /// at the target, whichever is later, and the audio held meanwhile is
  /// released from the preroll before that point.
  fn open_gate(&mut self, keyframe_us: i64) {
    self.gating = false;
    self.resume_us = self.epoch_target_us.max(keyframe_us);
    let preroll_us = self.info.audio.as_ref().map_or(0, |a| a.seek_preroll_us);
    self.audio_skip_until = Some(self.resume_us - preroll_us);
    while let Some(packet) = self.held_audio.pop_front() {
      self.queue_audio(packet);
    }
  }

  fn queue_audio(&mut self, packet: AudioPacket) -> bool {
    if self.audio_skip_until.is_some_and(|until| packet.pts_us < until) {
      return false;
    }
    self.audio_skip_until = None;
    self.audio_q.push_back(packet);
    true
  }

  /// Visit tail Cues the SeekHead points at, once, keeping the walk where
  /// it was. An interrupted visit does not count: the position stays for
  /// the newer seek that retries it, and the interruption is returned.
  fn load_cues(&mut self) -> Result<(), StreamError> {
    let Some(offset) = self.cues_position.take() else { return Ok(()) };
    let here = self.reader.pos;
    let loaded = self.reader.seek_to(self.segment_start + offset).and_then(|()| match self.reader.header()? {
      Some((ID_CUES, size)) => read_cues(&mut self.reader, size),
      _ => Err("seek head does not point at cues".into()),
    });
    match loaded {
      Ok(cues) => {
        let scale = self.timestamp_scale_ns;
        self.cues = cues.iter().map(|&(t, pos)| ((t as u128 * scale as u128 / 1000) as i64, pos)).collect();
      }
      Err(e) if e.kind == ErrorKind::Interrupted => {
        self.cues_position = Some(offset);
        return Err(e);
      }
      Err(e) => log::warn!("[forge::video] cues unavailable: {e}"),
    }
    if let Err(e) = self.reader.seek_to(here) {
      self.failed = Some(e);
    }
    Ok(())
  }
}

impl Demuxer for WebmDemuxer {
  fn info(&self) -> &MediaInfo {
    &self.info
  }

  fn next_video(&mut self) -> Result<Option<VideoAu>, StreamError> {
    if !self.play_video {
      return Ok(None);
    }
    while self.video_q.is_empty() && self.read_block()? {}
    Ok(self.video_q.pop_front())
  }

  fn next_audio(&mut self) -> Result<Option<AudioPacket>, StreamError> {
    if self.audio_track.is_none() || !self.play_audio {
      return Ok(None);
    }
    while self.audio_q.is_empty() && self.read_block()? {}
    Ok(self.audio_q.pop_front())
  }

  fn next_packet(&mut self) -> Result<Option<Packet>, StreamError> {
    loop {
      if let Some(au) = self.video_q.pop_front() {
        return Ok(Some(Packet::Video(au)));
      }
      if let Some(packet) = self.audio_q.pop_front() {
        return Ok(Some(Packet::Audio(packet)));
      }
      if !self.read_block()? {
        return Ok(None);
      }
    }
  }

  fn seek(&mut self, target_us: i64) -> Result<i64, StreamError> {
    if !self.info.seekable {
      return Err(StreamError::unsupported("source cannot seek"));
    }
    if self.cues.is_empty() {
      self.load_cues()?;
    }
    if self.cues.is_empty() {
      return Err(StreamError::unsupported("stream has no cues, cannot seek"));
    }
    self.failed = None;
    let at = self.cues.partition_point(|&(t, _)| t <= target_us);
    let (_, offset) = self.cues[at.saturating_sub(1)];
    self.reader.seek_to(self.segment_start + offset)?;
    match self.reader.header()? {
      Some((ID_CLUSTER, _)) => {}
      _ => return Err("cue points at something that is not a cluster".into()),
    }
    self.cluster_ts = None;
    self.video_q.clear();
    self.audio_q.clear();
    self.held_audio.clear();
    self.ended = false;
    self.epoch_target_us = target_us;
    self.resume_us = target_us;
    self.audio_skip_until = None;
    self.gating = self.play_video;
    if !self.gating {
      let preroll_us = self.info.audio.as_ref().map_or(0, |a| a.seek_preroll_us);
      self.audio_skip_until = Some(target_us - preroll_us);
    }
    while self.gating && self.read_block()? {}
    Ok(self.resume_us)
  }
}

struct TrackEntry {
  number: u64,
  kind: u64,
  codec: String,
  codec_private: Vec<u8>,
  seek_pre_roll_ns: u64,
  width: u32,
  height: u32,
  channels: u16,
  matrix: Option<u64>,
  range: Option<u64>,
}

fn read_tracks(reader: &mut Ebml, size: Option<u64>) -> Result<Vec<TrackEntry>, StreamError> {
  let end = reader.end_of(size).ok_or("tracks of unknown size")?;
  let mut tracks = Vec::new();
  while reader.pos < end {
    let (id, size) = reader.header()?.ok_or("truncated tracks")?;
    if id != ID_TRACK_ENTRY {
      reader.skip(size)?;
      continue;
    }
    let end = reader.end_of(size).ok_or("track entry of unknown size")?;
    let mut track = TrackEntry {
      number: 0,
      kind: 0,
      codec: String::new(),
      codec_private: Vec::new(),
      seek_pre_roll_ns: 0,
      width: 0,
      height: 0,
      channels: 0,
      matrix: None,
      range: None,
    };
    while reader.pos < end {
      let (id, size) = reader.header()?.ok_or("truncated track entry")?;
      match id {
        ID_TRACK_NUMBER => track.number = reader.uint(size)?,
        ID_TRACK_TYPE => track.kind = reader.uint(size)?,
        ID_CODEC_ID => track.codec = reader.string(size)?,
        ID_CODEC_PRIVATE => track.codec_private = reader.bytes(size)?,
        ID_SEEK_PRE_ROLL => track.seek_pre_roll_ns = reader.uint(size)?,
        ID_VIDEO => {
          let end = reader.end_of(size).ok_or("video element of unknown size")?;
          while reader.pos < end {
            let (id, size) = reader.header()?.ok_or("truncated video element")?;
            match id {
              ID_PIXEL_WIDTH => track.width = reader.uint(size)? as u32,
              ID_PIXEL_HEIGHT => track.height = reader.uint(size)? as u32,
              ID_COLOUR => {
                let end = reader.end_of(size).ok_or("colour of unknown size")?;
                while reader.pos < end {
                  let (id, size) = reader.header()?.ok_or("truncated colour")?;
                  match id {
                    ID_MATRIX_COEFFICIENTS => track.matrix = Some(reader.uint(size)?),
                    ID_RANGE => track.range = Some(reader.uint(size)?),
                    _ => reader.skip(size)?,
                  }
                }
              }
              _ => reader.skip(size)?,
            }
          }
        }
        ID_AUDIO => {
          let end = reader.end_of(size).ok_or("audio element of unknown size")?;
          while reader.pos < end {
            let (id, size) = reader.header()?.ok_or("truncated audio element")?;
            match id {
              ID_CHANNELS => track.channels = reader.uint(size)? as u16,
              _ => reader.skip(size)?,
            }
          }
        }
        _ => reader.skip(size)?,
      }
    }
    tracks.push(track);
  }
  Ok(tracks)
}

/// The Cues element as (cue time in ticks, cluster offset from the segment
/// start), one entry per cue point (the first track position of each).
fn read_cues(reader: &mut Ebml, size: Option<u64>) -> Result<Vec<(u64, u64)>, StreamError> {
  let end = reader.end_of(size).ok_or("cues of unknown size")?;
  let mut cues = Vec::new();
  while reader.pos < end {
    let (id, size) = reader.header()?.ok_or("truncated cues")?;
    if id != ID_CUE_POINT {
      reader.skip(size)?;
      continue;
    }
    let end = reader.end_of(size).ok_or("cue point of unknown size")?;
    let mut time = None;
    let mut cluster = None;
    while reader.pos < end {
      let (id, size) = reader.header()?.ok_or("truncated cue point")?;
      match id {
        ID_CUE_TIME => time = Some(reader.uint(size)?),
        ID_CUE_TRACK_POSITIONS => {
          let end = reader.end_of(size).ok_or("cue positions of unknown size")?;
          while reader.pos < end {
            let (id, size) = reader.header()?.ok_or("truncated cue positions")?;
            match id {
              ID_CUE_CLUSTER_POSITION => {
                let pos = reader.uint(size)?;
                cluster = cluster.or(Some(pos));
              }
              _ => reader.skip(size)?,
            }
          }
        }
        _ => reader.skip(size)?,
      }
    }
    if let (Some(time), Some(cluster)) = (time, cluster) {
      cues.push((time, cluster));
    }
  }
  cues.sort_unstable();
  Ok(cues)
}

/// The SeekHead's position of the Cues element, relative to the segment
/// start, when it lists one.
fn read_seek_head(reader: &mut Ebml, size: Option<u64>) -> Result<Option<u64>, StreamError> {
  let end = reader.end_of(size).ok_or("seek head of unknown size")?;
  let mut cues = None;
  while reader.pos < end {
    let (id, size) = reader.header()?.ok_or("truncated seek head")?;
    if id != ID_SEEK {
      reader.skip(size)?;
      continue;
    }
    let end = reader.end_of(size).ok_or("seek entry of unknown size")?;
    let mut target = 0;
    let mut position = None;
    while reader.pos < end {
      let (id, size) = reader.header()?.ok_or("truncated seek entry")?;
      match id {
        ID_SEEK_ID => target = reader.uint(size)? as u32,
        ID_SEEK_POSITION => position = Some(reader.uint(size)?),
        _ => reader.skip(size)?,
      }
    }
    if target == ID_CUES {
      cues = position;
    }
  }
  Ok(cues)
}

/// The Opus track's facts from its OpusHead; errs on a head the plain
/// decoder cannot take (more than two channels, a mapped layout).
fn opus_info(track: &TrackEntry) -> Result<AudioInfo, String> {
  let head = &track.codec_private;
  if head.len() < OPUS_HEAD_LEN || &head[..OPUS_HEAD_MAGIC.len()] != OPUS_HEAD_MAGIC {
    return Err("Opus track has no OpusHead".to_string());
  }
  let channels = head[OPUS_HEAD_CHANNELS] as u16;
  if channels == 0 || channels > OPUS_MAX_CHANNELS || head[OPUS_HEAD_MAPPING_FAMILY] != 0 {
    return Err(format!("Opus track has {channels} mapped channels; only mono and stereo play"));
  }
  if track.channels != 0 && track.channels != channels {
    log::warn!("[forge::video] Opus track says {} channels, its head {channels}", track.channels);
  }
  let pre_skip = u16::from_le_bytes([head[OPUS_HEAD_PRE_SKIP], head[OPUS_HEAD_PRE_SKIP + 1]]) as u32;
  // SeekPreRoll is in nanoseconds whatever the timestamp scale.
  Ok(AudioInfo {
    sample_rate: OPUS_SAMPLE_RATE,
    channels,
    pre_skip,
    seek_preroll_us: (track.seek_pre_roll_ns / 1000) as i64,
  })
}

/// A (Simple)Block's header: track number, timestamp relative to the
/// cluster, flags, and the header's length.
fn block_header(data: &[u8]) -> Result<(u64, i16, u8, usize), String> {
  let (track, len) = read_vint_slice(data).ok_or("bad block track number")?;
  let rest = &data[len..];
  if rest.len() < 3 {
    return Err("truncated block header".to_string());
  }
  let rel_ts = i16::from_be_bytes([rest[0], rest[1]]);
  Ok((track, rel_ts, rest[2], len + 3))
}

/// An EBML variable-length integer at the start of `data` (value, length),
/// with the length marker stripped.
fn read_vint_slice(data: &[u8]) -> Option<(u64, usize)> {
  let first = *data.first()?;
  let len = first.leading_zeros() as usize + 1;
  if len > 8 || data.len() < len {
    return None;
  }
  let mut value = (first & vint_mask(len)) as u64;
  for &b in &data[1..len] {
    value = (value << 8) | b as u64;
  }
  Some((value, len))
}

/// The value bits of a vint's first byte, for a vint of `len` bytes (an
/// 8-byte vint keeps none of them).
fn vint_mask(len: usize) -> u8 {
  (0xFFu16 >> len) as u8
}

/// The byte cursor: EBML element headers and payloads over a buffered
/// source, tracking its own offset. Sizes are checked here, before they
/// reach an allocation or a read-through skip.
struct Ebml {
  inner: BufReader<SeekableReader>,
  pos: u64,
  seekable: bool,
}

impl Ebml {
  fn new(source: SeekableReader, seekable: bool) -> Ebml {
    Ebml { inner: BufReader::new(source), pos: 0, seekable }
  }

  // A read that failed: the source's fault (network), unless it was the
  // source's interrupt (WouldBlock), a command for the reading thread.
  fn io_error(&self, what: &str, e: io::Error) -> StreamError {
    let kind = if e.kind() == io::ErrorKind::WouldBlock { ErrorKind::Interrupted } else { ErrorKind::Network };
    StreamError::new(kind, format!("{what} at {}: {e}", self.pos))
  }

  /// The stream's first four bytes, an element id or not.
  fn magic(&mut self) -> Result<[u8; 4], StreamError> {
    let mut magic = [0u8; 4];
    self.inner.read_exact(&mut magic).map_err(|e| self.io_error("read", e))?;
    self.pos += magic.len() as u64;
    Ok(magic)
  }

  /// The next element's id and data size (None for an unknown size); None
  /// at a clean end of stream.
  fn header(&mut self) -> Result<Option<(u32, Option<u64>)>, StreamError> {
    let mut first = [0u8; 1];
    match self.inner.read(&mut first) {
      Ok(0) => return Ok(None),
      Ok(_) => self.pos += 1,
      Err(e) => return Err(self.io_error("read", e)),
    }
    // Ids keep their length marker; a 4-byte id is the longest allowed.
    let len = first[0].leading_zeros() as usize + 1;
    if len > 4 {
      return Err(format!("bad element id at {}", self.pos - 1).into());
    }
    let mut id = first[0] as u32;
    for _ in 1..len {
      id = (id << 8) | self.byte()? as u32;
    }
    let size = self.vint()?;
    Ok(Some((id, size)))
  }

  /// A size vint: None when every value bit is set (unknown size).
  fn vint(&mut self) -> Result<Option<u64>, StreamError> {
    let first = self.byte()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
      return Err(format!("bad element size at {}", self.pos - 1).into());
    }
    let mask = vint_mask(len);
    let mut value = (first & mask) as u64;
    let mut all_ones = first & mask == mask;
    for _ in 1..len {
      let b = self.byte()?;
      all_ones &= b == 0xFF;
      value = (value << 8) | b as u64;
    }
    Ok(if all_ones { None } else { Some(value) })
  }

  /// Where an element of `size` ends, given the cursor is at its data.
  fn end_of(&self, size: Option<u64>) -> Option<u64> {
    size.map(|s| self.pos + s)
  }

  fn byte(&mut self) -> Result<u8, StreamError> {
    let mut b = [0u8; 1];
    self.inner.read_exact(&mut b).map_err(|e| self.io_error("read", e))?;
    self.pos += 1;
    Ok(b[0])
  }

  /// A size the stream can ask for; checked on the u64 before any cast.
  fn bounded(&self, size: u64) -> Result<usize, StreamError> {
    if size > MAX_ELEMENT_BYTES {
      return Err(format!("element of {size} bytes at {} exceeds the {MAX_ELEMENT_BYTES} byte limit", self.pos).into());
    }
    Ok(size as usize)
  }

  fn bytes(&mut self, size: Option<u64>) -> Result<Vec<u8>, StreamError> {
    let size = size.ok_or("binary element of unknown size")?;
    let len = self.bounded(size)?;
    let mut buf = vec![0u8; len];
    self.inner.read_exact(&mut buf).map_err(|e| self.io_error(&format!("read {size} bytes"), e))?;
    self.pos += size;
    Ok(buf)
  }

  fn uint(&mut self, size: Option<u64>) -> Result<u64, StreamError> {
    let bytes = self.bytes(size)?;
    if bytes.len() > 8 {
      return Err("integer element too long".into());
    }
    Ok(bytes.iter().fold(0u64, |v, &b| (v << 8) | b as u64))
  }

  fn float(&mut self, size: Option<u64>) -> Result<f64, StreamError> {
    let bytes = self.bytes(size)?;
    match bytes.len() {
      4 => Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64),
      8 => Ok(f64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])),
      n => Err(format!("float element of {n} bytes").into()),
    }
  }

  fn string(&mut self, size: Option<u64>) -> Result<String, StreamError> {
    let bytes = self.bytes(size)?;
    // Strings may be zero-padded to a fixed width.
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
  }

  /// Past an element. On a source that cannot seek this reads the bytes
  /// through, so the size is bounded like an allocation; a seekable
  /// source's own policy decides between reading through and restarting.
  fn skip(&mut self, size: Option<u64>) -> Result<(), StreamError> {
    let size = size.ok_or("cannot skip an element of unknown size")?;
    if !self.seekable {
      self.bounded(size)?;
    }
    self.seek_to(self.pos + size)
  }

  fn seek_to(&mut self, pos: u64) -> Result<(), StreamError> {
    // Relative to where the inner reader really is, not to `self.pos`: a
    // read the source interrupted part-way (blocked on a stalled body)
    // consumed bytes the walk never counted. Relative seeks keep the
    // buffer when the target is inside it.
    let here = self.inner.stream_position().map_err(|e| self.io_error("position", e))?;
    let delta = pos as i64 - here as i64;
    self.inner.seek_relative(delta).map_err(|e| self.io_error(&format!("seek to {pos}"), e))?;
    self.pos = pos;
    Ok(())
  }
}
