use xsynth_core::channel_group::ThreadCount;
use xsynth_core::soundfont::Interpolator;

// 实时配置结构体
#[derive(Clone)]
pub struct RealtimeConfig {
    pub render_window_ms: f64,
    pub thread_count: usize, // 0 为 Auto
    pub interpolator: InterpolatorWrapper,
    pub udp_port: u16,
    pub total_channels: u32,
    pub ignore_velocity_min: u8,
    pub ignore_velocity_max: u8,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            render_window_ms: 15.0,
            thread_count: 12, // 默认 12 线程
            interpolator: InterpolatorWrapper::Nearest,
            udp_port: 44444,
            total_channels: 64,
            ignore_velocity_min: 0,
            ignore_velocity_max: 0,
        }
    }
}

impl RealtimeConfig {
    pub fn get_thread_count(&self) -> ThreadCount {
        if self.thread_count == 0 {
            ThreadCount::Auto
        } else {
            ThreadCount::Manual(self.thread_count)
        }
    }

    pub fn get_interpolator(&self) -> Interpolator {
        match self.interpolator {
            InterpolatorWrapper::Nearest => Interpolator::Nearest,
            InterpolatorWrapper::Linear => Interpolator::Linear,
        }
    }
}

// 包装一下 Interpolator 以便在 UI 中使用
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum InterpolatorWrapper {
    Nearest,
    Linear,
}

impl ToString for InterpolatorWrapper {
    fn to_string(&self) -> String {
        match self {
            Self::Nearest => "最近邻 (Nearest) - 高性能".to_owned(),
            Self::Linear => "线性 (Linear) - 平滑".to_owned(),
        }
    }
}

// 渲染配置结构体
#[derive(Clone)]
pub struct RenderConfig {
    pub midi_path: String,
    pub output_path: String,
    pub sample_rate: u32,
    pub audio_channels: String,
    pub layers: u32,
    pub channel_threading: String,
    pub key_threading: String,
    pub apply_limiter: bool,
    pub disable_fade_out: bool,
    pub linear_envelope: bool,
    pub interpolation: String,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            midi_path: String::new(),
            output_path: "out.wav".to_string(),
            sample_rate: 48000,
            audio_channels: "stereo".to_string(),
            layers: 32,
            channel_threading: "auto".to_string(),
            key_threading: "auto".to_string(),
            apply_limiter: false,
            disable_fade_out: false,
            linear_envelope: false,
            interpolation: "linear".to_string(),
        }
    }
}