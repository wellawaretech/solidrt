// EBML and Matroska element ids, as they appear in the stream (the id's own
// length bits included), and the format facts the reader and the writer
// share: track types, codec ids, the block header flags, the Colour codes
// and the OpusHead layout. The reader's walking policy and the writer's
// cluster policy stay in their own files. An id only the writer uses is
// gated with it.

pub(super) const ID_EBML: u32 = 0x1A45DFA3;
#[cfg(feature = "video-encode")]
pub(super) const ID_EBML_VERSION: u32 = 0x4286;
#[cfg(feature = "video-encode")]
pub(super) const ID_EBML_READ_VERSION: u32 = 0x42F7;
#[cfg(feature = "video-encode")]
pub(super) const ID_EBML_MAX_ID_LENGTH: u32 = 0x42F2;
#[cfg(feature = "video-encode")]
pub(super) const ID_EBML_MAX_SIZE_LENGTH: u32 = 0x42F3;
pub(super) const ID_DOCTYPE: u32 = 0x4282;
#[cfg(feature = "video-encode")]
pub(super) const ID_DOCTYPE_VERSION: u32 = 0x4287;
#[cfg(feature = "video-encode")]
pub(super) const ID_DOCTYPE_READ_VERSION: u32 = 0x4285;
pub(super) const ID_SEGMENT: u32 = 0x18538067;
pub(super) const ID_SEEK_HEAD: u32 = 0x114D9B74;
pub(super) const ID_SEEK: u32 = 0x4DBB;
pub(super) const ID_SEEK_ID: u32 = 0x53AB;
pub(super) const ID_SEEK_POSITION: u32 = 0x53AC;
pub(super) const ID_INFO: u32 = 0x1549A966;
pub(super) const ID_TIMESTAMP_SCALE: u32 = 0x2AD7B1;
pub(super) const ID_DURATION: u32 = 0x4489;
#[cfg(feature = "video-encode")]
pub(super) const ID_MUXING_APP: u32 = 0x4D80;
#[cfg(feature = "video-encode")]
pub(super) const ID_WRITING_APP: u32 = 0x5741;
pub(super) const ID_TRACKS: u32 = 0x1654AE6B;
pub(super) const ID_TRACK_ENTRY: u32 = 0xAE;
pub(super) const ID_TRACK_NUMBER: u32 = 0xD7;
#[cfg(feature = "video-encode")]
pub(super) const ID_TRACK_UID: u32 = 0x73C5;
pub(super) const ID_TRACK_TYPE: u32 = 0x83;
pub(super) const ID_CODEC_ID: u32 = 0x86;
pub(super) const ID_CODEC_PRIVATE: u32 = 0x63A2;
#[cfg(feature = "video-encode")]
pub(super) const ID_CODEC_DELAY: u32 = 0x56AA;
pub(super) const ID_SEEK_PRE_ROLL: u32 = 0x56BB;
pub(super) const ID_VIDEO: u32 = 0xE0;
pub(super) const ID_PIXEL_WIDTH: u32 = 0xB0;
pub(super) const ID_PIXEL_HEIGHT: u32 = 0xBA;
pub(super) const ID_COLOUR: u32 = 0x55B0;
pub(super) const ID_MATRIX_COEFFICIENTS: u32 = 0x55B1;
pub(super) const ID_RANGE: u32 = 0x55B9;
pub(super) const ID_AUDIO: u32 = 0xE1;
#[cfg(feature = "video-encode")]
pub(super) const ID_SAMPLING_FREQUENCY: u32 = 0xB5;
pub(super) const ID_CHANNELS: u32 = 0x9F;
pub(super) const ID_CUES: u32 = 0x1C53BB6B;
pub(super) const ID_CUE_POINT: u32 = 0xBB;
pub(super) const ID_CUE_TIME: u32 = 0xB3;
pub(super) const ID_CUE_TRACK_POSITIONS: u32 = 0xB7;
#[cfg(feature = "video-encode")]
pub(super) const ID_CUE_TRACK: u32 = 0xF7;
pub(super) const ID_CUE_CLUSTER_POSITION: u32 = 0xF1;
pub(super) const ID_CLUSTER: u32 = 0x1F43B675;
pub(super) const ID_CLUSTER_TIMESTAMP: u32 = 0xE7;
pub(super) const ID_SIMPLE_BLOCK: u32 = 0xA3;
pub(super) const ID_BLOCK_GROUP: u32 = 0xA0;
pub(super) const ID_BLOCK: u32 = 0xA1;
pub(super) const ID_REFERENCE_BLOCK: u32 = 0xFB;
pub(super) const ID_DISCARD_PADDING: u32 = 0x75A2;

pub(super) const TRACK_TYPE_VIDEO: u64 = 1;
pub(super) const TRACK_TYPE_AUDIO: u64 = 2;
pub(super) const CODEC_VP9: &str = "V_VP9";
pub(super) const CODEC_OPUS: &str = "A_OPUS";
// TimestampScale default: cluster and block times in milliseconds.
pub(super) const DEFAULT_TIMESTAMP_SCALE_NS: u64 = 1_000_000;
// Block header flags: keyframe (SimpleBlock only) and the lacing field.
pub(super) const BLOCK_FLAG_KEYFRAME: u8 = 0x80;
pub(super) const BLOCK_FLAG_LACING: u8 = 0x06;
// Colour: MatrixCoefficients codes (ITU-T H.273) BT.709 and the two
// BT.601 codes; anything else (2 = unspecified, what ffmpeg writes) falls
// back to the resolution default. Range: 1 = broadcast (studio), 2 = full.
pub(super) const MATRIX_BT709: u64 = 1;
pub(super) const MATRIX_BT470BG: u64 = 5;
pub(super) const MATRIX_BT601: u64 = 6;
#[cfg(feature = "video-encode")]
pub(super) const RANGE_STUDIO: u64 = 1;
pub(super) const RANGE_FULL: u64 = 2;
// OpusHead (RFC 7845): magic, version, channels, pre-skip (u16 LE), input
// sample rate (u32 LE), output gain (i16 LE), channel mapping family.
pub(super) const OPUS_HEAD_MAGIC: &[u8] = b"OpusHead";
#[cfg(feature = "video-encode")]
pub(super) const OPUS_HEAD_VERSION: u8 = 1;
pub(super) const OPUS_HEAD_LEN: usize = 19;
pub(super) const OPUS_HEAD_CHANNELS: usize = 9;
pub(super) const OPUS_HEAD_PRE_SKIP: usize = 10;
#[cfg(feature = "video-encode")]
pub(super) const OPUS_HEAD_INPUT_RATE: usize = 12;
#[cfg(feature = "video-encode")]
pub(super) const OPUS_HEAD_OUTPUT_GAIN: usize = 16;
pub(super) const OPUS_HEAD_MAPPING_FAMILY: usize = 18;
// The seek preroll an Opus stream asks for (RFC 7845): decoding starts
// this far before a seek target so the decoder has converged.
#[cfg(feature = "video-encode")]
pub(super) const OPUS_SEEK_PREROLL_NS: u64 = 80_000_000;
