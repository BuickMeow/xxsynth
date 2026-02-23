use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use xsynth_core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent};
use xsynth_core::channel_group::{SynthEvent, SynthFormat};
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions};
use xsynth_core::{AudioStreamParams, ChannelCount};
use xsynth_realtime::{RealtimeSynth, XSynthRealtimeConfig};

use crate::config::RealtimeConfig;

pub struct AudioEngineHandle {
    pub is_running: Arc<AtomicBool>,
    pub thread_handle: Option<thread::JoinHandle<()>>,
}

impl AudioEngineHandle {
    pub fn stop(&mut self) {
        if self.is_running.load(Ordering::Relaxed) {
            println!("正在停止音频引擎...");
            self.is_running.store(false, Ordering::Relaxed);
            if let Some(handle) = self.thread_handle.take() {
                let _ = handle.join(); // 等待线程安全退出
            }
            println!("音频引擎已停止。");
        }
    }
}

pub fn spawn_audio_thread(
    config: RealtimeConfig,
    soundfonts: Vec<PathBuf>,
    load_progress: Arc<Mutex<f32>>, // 用于向 UI 上报加载进度
) -> Result<AudioEngineHandle, String> {
    let is_running = Arc::new(AtomicBool::new(true));
    let is_running_clone = is_running.clone();

    // 尝试提前绑定 UDP 端口，如果被占用直接报错
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", config.udp_port))
        .map_err(|e| format!("无法绑定 UDP 端口 {}: {}", config.udp_port, e))?;
    // 设置超时，让 recv_from 不会永久阻塞，从而能响应停止信号
    socket.set_read_timeout(Some(Duration::from_millis(10))).unwrap();

    let thread_handle = thread::spawn(move || {
        println!("=== 后台音频线程已启动 ===");

        // 初始化环境与参数，给予 5% 的基础进度
        if let Ok(mut p) = load_progress.lock() { *p = 0.05; }

        // 1. 初始化 XSynth 配置
        let mut synth_cfg = XSynthRealtimeConfig::default();
        synth_cfg.render_window_ms = config.render_window_ms;
        synth_cfg.multithreading = config.get_thread_count();
        synth_cfg.format = SynthFormat::Custom { channels: config.total_channels };
        
        synth_cfg.ignore_range = config.ignore_velocity_min..=config.ignore_velocity_max;

        let mut synth = RealtimeSynth::open_with_default_output(synth_cfg);

        // 2. 加载音色库
        let audio_params = AudioStreamParams::new(48000, ChannelCount::Stereo);
        let mut sf_options = SoundfontInitOptions::default();
        sf_options.interpolator = config.get_interpolator();

        let mut loaded_sfs: Vec<Arc<dyn SoundfontBase>> = Vec::new();

        // 动态分配剩下的 90% 进度用于音色加载阶段
        let total_sfs = soundfonts.len();
        if total_sfs > 0 {
            for (i, sf_path) in soundfonts.into_iter().enumerate() {
                println!("正在加载音色库: {}", sf_path.display());
                match SampleSoundfont::new(&sf_path, audio_params, sf_options.clone()) {
                    Ok(sf) => loaded_sfs.push(Arc::new(sf)),
                    Err(e) => eprintln!("加载音色库失败 {}: {:?}", sf_path.display(), e),
                }
                
                // 每加载完一个更新一次进度
                if let Ok(mut p) = load_progress.lock() { 
                    *p = 0.05 + (0.90 * ((i + 1) as f32 / total_sfs as f32)); 
                }
            }
        } else {
            // 没有音色库的话跳过该阶段，直接拉到 95%
            if let Ok(mut p) = load_progress.lock() { *p = 0.95; }
        }

        if !loaded_sfs.is_empty() {
            println!("正在为 {} 个通道分配音色...", config.total_channels);
            for ch in 0..config.total_channels {
                let event = SynthEvent::Channel(
                    ch,
                    ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(loaded_sfs.clone())),
                );
                synth.send_event(event);
            }
        } else {
            println!("警告：未加载任何有效音色库，将没有声音！");
        }

        let synth_arc = Arc::new(Mutex::new(synth));
        println!("引擎就绪！正在监听 UDP 端口 {}...", config.udp_port);

        // 彻底就绪，进度条 100%
        if let Ok(mut p) = load_progress.lock() { *p = 1.0; }

        let mut buf = [0u8; 4];

        // 3. UDP 监听循环
        while is_running_clone.load(Ordering::Relaxed) {
            if let Ok((size, _)) = socket.recv_from(&mut buf) {
                if size == 4 {
                    let port_index = buf[0];
                    let status_byte = buf[1];
                    let data1 = buf[2];
                    let data2 = buf[3];

                    if status_byte >= 0x80 && status_byte < 0xF0 {
                        let original_channel = status_byte & 0x0F;
                        let target_channel = (port_index as u32 * 16) + original_channel as u32;

                        if target_channel >= config.total_channels {
                            continue;
                        }

                        if let Ok(mut s) = synth_arc.lock() {
                            let channel_event = match status_byte & 0xF0 {
                                0x90 if data2 > 0 => {
                                    Some(ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                                        key: data1,
                                        vel: data2,
                                    }))
                                }
                                0x80 | 0x90 => {
                                    Some(ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                                        key: data1,
                                    }))
                                }
                                _ => None,
                            };

                            if let Some(ce) = channel_event {
                                let event = SynthEvent::Channel(target_channel, ce);
                                s.send_event(event);
                            }
                        }
                    }
                }
            }
        }

        println!("=== 后台音频线程正在退出 ===");
    });

    Ok(AudioEngineHandle {
        is_running,
        thread_handle: Some(thread_handle),
    })
}