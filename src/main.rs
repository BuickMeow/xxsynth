use iced::{Element, Length, Task, Theme, Subscription};
use iced::widget::{button, column, row, text, container, text_input, scrollable, Space, Column};

pub fn main() -> iced::Result {
    env_logger::init();

    // 【修复1】 回归使用函数指针 App::update 和 App::view
    // 这样通常能让编译器正确处理高阶生命周期 (HRTB)，避免闭包带来的 lifetime 问题
    iced::application(App::new, App::update, App::view)
        .title(|_state: &App| "XSynth GUI Configuration".to_string())
        .theme(|_: &App| Theme::Dark)
        .centered()
        .subscription(|s: &App| s.subscription())
        .run()
}

// --- 1. 状态定义 (State) ---
struct App {
    soundfont_path: String,
    layer_limit: String, 
    gain_level: String,
    
    is_running: bool,
    voice_count: u64,
    tick_counter: u64, // 用于模拟动画
}

impl Default for App {
    fn default() -> Self {
        Self {
            soundfont_path: "".to_string(),
            layer_limit: "100".to_string(),
            gain_level: "1.0".to_string(),
            // 启动软件时直接启动引擎
            is_running: true,
            voice_count: 0,
            tick_counter: 0,
        }
    }
}

impl App {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }
}

// --- 2. 消息定义 (Message) ---
#[derive(Debug, Clone)]
enum Message {
    PickSoundFont,
    SoundFontSelected(Option<String>),
    LayerLimitChanged(String),
    GainChanged(String),
    Tick, 
}

// --- 3. 逻辑处理 (Update) ---
impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PickSoundFont => {
                // 【修复2】 直接调用 pick_file，不再有 cfg 限制
                Task::perform(pick_file(), Message::SoundFontSelected)
            }
            Message::SoundFontSelected(path) => {
                if let Some(p) = path {
                    self.soundfont_path = p;
                }
                Task::none()
            }
            Message::LayerLimitChanged(val) => {
                self.layer_limit = val;
                Task::none()
            }
            Message::GainChanged(val) => {
                self.gain_level = val;
                Task::none()
            }
            // 移除手动切换引擎的逻辑，软件启动即运行
            Message::Tick => {
                // 简单的模拟逻辑，避免引入 rand 依赖导致报错
                if self.is_running {
                    self.tick_counter = self.tick_counter.wrapping_add(1);
                    self.voice_count = 100 + (self.tick_counter % 50);
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    // --- 4. 订阅逻辑 (Subscription) ---
    fn subscription(&self) -> Subscription<Message> {
        if self.is_running {
            iced::time::every(std::time::Duration::from_millis(100))
                .map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    // --- 5. 界面布局 (View) ---
    fn view(&self) -> Element<'_, Message> {
        // 文件选择区
        let file_section = row![
            button("📂 加载音色库 (SF2/SFZ)").on_press(Message::PickSoundFont),
            text(if self.soundfont_path.is_empty() { "未选择文件" } else { &self.soundfont_path }).size(14)
        ].spacing(10).align_y(iced::Alignment::Center);

        // 设置区
        let settings_section = row![
            input_group("最大层数 (Layers)", &self.layer_limit, Message::LayerLimitChanged),
            input_group("全局增益 (Gain)", &self.gain_level, Message::GainChanged),
        ].spacing(20);

        // 状态栏
        let status_bar = row![
            text(if self.is_running { "🟢 运行中" } else { "🔴 已停止" }),
            // 【修复3】 Space::new() 不接受参数，改为链式调用 .width()
            Space::new().width(Length::Fill),
            text(format!("当前复音数: {}", self.voice_count)).color([0.0, 1.0, 0.0])
        ].width(Length::Fill).align_y(iced::Alignment::Center);

        container(
            column![
                text("XSynth 控制台").size(24),
                file_section,
                text("引擎参数").size(16).color(iced::Color::from_rgb(0.4, 0.6, 1.0)),
                settings_section,
                status_bar,
            ]
            .spacing(20)
            .padding(20)
            .max_width(800)
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}

// --- 辅助函数 ---

fn input_group<'a>(label: &'a str, value: &'a str, msg: fn(String) -> Message) -> Column<'a, Message> {
    column![
        text(label).size(14).color([0.7, 0.7, 0.7]),
        text_input("...", value).on_input(msg).padding(5).width(150)
    ]
    .spacing(5)
}

// 【修复4】 启用真实文件选择，移除 cfg
async fn pick_file() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .add_filter("SoundFont", &["sf2", "sfz"])
        .pick_file()
        .await
        .map(|f| f.path().to_string_lossy().to_string())
}