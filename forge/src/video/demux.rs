// MP4 demux via the `mp4` crate. Probed 2026-08-12: symphonia's isomp4
// reader delivers the H.264 track's packets but not the avcC extra data,
// and in MP4 the SPS/PPS live only there - so the container is read with
// `mp4` (avcC, sync flags, per-track timescales, AAC config) and symphonia
// stays a pure audio decoder (see aac.rs).
//
// Video samples come out of the container as AVCC (length-prefixed NALs);
// the demuxer normalizes them to Annex-B access units with SPS/PPS
// prepended at every sync sample, which is what both openh264 and
// AMediaCodec want fed.

use std::io::{Seek, SeekFrom};

use crate::seek::SeekableReader;

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

/// One Annex-B video access unit, SPS/PPS prepended when `sync`.
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
  video_track: u32,
  video_timescale: u32,
  video_next: u32,
  video_count: u32,
  sps: Vec<u8>,
  pps: Vec<u8>,
  audio_track: Option<(u32, u32)>,
  audio_next: u32,
  audio_count: u32,
}

impl Mp4Demuxer {
  /// Open an MP4 and read its header. The path resolves like every forge
  /// file read (through the assets mount when one is set, so packed apps
  /// work unchanged). Errs when the file is not a readable MP4 or has no
  /// H.264 video track. A missing or non-AAC audio track is not an error:
  /// `info().audio` is None and playback is silent.
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
      Ok(mp4::MediaType::H264) => {}
      other => return Err(format!("unsupported video codec {other:?} (H.264 only for now)")),
    }
    let sps = vtrack.sequence_parameter_set().map_err(|e| format!("read SPS: {e}"))?.to_vec();
    let pps = vtrack.picture_parameter_set().map_err(|e| format!("read PPS: {e}"))?.to_vec();

    let audio_info = audio.and_then(|(id, track)| match audio_config(track) {
      Ok(info) => Some((id, track.timescale(), info)),
      Err(e) => {
        log::warn!("[forge::video] ignoring audio track: {e}");
        None
      }
    });

    let info = MediaInfo {
      width: vtrack.width() as u32,
      height: vtrack.height() as u32,
      duration_us: reader.duration().as_micros() as i64,
      frame_count: vtrack.sample_count(),
      audio: audio_info.as_ref().map(|(_, _, a)| AudioInfo { sample_rate: a.sample_rate, channels: a.channels, asc: a.asc.clone() }),
    };
    Ok(Mp4Demuxer {
      video_timescale: vtrack.timescale(),
      video_count: vtrack.sample_count(),
      audio_track: audio_info.as_ref().map(|&(id, ts, _)| (id, ts)),
      audio_count: audio_info.as_ref().and_then(|(id, _, _)| reader.tracks().get(id)).map(|t| t.sample_count()).unwrap_or(0),
      reader,
      info,
      video_track,
      video_next: 1,
      sps,
      pps,
      audio_next: 1,
    })
  }

  pub fn info(&self) -> &MediaInfo {
    &self.info
  }

  /// The layout decoded frames will arrive in is the DECODER's business, not
  /// the container's; this is the color default: BT.709 for HD (720 lines
  /// and up), BT.601 below, absent explicit container metadata (the mp4
  /// crate exposes none, matching the probed TV stream which carried none).
  pub fn color_is_bt709(&self) -> bool {
    self.info.height >= 720
  }

  /// Next video access unit in decode order, None past the end.
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
    // Presentation time: decode time plus the composition offset (the
    // rendering offset is 0 for streams without reordering).
    let ts = sample.start_time as i64 + sample.rendering_offset as i64;
    let pts_us = ts * 1_000_000 / self.video_timescale as i64;
    let mut data = Vec::with_capacity(sample.bytes.len() + 16);
    if sample.is_sync {
      data.extend_from_slice(&[0, 0, 0, 1]);
      data.extend_from_slice(&self.sps);
      data.extend_from_slice(&[0, 0, 0, 1]);
      data.extend_from_slice(&self.pps);
    }
    avcc_to_annexb(&sample.bytes, &mut data)?;
    Ok(Some(VideoAu { pts_us, sync: sample.is_sync, data }))
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

  /// Raw SPS and PPS from the avcC box (no start codes), for decoder
  /// configuration (MediaCodec csd).
  pub fn parameter_sets(&self) -> (&[u8], &[u8]) {
    (&self.sps, &self.pps)
  }
}

/// AVCC sample (length-prefixed NALs) -> Annex-B start codes, appended to
/// `out`. The NAL length size is practically always 4; a sample that does
/// not parse as 4-byte lengths errs rather than feeding garbage downstream.
fn avcc_to_annexb(sample: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
  let mut i = 0;
  while i < sample.len() {
    if i + 4 > sample.len() {
      return Err("truncated NAL length".to_string());
    }
    let len = u32::from_be_bytes([sample[i], sample[i + 1], sample[i + 2], sample[i + 3]]) as usize;
    i += 4;
    if len == 0 || i + len > sample.len() {
      return Err(format!("NAL length {len} out of bounds (not 4-byte AVCC?)"));
    }
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(&sample[i..i + len]);
    i += len;
  }
  Ok(())
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
