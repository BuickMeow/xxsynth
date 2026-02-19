use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// 提前为你规划好的数据结构，这部分代码保留，用于之后对接 GUI 和本地存档。
// 即使目前处于 Headless 模式，依然可以使用这些结构体。

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub soundfonts: Vec<SoundfontEntry>,
    pub realtime: RealtimeConfig,
    pub render: RenderConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            soundfonts: Vec::new(),
            realtime: RealtimeConfig::default(),
            render: RenderConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioChannels { Mono, Stereo }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadCount { None, Auto, Manual(usize) }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolator { None, Nearest, Linear }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeCurveType { Linear, Exponential }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthFormat {
    Midi,
    Custom { channels: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeOptions {
    pub attack_curve: EnvelopeCurveType,
    pub decay_curve: EnvelopeCurveType,
    pub release_curve: EnvelopeCurveType,
}

impl Default for EnvelopeOptions {
    fn default() -> Self {
        Self {
            attack_curve: EnvelopeCurveType::Exponential,
            decay_curve: EnvelopeCurveType::Linear,
            release_curve: EnvelopeCurveType::Linear,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundfontEntry {
    pub enabled: bool,
    pub path: PathBuf,
    pub bank: Option<u32>,
    pub preset: Option<u32>,
    pub vol_envelope_options: EnvelopeOptions,
    pub use_effects: bool,
    pub interpolator: Interpolator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeConfig {
    pub render_window_ms: f64,
    pub format: SynthFormat,
    pub channel_threading: ThreadCount,
    pub key_threading: ThreadCount,
    pub ignore_range_start: u8,
    pub ignore_range_end: u8,
    pub ignore_range_exhausted: bool,
    pub input_ports: Vec<Option<String>>, 
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            render_window_ms: 10.0,
            format: SynthFormat::Custom { channels: 256 },
            channel_threading: ThreadCount::Auto,
            key_threading: ThreadCount::Auto,
            ignore_range_start: 0,
            ignore_range_end: 0,
            ignore_range_exhausted: false,
            input_ports: vec![None; 16],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    pub input_midi: Option<PathBuf>,
    pub output_path: PathBuf,
    pub sample_rate: u32,
    pub audio_channels: AudioChannels,
    pub layers: u32,
    pub channel_threading: ThreadCount,
    pub key_threading: ThreadCount,
    pub limiter: bool,
    pub disable_fade_out: bool,
    pub linear_envelope: bool,
    pub interpolation: Interpolator,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            input_midi: None,
            output_path: PathBuf::from("output.wav"),
            sample_rate: 48000,
            audio_channels: AudioChannels::Stereo,
            layers: 32,
            channel_threading: ThreadCount::Auto,
            key_threading: ThreadCount::Auto,
            limiter: true,
            disable_fade_out: false,
            linear_envelope: false,
            interpolation: Interpolator::Linear,
        }
    }
}