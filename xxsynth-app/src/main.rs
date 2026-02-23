#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // 隐藏控制台窗口

mod audio;
mod config;
mod settings; // 新增模块：本地持久化设置
mod ui;       // 新增模块：UI 细节渲染

use eframe::egui;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use config::{InterpolatorWrapper, RealtimeConfig, RenderConfig};
use audio::{spawn_audio_thread, AudioEngineHandle};
use settings::AppSettings;

const MIDI_PORT_NAME: &str = "midi7";

#[derive(PartialEq)]
pub(crate) enum Tab {
    Soundfonts,
    RealtimeSettings,
    RenderSettings,
}

pub(crate) struct XXSynthApp {
    pub(crate) active_tab: Tab,
    pub(crate) soundfonts: Vec<PathBuf>,
    pub(crate) realtime_config: RealtimeConfig,
    pub(crate) render_config: RenderConfig,
    
    // 运行状态与脏标记
    pub(crate) audio_handle: Option<AudioEngineHandle>,
    pub(crate) status_message: String,
    pub(crate) is_dirty: bool, // 是否有未保存/未重启的修改
    
    // 加载/渲染进度状态
    pub(crate) load_progress: Arc<Mutex<f32>>,
    pub(crate) is_rendering: Arc<AtomicBool>,
    pub(crate) render_progress: Arc<Mutex<f32>>,
    pub(crate) render_error: Arc<Mutex<Option<String>>>,
}

impl XXSynthApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 配置中文字体
        Self::setup_custom_fonts(&cc.egui_ctx);

        // 自动写入注册表 (带智能提权)
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
            realtime_config,
            render_config: RenderConfig::default(),
            audio_handle: None,
            status_message: "正在准备引擎...".to_string(),
            is_dirty: false,
            load_progress: Arc::new(Mutex::new(0.0)),
            is_rendering: Arc::new(AtomicBool::new(false)),
            render_progress: Arc::new(Mutex::new(0.0)),
            render_error: Arc::new(Mutex::new(None)),
        };

        // 2. 默认自动启动引擎
        if app.soundfonts.is_empty() {
            app.status_message = "警告：没有加载任何音色库，将不会有声音。".to_string();
        }
        
        // 统一调用重启流程
        app.restart_engine();

        app
    }

    /// 统一的引擎重启流程
    pub(crate) fn restart_engine(&mut self) {
        // 1. 停止旧引擎
        if let Some(mut handle) = self.audio_handle.take() {
            handle.stop();
        }

        // 2. 保存设置到本地 JSON
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
        
        // 清除脏标记
        self.is_dirty = false;
        
        // 3. 重置进度条
        if let Ok(mut p) = self.load_progress.lock() { 
            *p = 0.0; 
        }

        // 4. 启动新引擎
        match spawn_audio_thread(self.realtime_config.clone(), self.soundfonts.clone(), self.load_progress.clone()) {
            Ok(handle) => {
                self.audio_handle = Some(handle);
                self.status_message = format!("已启动引擎。监听 UDP 端口 {}", self.realtime_config.udp_port);
            }
            Err(e) => {
                self.status_message = format!("启动失败: {}", e);
                // 失败时直接将进度条拉满，避免界面卡死在加载状态
                if let Ok(mut p) = self.load_progress.lock() { *p = 1.0; }
            }
        }
    }

    fn setup_custom_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        let font_path = "C:\\Windows\\Fonts\\msyh.ttc";
        if let Ok(font_data) = std::fs::read(font_path) {
            fonts.font_data.insert(
                "msyh".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );

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
            _ => {
                println!("普通权限写入失败，准备通过 PowerShell 申请 UAC 提权...");
                let ps_script = format!(
                    "Start-Process reg -ArgumentList 'add \"{}\" /v {} /t REG_SZ /d xxsynth_winmm.dll /f' -Verb RunAs -WindowStyle Hidden",
                    reg_key, MIDI_PORT_NAME
                );
                
                let admin_status = Command::new("powershell")
                    .args(&["-Command", &ps_script])
                    .status();

                match admin_status {
                    Ok(s) if s.success() => println!("提权请求已发送，请在 UAC 弹窗中点击“是”。"),
                    _ => eprintln!("提权请求失败！如果需要使用 MIDI 端口，请手动以管理员运行程序。"),
                }
            }
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.audio_handle.is_some()
    }
}

