use eframe::egui;
use crate::XXSynthApp;
use crate::config::InterpolatorWrapper;

// 将 UI 绘制逻辑独立出来
impl XXSynthApp {
    pub(crate) fn ui_soundfonts(&mut self, ui: &mut egui::Ui) {
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
                if !self.soundfonts.is_empty() {
                    self.soundfonts.clear();
                }
            }
            if ui.button("🔄 保存并应用").clicked() {
                self.restart_engine();
            }
            /*ui.add_space(20.0);
            
            // 明确的应用更改按钮
            if ui.add_sized([180.0, 30.0], egui::Button::new(egui::RichText::new("🚀 确认修改并重启引擎").strong())).clicked() {
                self.restart_engine();
            }*/
        });

        ui.add_space(10.0);

        let mut to_remove = None;
        let mut move_up = None;
        let mut move_down = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let sf_len = self.soundfonts.len();
            for (i, path) in self.soundfonts.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", i + 1));
                    
                    // 上移按钮 (第一项禁用)
                    if ui.add_enabled(i > 0, egui::Button::new("⬆")).clicked() {
                        move_up = Some(i);
                    }
                    // 下移按钮 (最后一项禁用)
                    if ui.add_enabled(i < sf_len.saturating_sub(1), egui::Button::new("⬇")).clicked() {
                        move_down = Some(i);
                    }
                    // 删除按钮
                    if ui.button("❌").clicked() {
                        to_remove = Some(i);
                    }
                    
                    ui.label(egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong());
                });
                ui.label(egui::RichText::new(path.to_string_lossy()).small().weak());
                ui.separator();
            }
        });

        // 处理队列修改操作
        if let Some(i) = move_up {
            self.soundfonts.swap(i, i - 1);
        }
        if let Some(i) = move_down {
            self.soundfonts.swap(i, i + 1);
        }
        if let Some(i) = to_remove {
            self.soundfonts.remove(i);
        }
    }

    pub(crate) fn ui_realtime(&mut self, ui: &mut egui::Ui) {
        ui.heading("实时播放参数");
        ui.label("修改参数后点击下方【应用更改】即可重启引擎并保存到本地。");
        ui.separator();

        let is_running = self.is_running();

        {
            let cfg = &mut self.realtime_config;

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
        }

        ui.add_space(20.0);
        
        ui.horizontal(|ui| {
            // 应用更改按钮触发重启
            if ui.add_sized([200.0, 40.0], egui::Button::new(egui::RichText::new("🔄 应用更改并重启").heading())).clicked() {
                self.restart_engine();
            }

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

    pub(crate) fn ui_render(&mut self, ui: &mut egui::Ui) {
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