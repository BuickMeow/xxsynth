use once_cell::sync::Lazy;
use std::net::UdpSocket;
use std::sync::Mutex;

// --- 手动定义必要的 Windows API 常量和结构体，彻底摆脱 windows-sys 依赖问题 ---
pub const MODM_GETNUMDEVS: u32 = 1;
pub const MODM_GETDEVCAPS: u32 = 2;
pub const MODM_OPEN: u32 = 3;
pub const MODM_CLOSE: u32 = 4;
pub const MODM_PREPARE: u32 = 5;
pub const MODM_UNPREPARE: u32 = 6;
pub const MODM_DATA: u32 = 7;
pub const MODM_LONGDATA: u32 = 8;

pub const MMSYSERR_NOERROR: u32 = 0;
pub const MMSYSERR_NOTSUPPORTED: u32 = 11;

pub const MOD_MIDIPORT: u16 = 1;

#[repr(C)]
pub struct MIDIOUTCAPSW {
    pub w_mid: u16,
    pub w_pid: u16,
    pub v_driver_version: u32,
    pub sz_pname: [u16; 32],
    pub w_technology: u16,
    pub w_voices: u16,
    pub w_notes: u16,
    pub w_channel_mask: u16,
    pub dw_support: u32,
}
// --------------------------------------------------------------------------

// 全局复用的 UDP Socket，用于将 MIDI 数据极速发送给后台的 EXE 引擎
static SOCKET: Lazy<Mutex<Option<UdpSocket>>> = Lazy::new(|| Mutex::new(None));

// Windows 多媒体驱动生命周期回调
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DriverProc(
    _id: u32,
    _h_driver: usize,
    u_msg: u32,
    _param1: usize,
    _param2: usize,
) -> usize {
    match u_msg {
        0x0001 | 0x0002 | 0x0003 | 0x0004 | 0x0005 | 0x0006 => 1,
        _ => 0,
    }
}

// 核心：处理所有的 MIDI 消息
#[unsafe(no_mangle)]
pub unsafe extern "system" fn modMessage(
    u_device_id: u32, // 宿主请求的设备ID (0~15)
    u_msg: u32,
    _user: usize,
    param1: usize,
    _param2: usize,
) -> u32 {
    match u_msg {
        // 宿主询问支持多少个设备？答：16个
        MODM_GETNUMDEVS => 16,

        // 宿主获取设备信息（名字会显示在 Domino 里）
        MODM_GETDEVCAPS => {
            if let Some(caps) = (param1 as *mut MIDIOUTCAPSW).as_mut() {
                caps.w_mid = 0xFFFF; 
                caps.w_pid = 0xFFFF; 
                caps.v_driver_version = 0x0100; 
                caps.w_technology = MOD_MIDIPORT;
                caps.w_voices = 256;
                caps.w_notes = 256;
                caps.w_channel_mask = 0xFFFF;
                caps.dw_support = 0;

                // 名字例如 "XXSynth Port 1"
                let name = format!("XXSynth Port {}\0", u_device_id + 1);
                for (i, c) in name.encode_utf16().enumerate() {
                    if i < 32 {
                        caps.sz_pname[i] = c;
                    }
                }
            }
            MMSYSERR_NOERROR
        }

        // 宿主准备打开设备
        MODM_OPEN => {
            let mut sock = SOCKET.lock().unwrap();
            if sock.is_none() {
                // 绑定任意本地端口发送
                *sock = UdpSocket::bind("127.0.0.1:0").ok();
            }
            MMSYSERR_NOERROR
        }

        // 宿主发送短 MIDI 消息
        MODM_DATA => {
            if let Some(sock) = SOCKET.lock().unwrap().as_ref() {
                let msg = param1 as u32;
                let status = (msg & 0xFF) as u8;
                let data1 = ((msg >> 8) & 0xFF) as u8;
                let data2 = ((msg >> 16) & 0xFF) as u8;

                // 封包格式：[端口ID, 状态字节, 数据1, 数据2]
                let packet = [u_device_id as u8, status, data1, data2];
                
                // 无阻塞发给 44444 端口 (后台引擎监听端口)
                let _ = sock.send_to(&packet, "127.0.0.1:44444");
            }
            MMSYSERR_NOERROR
        }

        MODM_CLOSE | MODM_PREPARE | MODM_UNPREPARE => MMSYSERR_NOERROR,
        MODM_LONGDATA => MMSYSERR_NOTSUPPORTED, // 长消息(SysEx)暂不处理
        _ => MMSYSERR_NOTSUPPORTED,
    }
}