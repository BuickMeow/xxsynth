use eframe::egui;
use crate::XXSynthApp;
use crate::config::InterpolatorWrapper;

// 将 UI 绘制逻辑独立出来
impl XXSynthApp {
    pub(crate) fn ui_soundfonts(&mut self, ui: &mut egui::Ui) {
        ui.heading("已加载的音色库 (SF2 / SFZ)");
        ui.label("注意: 列表顺序即为加载顺序，上方的音色如果遇到相同的预设 / 乐器会覆盖下方的。");
        ui.separator();

        let mut changed = false;

        ui.horizontal(|ui| {
            if ui.button("➕ 添加音色文件...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Soundfonts", &["sf2", "sfz"])
                    .pick_file() 
                {
                    self.soundfonts.push(path);
                    changed = true;
                }
            }
            if ui.button("\u{1F5D1} 清空列表").clicked() {
                if !self.soundfonts.is_empty() {
                    self.soundfonts.clear();
                    changed = true;
                }
            }
            
            // 保存并应用按钮：文本固定，仅在 is_dirty 时变色，使用默认尺寸以匹配其他按钮
            let btn_text = "🔄 保存并应用";
            let mut btn = egui::Button::new(egui::RichText::new(btn_text));
            if self.is_dirty {
                btn = btn.fill(egui::Color32::from_rgb(255, 127, 127));
            }
            if ui.add(btn).clicked() {
                self.restart_engine();
            }
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
                    
                    if ui.add_enabled(i > 0, egui::Button::new("⬆")).clicked() { move_up = Some(i); }
                    if ui.add_enabled(i < sf_len.saturating_sub(1), egui::Button::new("⬇")).clicked() { move_down = Some(i); }
                    if ui.button("❌").clicked() { to_remove = Some(i); }
                    
                    ui.label(egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong());
                });
                ui.label(egui::RichText::new(path.to_string_lossy()).small().weak());
                ui.separator();
            }
        });

        // 处理队列修改操作并打上脏标记
        if let Some(i) = move_up {
            self.soundfonts.swap(i, i - 1);
            changed = true;
        }
        if let Some(i) = move_down {
            self.soundfonts.swap(i, i + 1);
            changed = true;
        }
        if let Some(i) = to_remove {
            self.soundfonts.remove(i);
            changed = true;
        }

        if changed {
            self.is_dirty = true;
        }
    }

    pub(crate) fn ui_realtime(&mut self, ui: &mut egui::Ui) {
        ui.heading("实时播放参数");
        ui.label("修改参数后点击下方【应用更改】即可重启引擎并保存到本地。");
        ui.separator();

        let is_running = self.is_running();
        let mut cfg_changed = false;

        {
            let cfg = &mut self.realtime_config;

            // 移除了 striped(true) 以去掉灰白条
            egui::Grid::new("realtime_grid").num_columns(2).spacing([40.0, 10.0]).show(ui, |ui| {
                ui.label("UDP 监听端口:");
                cfg_changed |= ui.add(egui::DragValue::new(&mut cfg.udp_port)).changed();
                ui.end_row();

                ui.label("总通道数:");
                cfg_changed |= ui.add(egui::DragValue::new(&mut cfg.total_channels).range(16..=256)).changed();
                ui.end_row();

                ui.label("缓冲区大小 (ms):");
                cfg_changed |= ui.add(egui::Slider::new(&mut cfg.render_window_ms, 1.0..=100.0).text("ms")).changed();
                ui.end_row();

                ui.label("多线程数量:");
                ui.horizontal(|ui| {
                    let max_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(16);
                    cfg_changed |= ui.radio_value(&mut cfg.thread_count, 1, "单线程").changed();
                    cfg_changed |= ui.radio_value(&mut cfg.thread_count, 0, "自动").changed();
                    
                    let is_custom = cfg.thread_count > 1;
                    let mut custom_clicked = is_custom;
                    if ui.radio(custom_clicked, "自定义:").clicked() {
                        if !is_custom { cfg.thread_count = max_threads / 2; }
                        custom_clicked = true;
                        cfg_changed = true;
                    }
                    if custom_clicked {
                        cfg_changed |= ui.add(egui::DragValue::new(&mut cfg.thread_count).range(2..=max_threads)).changed();
                    }
                });
                ui.end_row();

                ui.label("插值算法:");
                cfg_changed |= egui::ComboBox::from_id_salt("interp_combo")
                    .selected_text(cfg.interpolator.to_string())
                    .show_ui(ui, |ui| {
                        let mut c = false;
                        c |= ui.selectable_value(&mut cfg.interpolator, InterpolatorWrapper::Nearest, "最近邻 (Nearest) - 极低CPU占用").changed();
                        c |= ui.selectable_value(&mut cfg.interpolator, InterpolatorWrapper::Linear, "线性 (Linear) - 音质平滑").changed();
                        c
                    }).inner.unwrap_or(false);
                ui.end_row();

                ui.label("忽略力度范围:");
                ui.horizontal(|ui| {
                    cfg_changed |= ui.add(egui::DragValue::new(&mut cfg.ignore_velocity_min).range(0..=127)).changed();
                    ui.label("至");
                    cfg_changed |= ui.add(egui::DragValue::new(&mut cfg.ignore_velocity_max).range(0..=127)).changed();
                });
                if cfg.ignore_velocity_min > cfg.ignore_velocity_max {
                    cfg.ignore_velocity_max = cfg.ignore_velocity_min;
                }
                ui.end_row();
            });
        }

        if cfg_changed {
            self.is_dirty = true;
        }

        ui.add_space(20.0);
        
        ui.horizontal(|ui| {
            // 带有小红点/变色提示的重启按钮
            let btn_text = "🔄 应用更改并重启";
            let mut btn = egui::Button::new(egui::RichText::new(btn_text).heading());
            if self.is_dirty {
                btn = btn.fill(egui::Color32::from_rgb(255, 127, 127));
            }

            if ui.add_sized([200.0, 40.0], btn).clicked() {
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
        ui.label("设置渲染参数并调用底层的 xsynth-render 来完成急速渲染。");
        ui.separator();

        let cfg = &mut self.render_config;

        ui.horizontal(|ui| {
            ui.label("输入 MIDI:");
            if ui.button("📂 选择文件").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("MIDI", &["mid", "midi"]).pick_file() {
                    cfg.midi_path = path.to_string_lossy().to_string();
                }
            }
            ui.label(&cfg.midi_path);
        });

        ui.horizontal(|ui| {
            ui.label("输出 WAV:");
            if ui.button("💾 保存位置").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).set_file_name("out.wav").save_file() {
                    cfg.output_path = path.to_string_lossy().to_string();
                }
            }
            ui.label(&cfg.output_path);
        });

        ui.add_space(15.0);

        egui::Grid::new("render_grid").num_columns(2).spacing([40.0, 10.0]).show(ui, |ui| {
            ui.label("采样率:");
            ui.add(egui::DragValue::new(&mut cfg.sample_rate));
            ui.end_row();

            ui.label("音频通道:");
            egui::ComboBox::from_id_salt("channels").selected_text(&cfg.audio_channels).show_ui(ui, |ui| {
                ui.selectable_value(&mut cfg.audio_channels, "stereo".to_string(), "立体声 (stereo)");
                ui.selectable_value(&mut cfg.audio_channels, "mono".to_string(), "单声道 (mono)");
            });
            ui.end_row();

            ui.label("通道图层限制 (0为无限制):");
            ui.add(egui::DragValue::new(&mut cfg.layers));
            ui.end_row();

            ui.label("插值算法:");
            egui::ComboBox::from_id_salt("render_interp").selected_text(&cfg.interpolation).show_ui(ui, |ui| {
                ui.selectable_value(&mut cfg.interpolation, "linear".to_string(), "线性 (linear)");
                ui.selectable_value(&mut cfg.interpolation, "none".to_string(), "最近邻 (none)");
            });
            ui.end_row();

            ui.label("通道多线程:");
            ui.text_edit_singleline(&mut cfg.channel_threading).on_hover_text("填 none, auto, 或正整数");
            ui.end_row();

            ui.label("按键多线程:");
            ui.text_edit_singleline(&mut cfg.key_threading).on_hover_text("填 none, auto, 或正整数");
            ui.end_row();
            
            ui.label("其他处理:");
            ui.horizontal(|ui| {
                ui.checkbox(&mut cfg.apply_limiter, "开启限制器 (-L)");
                ui.checkbox(&mut cfg.disable_fade_out, "禁用声音淡出");
                ui.checkbox(&mut cfg.linear_envelope, "使用线性包络");
            });
            ui.end_row();
        });

        ui.add_space(20.0);

        if ui.add_sized([200.0, 40.0], egui::Button::new(egui::RichText::new("🚀 开始渲染").heading())).clicked() {
            if self.soundfonts.is_empty() {
                self.status_message = "错误：渲染需要至少加载一个音色库！".to_string();
                return;
            }
            if self.render_config.midi_path.is_empty() {
                self.status_message = "错误：请先选择输入的 MIDI 文件！".to_string();
                return;
            }

            self.is_rendering.store(true, std::sync::atomic::Ordering::SeqCst);
            *self.render_progress.lock().unwrap() = 0.0;
            self.status_message = "正在渲染...".to_string();

            // 克隆参数丢进渲染子线程
            let midi = self.render_config.midi_path.clone();
            let out = self.render_config.output_path.clone();
            let sfs = self.soundfonts.clone();
            let sample_rate = self.render_config.sample_rate;
            let audio_channels = self.render_config.audio_channels.clone();
            let layers = self.render_config.layers;
            let channel_threading = self.render_config.channel_threading.clone();
            let key_threading = self.render_config.key_threading.clone();
            let apply_limiter = self.render_config.apply_limiter;
            let disable_fade_out = self.render_config.disable_fade_out;
            let linear_envelope = self.render_config.linear_envelope;
            let interpolation = self.render_config.interpolation.clone();

            let is_rendering_clone = self.is_rendering.clone();
            let progress_clone = self.render_progress.clone();
            let error_clone = self.render_error.clone();

            std::thread::spawn(move || {
                use std::process::{Command, Stdio};
                use std::io::Read;

                let mut cmd = Command::new("xsynth-render"); // 会自动查找 PATH 或同级目录下的 xsynth-render(.exe)
                
                cmd.arg(&midi);
                for sf in &sfs { cmd.arg(sf); }
                cmd.arg("-o").arg(&out);
                cmd.arg("-s").arg(sample_rate.to_string());
                cmd.arg("-c").arg(&audio_channels);
                cmd.arg("-l").arg(layers.to_string());
                cmd.arg("--channel-threading").arg(&channel_threading);
                cmd.arg("--key-threading").arg(&key_threading);
                if apply_limiter { cmd.arg("-L"); }
                if disable_fade_out { cmd.arg("--disable-fade-out"); }
                if linear_envelope { cmd.arg("--linear-envelope"); }
                cmd.arg("-I").arg(&interpolation);

                // 在 Windows 环境下隐藏 xsynth-render 拉起时可能带来的黑框
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                }

                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                if let Ok(mut child) = cmd.spawn() {
                    // xsynth-render 通常将进度日志用 indicatif 库输出在 stderr 中
                    if let Some(stderr) = child.stderr.take() {
                        let mut byte_reader = stderr.bytes();
                        let mut buffer = String::new();
                        
                        // 逐字节读取 stderr 并在遇到 \r 或 \n 时解析进度
                        while let Some(Ok(b)) = byte_reader.next() {
                            if b == b'\r' || b == b'\n' {
                                if let Some(idx) = buffer.find("%") {
                                    // 往前寻找数字来匹配百分比值
                                    let mut start_idx = idx;
                                    while start_idx > 0 && buffer.as_bytes()[start_idx - 1].is_ascii_digit() {
                                        start_idx -= 1;
                                    }
                                    if let Ok(pct) = buffer[start_idx..idx].parse::<f32>() {
                                        if let Ok(mut p) = progress_clone.lock() {
                                            *p = pct / 100.0;
                                        }
                                    }
                                }
                                buffer.clear();
                            } else {
                                buffer.push(b as char);
                            }
                        }
                    }
                    
                    let status = child.wait();
                    if status.is_err() || !status.unwrap().success() {
                         if let Ok(mut err) = error_clone.lock() {
                            *err = Some("错误：渲染进程异常退出！请检查 xsynth-render 工具。".to_string());
                         }
                    } else {
                         if let Ok(mut err) = error_clone.lock() {
                            *err = Some(format!("渲染完成！音频已保存至 {}", out));
                         }
                    }
                } else {
                    if let Ok(mut err) = error_clone.lock() {
                        *err = Some("错误：找不到 xsynth-render！请确保它放置在同级目录或已添加到系统 PATH 中。".to_string());
                    }
                }
                
                // 渲染流程结束，解除模态锁
                is_rendering_clone.store(false, std::sync::atomic::Ordering::SeqCst);
            });
        }
    }
}