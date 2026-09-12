// MP4 demux via the `mp4` crate: it reads the vp09 sample entry (and its
// vpcC box: profile, bit depth, chroma layout, color range and matrix),
// sync flags, per-track timescales and the AAC config; symphonia stays a
// pure audio decoder (see aac.rs).
//
// VP9 needs no bitstream rewriting: a sample is one coded frame (or a
// superframe packing a hidden ALT-REF with its shown frame) exactly as the
// decoders want it fed, and VP9 has no out-of-band parameter sets.

use std::io::{Seek, SeekFrom};

use crate::seek::SeekableReader;

// vpcC facts the pipeline accepts: 8-bit 4:2:0 is VP9 profile 0, and the
// two chroma_subsampling values below 2 are both 4:2:0 (vertical vs.
// co-located chroma siting, which the upload does not distinguish).
const VP9_PROFILE_8BIT_420: u8 = 0;
const BIT_DEPTH_8: u8 = 8;
const CHROMA_420_MAX: u8 = 1;
// vpcC matrix_coefficients (ISO/IEC 23001-8): BT.709 and the two BT.601
// codes; anything else (2 = unspecified, what ffmpeg writes) falls back to
// the resolution default.
const MATRIX_BT709: u8 = 1;
const MATRIX_BT470BG: u8 = 5;
const MATRIX_BT601: u8 = 6;
// The resolution default when the container does not say: HD content (this
// many lines and up) is BT.709, SD is BT.601.
const HD_LINES: u32 = 720;

/// Stream facts read from the container header.
#[derive(Clone)]
pub struct MediaInfo {
  pub width: u32,
  pub height: u32,
  pub duration_us: i64,
  /// Frames in the video track (0 when the container does not say).
  pub frame_count: u32,
  pub audio: Option<AudioInfo>,
}

#[derive(Clone)]
pub struct AudioInfo {
  pub sample_rate: u32,
  pub channels: u16,
  /// AudioSpecificConfig for the AAC decoder, synthesized from the
  /// container's object type / frequency index / channel configuration.
  pub asc: Vec<u8>,
}

/// One coded VP9 frame (a superframe when the container packs a hidden
/// ALT-REF with its shown frame), exactly as stored; decoders split
/// superframes themselves. `sync` marks a keyframe.
pub struct VideoAu {
  pub pts_us: i64,
  pub sync: bool,
  pub data: Vec<u8>,
}

/// One raw AAC frame.
pub struct AudioPacket {
  pub pts_us: i64,
  pub data: Vec<u8>,
}

pub struct Mp4Demuxer {
  reader: mp4::Mp4Reader<SeekableReader>,
  info: MediaInfo,
  bt709: bool,
  full_range: bool,
  video_track: u32,
  video_timescale: u32,
  video_next: u32,
  video_count: u32,
  audio_track: Option<(u32, u32)>,
  audio_next: u32,
  audio_count: u32,
}

impl Mp4Demuxer {
  /// Open an MP4 and read its header. The path resolves like every forge
  /// file read (through the assets mount when one is set, so packed apps
  /// work unchanged). Errs when the file is not a readable MP4, has no VP9
  /// video track, or the VP9 stream is not 8-bit 4:2:0 (profile 0). A
  /// missing or non-AAC audio track is not an error: `info().audio` is
  /// None and playback is silent.
  pub fn open(path: &str) -> Result<Self, String> {
    let mut file = crate::fs::open_seekable(path)?;
    let size = file.seek(SeekFrom::End(0)).map_err(|e| format!("size {path}: {e}"))?;
    file.seek(SeekFrom::Start(0)).map_err(|e| format!("rewind {path}: {e}"))?;
    let reader = mp4::Mp4Reader::read_header(file, size).map_err(|e| format!("read mp4 header: {e}"))?;

    let mut video: Option<(u32, &mp4::Mp4Track)> = None;
    let mut audio: Option<(u32, &mp4::Mp4Track)> = None;
    for (&id, track) in reader.tracks() {
      match track.track_type() {
        Ok(mp4::TrackType::Video) if video.is_none() => video = Some((id, track)),
        Ok(mp4::TrackType::Audio) if audio.is_none() => audio = Some((id, track)),
        _ => {}
      }
    }
    let (video_track, vtrack) = video.ok_or_else(|| "no video track".to_string())?;
    match vtrack.media_type() {
      Ok(mp4::MediaType::VP9) => {}
      other => return Err(format!("unsupported video codec {other:?} (VP9 only)")),
    }
    let vpcc = &vtrack
      .trak
      .mdia
      .minf
      .stbl
      .stsd
      .vp09
      .as_ref()
      .ok_or_else(|| "VP9 track without a vp09 sample entry".to_string())?
      .vpcc;
    if vpcc.profile != VP9_PROFILE_8BIT_420 || vpcc.bit_depth != BIT_DEPTH_8 || vpcc.chroma_subsampling > CHROMA_420_MAX
    {
      return Err(format!(
        "unsupported VP9 stream: profile {}, {}-bit, chroma_subsampling {} (8-bit 4:2:0, profile 0, only)",
        vpcc.profile, vpcc.bit_depth, vpcc.chroma_subsampling
      ));
    }
    let height = vtrack.height() as u32;
    let bt709 = match vpcc.matrix_coefficients {
      MATRIX_BT709 => true,
      MATRIX_BT470BG | MATRIX_BT601 => false,
      _ => height >= HD_LINES,
    };
    let full_range = vpcc.video_full_range_flag;

    let audio_info = audio.and_then(|(id, track)| match audio_config(track) {
      Ok(info) => Some((id, track.timescale(), info)),
      Err(e) => {
        log::warn!("[forge::video] ignoring audio track: {e}");
        None
      }
    });

    let info = MediaInfo {
      width: vtrack.width() as u32,
      height,
      duration_us: reader.duration().as_micros() as i64,
      frame_count: vtrack.sample_count(),
      audio: audio_info.as_ref().map(|(_, _, a)| AudioInfo {
        sample_rate: a.sample_rate,
        channels: a.channels,
        asc: a.asc.clone(),
      }),
    };
    Ok(Mp4Demuxer {
      video_timescale: vtrack.timescale(),
      video_count: vtrack.sample_count(),
      audio_track: audio_info.as_ref().map(|&(id, ts, _)| (id, ts)),
      audio_count: audio_info
        .as_ref()
        .and_then(|(id, _, _)| reader.tracks().get(id))
        .map(|t| t.sample_count())
        .unwrap_or(0),
      reader,
      info,
      bt709,
      full_range,
      video_track,
      video_next: 1,
      audio_next: 1,
    })
  }

