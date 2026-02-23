use std::fs;
use std::path::PathBuf;

// 本地持久化保存结构
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub soundfonts: Vec<PathBuf>,
    pub udp_port: u16,
    pub total_channels: u32,
    pub render_window_ms: f64,
    pub thread_count: usize,
    pub interpolator: u8,
    pub ignore_velocity_min: u8,
    pub ignore_velocity_max: u8,
}

impl AppSettings {
    pub fn load() -> Self {
        if let Ok(data) = fs::read_to_string("xxsynth_settings.json") {
            if let Ok(settings) = serde_json::from_str(&data) {
                return settings;
            }
        }
        // 默认值
        Self {
            soundfonts: vec![],
            udp_port: 44444,
            total_channels: 64,
            render_window_ms: 15.0,
            thread_count: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(12),
            interpolator: 0,
            ignore_velocity_min: 0,
            ignore_velocity_max: 0,
        }
    }

    pub fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write("xxsynth_settings.json", data);
        }
    }
}