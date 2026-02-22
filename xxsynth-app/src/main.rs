#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // 隐藏控制台窗口

mod audio;
mod config;

use eframe::egui;
use std::path::PathBuf;
use std::process::Command;
use std::fs;

use config::{InterpolatorWrapper, RealtimeConfig, RenderConfig};
use audio::{spawn_audio_thread, AudioEngineHandle};

const MIDI_PORT_NAME: &str = "midi7";

#[derive(PartialEq)]
enum Tab {
    Soundfonts,
    RealtimeSettings,
    RenderSettings,
}

struct XXSynthApp {
    active_tab: Tab,
    soundfonts: Vec<PathBuf>,
    realtime_config: RealtimeConfig,
    render_config: RenderConfig,
    
    // 运行状态
    audio_handle: Option<AudioEngineHandle>,
    status_message: String,
}

// 本地持久化保存结构
#[derive(serde::Serialize, serde::Deserialize)]
struct AppSettings {
    soundfonts: Vec<PathBuf>,
    udp_port: u16,
    total_channels: u32,
    render_window_ms: f64,
    thread_count: usize,
    interpolator: u8,
    ignore_velocity_min: u8,
    ignore_velocity_max: u8,
}

impl AppSettings {
    fn load() -> Self {
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

    fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write("xxsynth_settings.json", data);
        }
    }
}

impl XXSynthApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 配置中文字体
        Self::setup_custom_fonts(&cc.egui_ctx);

        // 自动写入注册表
        Self::register_midi_port();

        // 1. 加载本地设置
        let settings = AppSettings::load();
        
        let mut realtime_config = RealtimeConfig::default();
        realtime_config.udp_port = settings.udp_port;
        realtime_config.total_channels = settings.total_channels;
        realtime_config.render_window_ms = settings.render_window_ms;
        realtime_config.thread_count = settings.thread_count;
        realtime_config.interpolator = if settings.interpolator == 1 { InterpolatorWrapper::Linear } else { InterpolatorWrapper::Nearest };
        realtime_config.ignore_velocity_min = settings.ignore_velocity_min;
        realtime_config.ignore_velocity_max = settings.ignore_velocity_max;

        let mut app = Self {
            active_tab: Tab::Soundfonts,
            soundfonts: settings.soundfonts.clone(),
            realtime_config: realtime_config.clone(),
            render_config: RenderConfig::default(),
            audio_handle: None,
            status_message: "正在自动启动引擎...".to_string(),
        };

        // 2. 默认自动启动引擎
        if app.soundfonts.is_empty() {
            app.status_message = "警告：没有加载任何音色库，将不会有声音。".to_string();
        }
        match spawn_audio_thread(app.realtime_config.clone(), app.soundfonts.clone()) {
            Ok(handle) => {
                app.audio_handle = Some(handle);
                app.status_message = format!("已自动启动引擎。监听 UDP 端口 {}", app.realtime_config.udp_port);
            }
            Err(e) => {
                app.status_message = format!("自动启动失败: {}", e);
            }
        }

        app
    }

    fn setup_custom_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        // 尝试加载 Windows 自带的微软雅黑字体
        let font_path = "C:\\Windows\\Fonts\\msyh.ttc";
        if let Ok(font_data) = std::fs::read(font_path) {
            // 注意这里：egui 新版本要求传入 Arc 包裹的 FontData
            fonts.font_data.insert(
                "msyh".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );

            // 将微软雅黑设置为首选字体
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                vec.insert(0, "msyh".to_owned());
            }
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                vec.insert(0, "msyh".to_owned());
            }
        } else {
            eprintln!("警告: 找不到微软雅黑字体 ({})，中文可能无法正常显示。", font_path);
        }

        ctx.set_fonts(fonts);
    }

    fn register_midi_port() {
        println!("尝试将虚拟 MIDI 端口 [{}] 写入注册表...", MIDI_PORT_NAME);
        let reg_key = "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Drivers32";
        let status = Command::new("reg")
            .args(&["add", reg_key, "/v", MIDI_PORT_NAME, "/t", "REG_SZ", "/d", "xxsynth_winmm.dll", "/f"])
            .status();

        match status {
            Ok(s) if s.success() => println!("注册表写入成功！(端口: {})", MIDI_PORT_NAME),
            _ => eprintln!("注册表写入失败！请确保你以【管理员身份】运行此程序。"),
        }
    }

    fn is_running(&self) -> bool {
        self.audio_handle.is_some()
    }
}

