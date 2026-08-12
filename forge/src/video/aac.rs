// AAC audio decode via symphonia, used as a pure decoder on the raw frames
// the demuxer hands over (the demuxer synthesizes the AudioSpecificConfig
// from the container facts). Pure Rust, and AAC-LC's patents have expired,
// so unlike the video side this is fine to ship everywhere. Symphonia also
// grows into the SDL3_mixer replacement later (staging item 6 of
// okf/backlog/video-playback.md).

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_AAC};
use symphonia::core::formats::Packet;

use super::AudioInfo;

/// Decoded PCM: interleaved f32 samples at the stream's rate and channel
/// count (constant per stream, carried here so a chunk is self-describing).
pub struct PcmChunk {
  pub pts_us: i64,
  pub sample_rate: u32,
  pub channels: u16,
  pub samples: Vec<f32>,
}

pub struct AacDecoder {
  inner: Box<dyn Decoder>,
  sample_rate: u32,
}

impl AacDecoder {
  pub fn new(info: &AudioInfo) -> Result<Self, String> {
    let mut params = CodecParameters::new();
    params
      .for_codec(CODEC_TYPE_AAC)
      .with_sample_rate(info.sample_rate)
      .with_extra_data(info.asc.clone().into_boxed_slice());
    let inner = symphonia::default::get_codecs()
      .make(&params, &DecoderOptions::default())
      .map_err(|e| format!("create aac decoder: {e}"))?;
    Ok(AacDecoder { inner, sample_rate: info.sample_rate })
  }

  /// Decode one raw AAC frame to interleaved f32 PCM.
  pub fn decode(&mut self, pts_us: i64, data: &[u8]) -> Result<PcmChunk, String> {
    // Track id and duration are only bookkeeping to symphonia's decode.
    let packet = Packet::new_from_slice(0, 0, 0, data);
    let decoded = self.inner.decode(&packet).map_err(|e| format!("aac decode: {e}"))?;
    let spec = *decoded.spec();
    let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
    buf.copy_interleaved_ref(decoded);
    Ok(PcmChunk {
      pts_us,
      sample_rate: self.sample_rate,
      channels: spec.channels.count() as u16,
      samples: buf.samples().to_vec(),
    })
  }
}