  pub fn info(&self) -> &MediaInfo {
    &self.info
  }

  /// The conversion matrix: what the container's vpcC says when it says
  /// anything, else BT.709 for HD (720 lines and up) and BT.601 below.
  pub fn color_is_bt709(&self) -> bool {
    self.bt709
  }

  /// Full-range (0..255) samples rather than studio range, from vpcC.
  pub fn color_is_full_range(&self) -> bool {
    self.full_range
  }

  /// Next coded video frame in decode order, None past the end.
  pub fn next_video(&mut self) -> Result<Option<VideoAu>, String> {
    if self.video_next > self.video_count {
      return Ok(None);
    }
    let id = self.video_next;
    self.video_next += 1;
    let sample = self
      .reader
      .read_sample(self.video_track, id)
      .map_err(|e| format!("read video sample {id}: {e}"))?
      .ok_or_else(|| format!("video sample {id} missing"))?;
    // Presentation time: decode time plus the composition offset (0 for
    // VP9, whose hidden frames travel inside superframes rather than as
    // reordered samples).
    let ts = sample.start_time as i64 + sample.rendering_offset as i64;
    let pts_us = ts * 1_000_000 / self.video_timescale as i64;
    Ok(Some(VideoAu { pts_us, sync: sample.is_sync, data: sample.bytes.to_vec() }))
  }

  /// Next raw AAC frame, None past the end or when there is no audio track.
  pub fn next_audio(&mut self) -> Result<Option<AudioPacket>, String> {
    let Some((track, timescale)) = self.audio_track else {
      return Ok(None);
    };
    if self.audio_next > self.audio_count {
      return Ok(None);
    }
    let id = self.audio_next;
    self.audio_next += 1;
    let sample = self
      .reader
      .read_sample(track, id)
      .map_err(|e| format!("read audio sample {id}: {e}"))?
      .ok_or_else(|| format!("audio sample {id} missing"))?;
    let pts_us = sample.start_time as i64 * 1_000_000 / timescale as i64;
    Ok(Some(AudioPacket { pts_us, data: sample.bytes.to_vec() }))
  }
}

struct AudioConfig {
  sample_rate: u32,
  channels: u16,
  asc: Vec<u8>,
}

/// Read the AAC configuration and synthesize the 2-byte AudioSpecificConfig
/// the decoder wants: object type (5 bits), frequency index (4), channel
/// configuration (4).
fn audio_config(track: &mp4::Mp4Track) -> Result<AudioConfig, String> {
  match track.media_type() {
    Ok(mp4::MediaType::AAC) => {}
    other => return Err(format!("unsupported audio codec {other:?} (AAC only for now)")),
  }
  let object = track.audio_profile().map_err(|e| format!("audio profile: {e}"))? as u8;
  let freq = track.sample_freq_index().map_err(|e| format!("sample rate: {e}"))?;
  let ch = track.channel_config().map_err(|e| format!("channels: {e}"))? as u8;
  let freq_index = freq as u8;
  let asc = vec![(object << 3) | (freq_index >> 1), ((freq_index & 1) << 7) | (ch << 3)];
  Ok(AudioConfig { sample_rate: freq.freq(), channels: ch as u16, asc })
}