impl eframe::App for XXSynthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 顶部导航栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Soundfonts, "🎹 音色库");
                ui.selectable_value(&mut self.active_tab, Tab::RealtimeSettings, "\u{2699} 实时设置");
                ui.selectable_value(&mut self.active_tab, Tab::RenderSettings, "🎬 渲染导出");
            });
        });

        // 底部状态栏
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let status_color = if self.is_running() { 
                    egui::Color32::from_rgba_unmultiplied(0, 200, 0, 255) 
                } else { 
                    egui::Color32::from_rgba_unmultiplied(200, 0, 0, 255) 
                };
                ui.colored_label(status_color, if self.is_running() { "● 正在运行" } else { "○ 已停止" });
                ui.separator();
                ui.label(&self.status_message);
            });
        });

        // 中央内容区
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Soundfonts => self.ui_soundfonts(ui),
                Tab::RealtimeSettings => self.ui_realtime(ui),
                Tab::RenderSettings => self.ui_render(ui),
            }
        });
    }
}

// === 以下为 UI 渲染逻辑分离 ===
impl XXSynthApp {
    fn ui_soundfonts(&mut self, ui: &mut egui::Ui) {
        ui.heading("已加载的音色库 (SF2 / SFZ)");
        ui.label("注意: 列表顺序即为加载顺序，上方的音色如果遇到相同的预设 / 乐器会覆盖下方的。");
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("➕ 添加音色文件...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Soundfonts", &["sf2", "sfz"])
                    .pick_file() 
                {
                    self.soundfonts.push(path);
                }
            }
            if ui.button("\u{1F5D1} 清空列表").clicked() {
                self.soundfonts.clear();
            }
        });

        ui.add_space(10.0);

        let mut to_remove = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, path) in self.soundfonts.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", i + 1));
                    if ui.button("❌").clicked() {
                        to_remove = Some(i);
                    }
                    ui.label(egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong());
                });
                ui.label(egui::RichText::new(path.to_string_lossy()).small().weak());
                ui.separator();
            }
        });

        if let Some(i) = to_remove {
            self.soundfonts.remove(i);
        }
    }

    fn ui_realtime(&mut self, ui: &mut egui::Ui) {
        ui.heading("实时播放参数");
        ui.label("修改参数后点击下方【应用更改】即可重启引擎并保存到本地。");
        ui.separator();

        let is_running = self.is_running();

        // 【修复 E0502】使用作用域限定对 self.realtime_config 的可变借用生命周期
        {
            let cfg = &mut self.realtime_config;

            // 移除 add_enabled_ui 限制，让引擎运行时依然可以修改参数
            egui::Grid::new("realtime_grid").num_columns(2).spacing([40.0, 10.0]).striped(true).show(ui, |ui| {
                ui.label("UDP 监听端口:");
                ui.add(egui::DragValue::new(&mut cfg.udp_port));
                ui.end_row();

                ui.label("总通道数:");
                ui.add(egui::DragValue::new(&mut cfg.total_channels).range(16..=256));
                ui.end_row();

                ui.label("缓冲区大小 (ms):");
                ui.add(egui::Slider::new(&mut cfg.render_window_ms, 1.0..=100.0).text("ms"));
                ui.end_row();

                ui.label("多线程数量:");
                ui.horizontal(|ui| {
                    let max_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(16);
                    ui.radio_value(&mut cfg.thread_count, 1, "单线程");
                    ui.radio_value(&mut cfg.thread_count, 0, "自动");
                    
                    // 【修复未使用 mut 警告】去掉这里的 mut
                    let is_custom = cfg.thread_count > 1;
                    if ui.radio(is_custom, "自定义:").clicked() {
                        if !is_custom { cfg.thread_count = max_threads / 2; }
                    }
                    if is_custom {
                        ui.add(egui::DragValue::new(&mut cfg.thread_count).range(2..=max_threads));
                    }
                });
                ui.end_row();

                ui.label("插值算法:");
                egui::ComboBox::from_id_salt("interp_combo")
                    .selected_text(cfg.interpolator.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut cfg.interpolator, InterpolatorWrapper::Nearest, "最近邻 (Nearest) - 极低CPU占用");
                        ui.selectable_value(&mut cfg.interpolator, InterpolatorWrapper::Linear, "线性 (Linear) - 音质平滑");
                    });
                ui.end_row();

                ui.label("忽略力度范围:");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut cfg.ignore_velocity_min).range(0..=127));
                    ui.label("至");
                    ui.add(egui::DragValue::new(&mut cfg.ignore_velocity_max).range(0..=127));
                });
                // 确保 min 不大于 max
                if cfg.ignore_velocity_min > cfg.ignore_velocity_max {
                    cfg.ignore_velocity_max = cfg.ignore_velocity_min;
                }
                ui.end_row();
            });
        } // `cfg` 的可变借用在这里结束

        ui.add_space(20.0);
        
        ui.horizontal(|ui| {
            // 应用更改按钮
            if ui.add_sized([200.0, 40.0], egui::Button::new(egui::RichText::new("🔄 应用更改并重启").heading())).clicked() {
                // 1. 停止旧引擎
                if let Some(mut handle) = self.audio_handle.take() {
                    handle.stop();
                }

                // 2. 保存设置到本地 JSON
                // 此时直接使用 &self.realtime_config 即可，不再有可变借用的冲突
                let cfg = &self.realtime_config;
                let settings = AppSettings {
                    soundfonts: self.soundfonts.clone(),
                    udp_port: cfg.udp_port,
                    total_channels: cfg.total_channels,
                    render_window_ms: cfg.render_window_ms,
                    thread_count: cfg.thread_count,
                    interpolator: if cfg.interpolator == InterpolatorWrapper::Linear { 1 } else { 0 },
                    ignore_velocity_min: cfg.ignore_velocity_min,
                    ignore_velocity_max: cfg.ignore_velocity_max,
                };
                settings.save();
                
                // 3. 启动新引擎
                match spawn_audio_thread(self.realtime_config.clone(), self.soundfonts.clone()) {
                    Ok(handle) => {
                        self.audio_handle = Some(handle);
                        self.status_message = format!("已应用更改。监听 UDP 端口 {}", self.realtime_config.udp_port);
                    }
                    Err(e) => {
                        self.status_message = format!("启动失败: {}", e);
                    }
                }
            }

            // 提供一个单独的停止按钮
            if is_running {
                ui.add_space(10.0);
                if ui.add_sized([100.0, 40.0], egui::Button::new("⏹ 停止引擎")).clicked() {
                    if let Some(mut handle) = self.audio_handle.take() {
                        handle.stop();
                    }
                    self.status_message = "音频引擎已手动停止。".to_string();
                }
            }
        });
    }

    fn ui_render(&mut self, ui: &mut egui::Ui) {
        ui.heading("离线渲染 (MIDI -> WAV)");
        ui.label("渲染功能正在开发中，即将接入 xsynth-render。");
        ui.separator();

        let cfg = &mut self.render_config;

        ui.horizontal(|ui| {
            ui.label("输入 MIDI:");
            if ui.button("📂 选择").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("MIDI", &["mid", "midi"]).pick_file() {
                    cfg.midi_path = path.to_string_lossy().to_string();
                }
            }
            ui.label(&cfg.midi_path);
        });

        ui.horizontal(|ui| {
            ui.label("输出 WAV:");
            if ui.button("💾 保存").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).save_file() {
                    cfg.output_path = path.to_string_lossy().to_string();
                }
            }
            ui.label(&cfg.output_path);
        });

        ui.add_space(20.0);

        if ui.button("🚀 开始渲染 (WIP)").clicked() {
            self.status_message = "渲染功能尚未完全实装。".to_string();
        }
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([650.0, 550.0])
            .with_title("XXSynth - Black MIDI Engine"),
        ..Default::default()
    };

    eframe::run_native(
        "xxsynth-app",
        options,
        Box::new(|cc| Ok(Box::new(XXSynthApp::new(cc)))),
    )
}