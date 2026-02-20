mod config;

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use xsynth_core::channel_group::{SynthEvent, SynthFormat};
// 移除了多余的 KeyNoteEvent，直接使用 ChannelAudioEvent
use xsynth_core::channel::{ChannelEvent, ChannelConfigEvent, ChannelAudioEvent};
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions};
use xsynth_core::{AudioStreamParams, ChannelCount};
use xsynth_realtime::{RealtimeSynth, XSynthRealtimeConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    println!("=== XXSynth 引擎已启动 ===");
    println!("模式: Headless (UDP IPC 监听中...)");

    // 1. 初始化 XSynth，配置为 256 通道
    let mut synth_cfg = XSynthRealtimeConfig::default();
    synth_cfg.render_window_ms = 10.0;
    
    // 自定义 256 通道
    let format = SynthFormat::Custom { channels: 256 };
    synth_cfg.format = format; 
    
    println!("正在初始化音频引擎 (256 通道) ...");
    let mut synth = RealtimeSynth::open_with_default_output(synth_cfg);

    // 2. 加载并分配音色库
    println!("正在加载和解析 SF2 音色库...");
    
    let audio_params = AudioStreamParams::new(44100, ChannelCount::Stereo); 
    let sf_options = SoundfontInitOptions::default();
    
    let soundfont = Arc::new(
        SampleSoundfont::new("D:\\Soundfonts\\Choomaypiano.sf2", audio_params, sf_options)
            .expect("无法加载 SF2 文件，请检查文件路径是否正确")
    );

    println!("正在为 256 个通道分配音色...");
    for ch in 0..256 {
        let sf_base: Arc<dyn SoundfontBase> = soundfont.clone();
        
        // 挂载音色库属于 Config 事件
        let event = SynthEvent::Channel(
            ch,
            ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(vec![sf_base]))
        );
        synth.send_event(event);
    }

    // 将初始化完毕的合成器放入 Arc<Mutex>，供接收线程使用
    let synth_arc = Arc::new(Mutex::new(synth));

    // 3. 启动 UDP 监听 (对应 DLL 发送的 44444)
    let socket = UdpSocket::bind("127.0.0.1:44444")?;
    socket.set_read_timeout(Some(Duration::from_millis(10)))?;
    
    println!("引擎就绪！监听 127.0.0.1:44444 中。请在 Domino 中播放...");

    let mut buf = [0u8; 4];

    // 4. 高频接收来自 DLL 的数据并发送至合成器
    loop {
        if let Ok((size, _)) = socket.recv_from(&mut buf) {
            if size == 4 {
                let port_index = buf[0];
                let status_byte = buf[1];
                let data1 = buf[2];
                let data2 = buf[3];

                // 判断是否是 Channel 消息 (0x80 到 0xEF)
                if status_byte >= 0x80 && status_byte < 0xF0 {
                    let original_channel = status_byte & 0x0F;
                    
                    let target_channel = (port_index as u32 * 16) + original_channel as u32;

                    if let Ok(mut s) = synth_arc.lock() {
                        // 构建实时的 Audio 事件：根据你提供的源码结构，直接使用 NoteOn 和 NoteOff
                        let channel_event = match status_byte & 0xF0 {
                            0x90 if data2 > 0 => {
                                // Note On: 修复 E0308，直接使用原生的 u8 类型
                                Some(ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                                    key: data1,
                                    vel: data2,
                                }))
                            },
                            0x80 | 0x90 => {
                                // Note Off: 修复 E0308，直接使用原生的 u8 类型
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