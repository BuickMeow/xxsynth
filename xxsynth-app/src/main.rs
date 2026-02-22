mod config;

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::process::Command;

use xsynth_core::channel_group::{SynthEvent, SynthFormat, ThreadCount};
use xsynth_core::channel::{ChannelEvent, ChannelConfigEvent, ChannelAudioEvent, ChannelInitOptions};
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions, Interpolator};
use xsynth_core::{AudioStreamParams, ChannelCount};
use xsynth_realtime::{RealtimeSynth, XSynthRealtimeConfig};

// ==========================================
// 全局配置区
// 后续如果想分离，直接把这些常量移到 config.rs 即可
// ==========================================

/// UDP 监听端口
const UDP_PORT: u16 = 44444;

/// MIDI 虚拟端口名称 (用于写入注册表)
const MIDI_PORT_NAME: &str = "midi7";

/// 需要创建的合成器通道总数
/// 如果使用黑 MIDI，每个 Port 16 个通道，如果有 4 个 Port 就是 64 个通道。
const TOTAL_CHANNELS: u32 = 64; 

/// 默认加载的 SF2 音色库路径
//const DEFAULT_SOUNDFONT_PATH: &str = "D:\\Soundfonts\\Choomaypiano.sf2";
const DEFAULT_SOUNDFONT_PATH: &str = "D:\\Soundfonts\\Starry Studio Grand v2.7~\\Presets\\A_Standard\\Studio Grand - Standard (No Hammer).sfz";

/// 是否开启多线程渲染 (ThreadCount::Auto 或 ThreadCount::None)
/// 注意：Debug 模式下建议设为 None 防止爆音，Release 模式建议设为 Auto 提升性能。
/// 节能酱认为，这个Auto不太好用，不如自己设定线程数，也有可能是我不会设
const MULTITHREADING: ThreadCount = ThreadCount::Manual(12);

/// 渲染窗口缓冲大小 (毫秒)
const RENDER_WINDOW_MS: f64 = 15.0;

/// 音色库插值器算法 (Interpolator::Linear 或 Interpolator::Nearest 等)
/// Linear（线性插值）能显著降低 CPU 占用，适合高负载或黑 MIDI 场景。
const SF_INTERPOLATOR: Interpolator = Interpolator::Nearest;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // --- 新增：自动写入注册表 ---
    println!("正在尝试将虚拟 MIDI 端口 [{}] 写入注册表...", MIDI_PORT_NAME);
    let reg_key = "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Drivers32";
    let status = Command::new("reg")
        .args(&["add", reg_key, "/v", MIDI_PORT_NAME, "/t", "REG_SZ", "/d", "xxsynth_winmm.dll", "/f"])
        .status();

    match status {
        Ok(s) if s.success() => println!("注册表写入成功！(端口: {})", MIDI_PORT_NAME),
        _ => eprintln!("注册表写入失败！请确保你以【管理员身份】运行此程序。"),
    }
    // -----------------------------

    println!("=== XXSynth 引擎已启动 ===");
    println!("监听端口: {}", UDP_PORT);
    println!("目标通道数: {}", TOTAL_CHANNELS);

    // 1. 初始化 XSynth 实时配置
    let mut synth_cfg = XSynthRealtimeConfig::default();
    
    synth_cfg.render_window_ms = RENDER_WINDOW_MS; 
    synth_cfg.multithreading = MULTITHREADING;
    
    // 自定义通道数
    let format = SynthFormat::Custom { channels: TOTAL_CHANNELS };
    synth_cfg.format = format; 
    
    // 【配置：ignore_range 忽略范围】
    // 这里指的是忽略力度范围，当前设定为忽略 0 到 1 力度的音符
    // 如果不需要可以注释掉这行，默认是 0..=0（也就是不忽略任何正常音符）。
    synth_cfg.ignore_range = 0..=1;

    println!("正在初始化音频引擎...");
    let mut synth = RealtimeSynth::open_with_default_output(synth_cfg);

    // 2. 加载并分配音色库
    println!("正在加载和解析音色库: {}", DEFAULT_SOUNDFONT_PATH);
    
    let audio_params = AudioStreamParams::new(48000, ChannelCount::Stereo); 
    
    let mut sf_options = SoundfontInitOptions::default();
    // 应用顶部配置的插值器算法
    sf_options.interpolator = SF_INTERPOLATOR;
    
    let soundfont = Arc::new(
        SampleSoundfont::new(DEFAULT_SOUNDFONT_PATH, audio_params, sf_options)
            .expect("无法加载 SF2 / SFZ 文件，请检查文件路径是否正确")
    );

    println!("正在为 {} 个通道分配音色...", TOTAL_CHANNELS);
    for ch in 0..TOTAL_CHANNELS {
        let sf_base: Arc<dyn SoundfontBase> = soundfont.clone();
        
        let event = SynthEvent::Channel(
            ch,
            ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(vec![sf_base]))
        );
        synth.send_event(event);
    }

    let synth_arc = Arc::new(Mutex::new(synth));

    // 3. 启动 UDP 监听
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", UDP_PORT))?;
    socket.set_read_timeout(Some(Duration::from_millis(10)))?;
    
    println!("引擎就绪！请在 Domino 中播放...");

    let mut buf = [0u8; 4];

    loop {
        if let Ok((size, _)) = socket.recv_from(&mut buf) {
            if size == 4 {
                let port_index = buf[0];
                let status_byte = buf[1];
                let data1 = buf[2];
                let data2 = buf[3];

                if status_byte >= 0x80 && status_byte < 0xF0 {
                    let original_channel = status_byte & 0x0F;
                    let target_channel = (port_index as u32 * 16) + original_channel as u32;
                    
                    // 防御性编程：避免接收到的 Target Channel 超出了我们初始化的总通道数
                    if target_channel >= TOTAL_CHANNELS {
                        continue;
                    }

                    if let Ok(mut s) = synth_arc.lock() {
                        let channel_event = match status_byte & 0xF0 {
                            0x90 if data2 > 0 => {
                                Some(ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                                    key: data1,
                                    vel: data2,
                                }))
                            },
                            0x80 | 0x90 => {
                                Some(ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                                    key: data1,
                                }))
                            },
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
}