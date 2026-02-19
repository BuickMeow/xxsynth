mod config;

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use xsynth_core::channel_group::SynthFormat;
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
    
    println!("正在初始化音频引擎 (256 通道) ...");
    let synth = RealtimeSynth::open_with_default_output(synth_cfg);
let synth_arc = Arc::new(Mutex::new(synth));

    // 测试阶段，请在这里强行加载一个 sf2 方便测试发声
    /*{
        let mut s = synth_arc.lock().unwrap();
        // 调用 xsynth 的 api 加载音色库
        s.load_soundfont("D:\\Soundfonts\\Choomaypiano.sf2"); 
    }*/

    // 2. 启动 UDP 监听 (对应 DLL 发送的 44444)
    let socket = UdpSocket::bind("127.0.0.1:44444")?;
    // 设置一点超时避免死锁，虽然 UDP 接收很快
    socket.set_read_timeout(Some(Duration::from_millis(10)))?;
    
    println!("引擎就绪！监听 127.0.0.1:44444 中。请在 Domino 中播放...");

    let mut buf = [0u8; 4];

    // 3. 高频接收来自 DLL 的数据
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
                    
                    // 核心：计算目标 256 通道 ID
                    let target_channel = (port_index as u32 * 16) + original_channel as u32;

                    if let Ok(mut _s) = synth_arc.lock() {
                        // 伪代码: 构造并发送 XSynth 接受的事件
                        // Event API 取决于你拉取的 xsynth_core 的具体实现
                        // let event = Event::Midi(target_channel, [status_byte, data1, data2]);
                        // _s.send_event(event);
                        
                        // 由于未接入真正的 XSynth send_event，先打印日志查看映射是否正确：
                        // 如果 Domino 非常密集地发声，建议注释掉打印，以免控制台卡顿
                        println!("收到 DLL 数据 -> Port {} 映射至内部 Channel {}: [0x{:02X}, {}, {}]", 
                           port_index + 1, target_channel, status_byte, data1, data2);
                    }
                }
            }
        }
    }
}