// 主界面的全局 Layout 逻辑
impl eframe::App for XXSynthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 捕获渲染子线程汇报的错误/完成消息
        if let Ok(mut err) = self.render_error.lock() {
            if let Some(msg) = err.take() {
                self.status_message = msg;
            }
        }

        let is_loading = *self.load_progress.lock().unwrap() < 1.0;
        let is_rendering = self.is_rendering.load(Ordering::SeqCst);
        let is_locked = is_loading || is_rendering;

        // 模态加载进度弹窗
        if is_loading {
            ctx.set_cursor_icon(egui::CursorIcon::Wait);
            egui::Window::new("⏳ 引擎正在加载")
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(15.0);
                    ui.vertical_centered(|ui| {
                        ui.heading("正在启动/重启引擎...");
                        ui.add_space(15.0);
                        let pct = *self.load_progress.lock().unwrap();
                        ui.add(egui::ProgressBar::new(pct)
                            .show_percentage()
                            .animate(true)
                            .desired_width(300.0));
                        ui.add_space(15.0);
                        ui.label("加载大型音色库可能较久");
                    });
                    ui.add_space(15.0);
                });
        } 
        // 模态渲染进度弹窗
        else if is_rendering {
            ctx.set_cursor_icon(egui::CursorIcon::Wait);
            egui::Window::new("🎬 正在离线渲染")
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(15.0);
                    ui.vertical_centered(|ui| {
                        ui.heading("🚀 正在将 MIDI 渲染至音频文件...");
                        ui.add_space(15.0);
                        let pct = *self.render_progress.lock().unwrap();
                        ui.add(egui::ProgressBar::new(pct)
                            .show_percentage()
                            .animate(true)
                            .desired_width(300.0));
                        ui.add_space(15.0);
                        ui.label("请勿关闭程序，渲染时间取决于乐曲复杂度和多线程配置。");
                    });
                    ui.add_space(15.0);
                });
        }

        // 顶部导航栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_enabled_ui(!is_locked, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_tab, Tab::Soundfonts, "🎹 音色库");
                    ui.selectable_value(&mut self.active_tab, Tab::RealtimeSettings, "\u{2699} 实时设置");
                    ui.selectable_value(&mut self.active_tab, Tab::RenderSettings, "🎬 渲染导出");
                });
            });
        });

        // 底部状态栏
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_enabled_ui(!is_locked, |ui| {
                ui.horizontal(|ui| {
                    let status_color = if self.is_running() { 
                        egui::Color32::from_rgba_unmultiplied(0, 200, 0, 255) 
                    } else { 
                        egui::Color32::from_rgba_unmultiplied(200, 0, 0, 255) 
                    };
                    ui.colored_label(status_color, if self.is_running() { "● 正在运行" } else { "● 已停止" });
                    ui.separator();
                    ui.label(&self.status_message);
                });
            });
        });

        // 中央内容区路由
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!is_locked, |ui| {
                match self.active_tab {
                    Tab::Soundfonts => self.ui_soundfonts(ui),
                    Tab::RealtimeSettings => self.ui_realtime(ui),
                    Tab::RenderSettings => self.ui_render(ui),
                }
            });
        });

        if is_locked {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([680.0, 580.0])
            .with_title("XXSynth"),
        ..Default::default()
    };

    eframe::run_native(
        "xxsynth-app",
        options,
        Box::new(|cc| Ok(Box::new(XXSynthApp::new(cc)))),
    )
}