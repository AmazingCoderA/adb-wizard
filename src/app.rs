use crate::adb::{self, Monitor, ScreenImage, Snapshot};
use crate::i18n::{tr, Lang};
use crate::parsers::{human_bytes, human_kb, human_uptime, Device, DeviceInfo, ProcStat};
use crate::publish::{self, PublishEvent, Tools};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, RichText, ScrollArea, Stroke, TextEdit,
    TextureHandle, TextureOptions, Vec2,
};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Processes,
    Apps,
    Screen,
    Console,
    Info,
    GitHub,
}

enum TaskMsg {
    Console {
        title: String,
        output: String,
        success: bool,
    },
    Info(Result<DeviceInfo, String>),
    Snapshot(Snapshot),
    Screenshot(Result<ScreenImage, String>),
    Publish(PublishEvent),
    Packages(Result<Vec<String>, String>),
    PackageInfo(Result<String, String>),
    TechInfo(Result<String, String>),
    Ping(Result<(f32, f32), String>),
}

#[derive(Clone)]
struct CustomButton {
    label: String,
    command: String,
}

fn default_buttons() -> Vec<CustomButton> {
    [
        ("Play/Pause", "input keyevent 85"),
        ("Menu", "input keyevent 82"),
        ("Camera", "input keyevent 27"),
        ("Vol +", "input keyevent 24"),
        ("Vol −", "input keyevent 25"),
    ]
    .iter()
    .map(|(label, command)| CustomButton {
        label: label.to_string(),
        command: command.to_string(),
    })
    .collect()
}

struct ConsoleEntry {
    time: String,
    title: String,
    output: String,
    success: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeKind {
    Graphite,
    Ocean,
    Emerald,
    Violet,
    Light,
    Sand,
    Mint,
    Rose,
}

impl ThemeKind {
    const ALL: [ThemeKind; 8] = [
        ThemeKind::Graphite,
        ThemeKind::Ocean,
        ThemeKind::Emerald,
        ThemeKind::Violet,
        ThemeKind::Light,
        ThemeKind::Sand,
        ThemeKind::Mint,
        ThemeKind::Rose,
    ];

    fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (ThemeKind::Graphite, Lang::Ru) => "Графит",
            (ThemeKind::Graphite, Lang::En) => "Graphite",
            (ThemeKind::Ocean, Lang::Ru) => "Океан",
            (ThemeKind::Ocean, Lang::En) => "Ocean",
            (ThemeKind::Emerald, Lang::Ru) => "Изумруд",
            (ThemeKind::Emerald, Lang::En) => "Emerald",
            (ThemeKind::Violet, Lang::Ru) => "Фиалка",
            (ThemeKind::Violet, Lang::En) => "Violet",
            (ThemeKind::Light, Lang::Ru) => "Светлая",
            (ThemeKind::Light, Lang::En) => "Light",
            (ThemeKind::Sand, Lang::Ru) => "Песок",
            (ThemeKind::Sand, Lang::En) => "Sand",
            (ThemeKind::Mint, Lang::Ru) => "Мята",
            (ThemeKind::Mint, Lang::En) => "Mint",
            (ThemeKind::Rose, Lang::Ru) => "Роза",
            (ThemeKind::Rose, Lang::En) => "Rose",
        }
    }

    fn palette(self) -> Palette {
        match self {
            ThemeKind::Graphite => Palette {
                panel: Color32::from_rgb(18, 20, 24),
                window: Color32::from_rgb(24, 27, 32),
                extreme: Color32::from_rgb(12, 14, 17),
                faint: Color32::from_rgb(26, 29, 35),
                widget: Color32::from_rgb(33, 37, 44),
                widget_hover: Color32::from_rgb(43, 48, 57),
                stroke: Color32::from_rgb(48, 53, 62),
                text: Color32::from_rgb(230, 233, 238),
                muted: Color32::from_rgb(150, 158, 170),
                accent: Color32::from_rgb(255, 152, 0),
                accent2: Color32::from_rgb(66, 165, 245),
                on_accent: Color32::from_rgb(26, 20, 8),
                good: Color32::from_rgb(102, 187, 106),
                warn: Color32::from_rgb(255, 202, 40),
                bad: Color32::from_rgb(239, 83, 80),
                warn_bg: Color32::from_rgb(48, 40, 22),
                is_light: false,
            },
            ThemeKind::Ocean => Palette {
                panel: Color32::from_rgb(14, 20, 30),
                window: Color32::from_rgb(18, 26, 38),
                extreme: Color32::from_rgb(9, 14, 22),
                faint: Color32::from_rgb(20, 28, 41),
                widget: Color32::from_rgb(24, 35, 50),
                widget_hover: Color32::from_rgb(32, 46, 65),
                stroke: Color32::from_rgb(40, 56, 78),
                text: Color32::from_rgb(226, 235, 245),
                muted: Color32::from_rgb(138, 155, 178),
                accent: Color32::from_rgb(56, 189, 248),
                accent2: Color32::from_rgb(129, 140, 248),
                on_accent: Color32::from_rgb(6, 20, 30),
                good: Color32::from_rgb(74, 222, 128),
                warn: Color32::from_rgb(251, 191, 36),
                bad: Color32::from_rgb(248, 113, 113),
                warn_bg: Color32::from_rgb(38, 36, 18),
                is_light: false,
            },
            ThemeKind::Emerald => Palette {
                panel: Color32::from_rgb(13, 23, 20),
                window: Color32::from_rgb(18, 30, 26),
                extreme: Color32::from_rgb(8, 15, 13),
                faint: Color32::from_rgb(19, 31, 27),
                widget: Color32::from_rgb(23, 38, 33),
                widget_hover: Color32::from_rgb(31, 49, 42),
                stroke: Color32::from_rgb(38, 62, 54),
                text: Color32::from_rgb(226, 240, 234),
                muted: Color32::from_rgb(139, 163, 154),
                accent: Color32::from_rgb(52, 211, 153),
                accent2: Color32::from_rgb(56, 189, 248),
                on_accent: Color32::from_rgb(6, 26, 20),
                good: Color32::from_rgb(110, 231, 183),
                warn: Color32::from_rgb(251, 191, 36),
                bad: Color32::from_rgb(248, 113, 113),
                warn_bg: Color32::from_rgb(40, 36, 18),
                is_light: false,
            },
            ThemeKind::Violet => Palette {
                panel: Color32::from_rgb(20, 17, 30),
                window: Color32::from_rgb(26, 22, 38),
                extreme: Color32::from_rgb(12, 10, 20),
                faint: Color32::from_rgb(27, 23, 40),
                widget: Color32::from_rgb(33, 28, 48),
                widget_hover: Color32::from_rgb(44, 37, 63),
                stroke: Color32::from_rgb(56, 48, 78),
                text: Color32::from_rgb(235, 231, 245),
                muted: Color32::from_rgb(158, 148, 180),
                accent: Color32::from_rgb(167, 139, 250),
                accent2: Color32::from_rgb(244, 114, 182),
                on_accent: Color32::from_rgb(22, 12, 40),
                good: Color32::from_rgb(74, 222, 128),
                warn: Color32::from_rgb(251, 191, 36),
                bad: Color32::from_rgb(248, 113, 113),
                warn_bg: Color32::from_rgb(40, 34, 20),
                is_light: false,
            },
            ThemeKind::Light => Palette {
                panel: Color32::from_rgb(244, 246, 250),
                window: Color32::from_rgb(255, 255, 255),
                extreme: Color32::from_rgb(232, 236, 242),
                faint: Color32::from_rgb(237, 241, 246),
                widget: Color32::from_rgb(235, 239, 245),
                widget_hover: Color32::from_rgb(224, 230, 239),
                stroke: Color32::from_rgb(205, 212, 222),
                text: Color32::from_rgb(24, 28, 36),
                muted: Color32::from_rgb(100, 110, 125),
                accent: Color32::from_rgb(37, 99, 235),
                accent2: Color32::from_rgb(8, 145, 178),
                on_accent: Color32::from_rgb(255, 255, 255),
                good: Color32::from_rgb(22, 163, 74),
                warn: Color32::from_rgb(180, 120, 0),
                bad: Color32::from_rgb(220, 38, 38),
                warn_bg: Color32::from_rgb(255, 247, 222),
                is_light: true,
            },
            ThemeKind::Sand => Palette {
                panel: Color32::from_rgb(247, 242, 234),
                window: Color32::from_rgb(255, 253, 248),
                extreme: Color32::from_rgb(239, 231, 218),
                faint: Color32::from_rgb(243, 237, 227),
                widget: Color32::from_rgb(237, 228, 213),
                widget_hover: Color32::from_rgb(227, 215, 196),
                stroke: Color32::from_rgb(216, 201, 180),
                text: Color32::from_rgb(43, 36, 28),
                muted: Color32::from_rgb(122, 106, 87),
                accent: Color32::from_rgb(194, 65, 12),
                accent2: Color32::from_rgb(15, 118, 110),
                on_accent: Color32::from_rgb(255, 255, 255),
                good: Color32::from_rgb(21, 128, 61),
                warn: Color32::from_rgb(180, 83, 9),
                bad: Color32::from_rgb(185, 28, 28),
                warn_bg: Color32::from_rgb(254, 243, 199),
                is_light: true,
            },
            ThemeKind::Mint => Palette {
                panel: Color32::from_rgb(240, 247, 243),
                window: Color32::from_rgb(255, 255, 255),
                extreme: Color32::from_rgb(227, 239, 232),
                faint: Color32::from_rgb(235, 244, 239),
                widget: Color32::from_rgb(225, 238, 231),
                widget_hover: Color32::from_rgb(213, 231, 221),
                stroke: Color32::from_rgb(195, 216, 204),
                text: Color32::from_rgb(23, 37, 30),
                muted: Color32::from_rgb(95, 122, 108),
                accent: Color32::from_rgb(5, 150, 105),
                accent2: Color32::from_rgb(8, 145, 178),
                on_accent: Color32::from_rgb(255, 255, 255),
                good: Color32::from_rgb(21, 128, 61),
                warn: Color32::from_rgb(180, 83, 9),
                bad: Color32::from_rgb(185, 28, 28),
                warn_bg: Color32::from_rgb(254, 243, 199),
                is_light: true,
            },
            ThemeKind::Rose => Palette {
                panel: Color32::from_rgb(250, 243, 246),
                window: Color32::from_rgb(255, 255, 255),
                extreme: Color32::from_rgb(243, 228, 235),
                faint: Color32::from_rgb(247, 235, 240),
                widget: Color32::from_rgb(242, 225, 233),
                widget_hover: Color32::from_rgb(234, 211, 222),
                stroke: Color32::from_rgb(220, 194, 207),
                text: Color32::from_rgb(42, 24, 32),
                muted: Color32::from_rgb(125, 95, 109),
                accent: Color32::from_rgb(219, 39, 119),
                accent2: Color32::from_rgb(124, 58, 237),
                on_accent: Color32::from_rgb(255, 255, 255),
                good: Color32::from_rgb(21, 128, 61),
                warn: Color32::from_rgb(180, 83, 9),
                bad: Color32::from_rgb(185, 28, 28),
                warn_bg: Color32::from_rgb(254, 243, 199),
                is_light: true,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    panel: Color32,
    window: Color32,
    extreme: Color32,
    faint: Color32,
    widget: Color32,
    widget_hover: Color32,
    stroke: Color32,
    text: Color32,
    muted: Color32,
    accent: Color32,
    accent2: Color32,
    on_accent: Color32,
    good: Color32,
    warn: Color32,
    bad: Color32,
    warn_bg: Color32,
    is_light: bool,
}

const PRESETS: &[(&str, &str, &str)] = &[
    ("Батарея", "Battery", "dumpsys battery"),
    ("Память", "Memory", "dumpsys meminfo | head -n 60"),
    ("Пакеты", "Packages", "pm list packages -3"),
    ("Логи", "Logs", "logcat -d -t 200"),
    ("Свойства", "Properties", "getprop | head -n 80"),
    ("Диск", "Disk", "df -h"),
    ("Топ CPU", "Top CPU", "top -b -n 1 -m 15"),
    (
        "Яркость",
        "Brightness",
        "settings get system screen_brightness",
    ),
    ("Экран", "Display", "dumpsys display | head -n 40"),
];

pub struct App {
    devices: Vec<Device>,
    selected: Option<String>,
    info: Option<DeviceInfo>,
    snapshot: Option<Snapshot>,
    monitor: Option<Monitor>,
    interval: f32,
    status: String,
    status_error: bool,

    cpu_hist: VecDeque<f32>,
    gpu_hist: VecDeque<f32>,
    ram_hist: VecDeque<f32>,
    bat_hist: VecDeque<f32>,
    last_cpu_stat: Option<(u64, u64)>,
    cpu_smooth: Option<f32>,
    last_net: Option<(u64, u64, Instant)>,
    net_rx_rate: Option<f32>,
    net_tx_rate: Option<f32>,

    tab: Tab,
    theme: ThemeKind,
    lang: Lang,
    tx: Sender<TaskMsg>,
    rx: Receiver<TaskMsg>,

    connect_addr: String,
    confirm_reboot: bool,

    command: String,
    console_log: Vec<ConsoleEntry>,
    console_busy: bool,

    screen_texture: Option<TextureHandle>,
    screen_png: Option<Vec<u8>>,
    screen_w: usize,
    screen_h: usize,
    screen_pending: bool,
    screen_auto: bool,
    screen_interval_ms: f32,
    screen_tap: bool,
    last_screenshot: Instant,
    custom_buttons: Vec<CustomButton>,

    apps: Vec<String>,
    apps_loading: bool,
    apps_loaded: bool,
    apps_filter: String,
    selected_pkg: Option<String>,
    pkg_info: Option<String>,
    pkg_info_loading: bool,

    tech_info: Option<String>,
    tech_loading: bool,

    ping_adb: Option<f32>,
    ping_net: Option<f32>,
    ping_busy: bool,

    tools: Tools,
    repo_name: String,
    repo_desc: String,
    repo_private: bool,
    repo_token: String,
    publish_log: Vec<String>,
    publish_busy: bool,
    published_url: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let theme = match std::env::var("ADBW_THEME")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "ocean" => ThemeKind::Ocean,
            "emerald" => ThemeKind::Emerald,
            "violet" => ThemeKind::Violet,
            "light" => ThemeKind::Light,
            "sand" => ThemeKind::Sand,
            "mint" => ThemeKind::Mint,
            "rose" => ThemeKind::Rose,
            _ => ThemeKind::Graphite,
        };
        let lang = if std::env::var("ADBW_LANG")
            .map(|v| v.eq_ignore_ascii_case("en"))
            .unwrap_or(false)
        {
            Lang::En
        } else {
            Lang::Ru
        };
        let tab = match std::env::var("ADBW_TAB")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "processes" => Tab::Processes,
            "apps" => Tab::Apps,
            "screen" => Tab::Screen,
            "console" => Tab::Console,
            "info" => Tab::Info,
            "github" => Tab::GitHub,
            _ => Tab::Overview,
        };
        let screen_auto = std::env::var("ADBW_SCREEN_AUTO")
            .map(|v| v == "1")
            .unwrap_or(false);
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx, &theme.palette());

        let (tx, rx) = channel();
        let mut app = Self {
            devices: Vec::new(),
            selected: None,
            info: None,
            snapshot: None,
            monitor: None,
            interval: 2.0,
            status: tr(lang, "Поиск устройств…", "Searching for devices…").to_string(),
            status_error: false,
            cpu_hist: VecDeque::new(),
            gpu_hist: VecDeque::new(),
            ram_hist: VecDeque::new(),
            bat_hist: VecDeque::new(),
            last_cpu_stat: None,
            cpu_smooth: None,
            last_net: None,
            net_rx_rate: None,
            net_tx_rate: None,
            tab,
            theme,
            lang,
            tx,
            rx,
            connect_addr: String::new(),
            confirm_reboot: false,
            command: "dumpsys battery".to_string(),
            console_log: Vec::new(),
            console_busy: false,
            screen_texture: None,
            screen_png: None,
            screen_w: 0,
            screen_h: 0,
            screen_pending: false,
            screen_auto,
            screen_interval_ms: 500.0,
            screen_tap: true,
            last_screenshot: Instant::now(),
            custom_buttons: default_buttons(),
            apps: Vec::new(),
            apps_loading: false,
            apps_loaded: false,
            apps_filter: String::new(),
            selected_pkg: None,
            pkg_info: None,
            pkg_info_loading: false,
            tech_info: None,
            tech_loading: false,
            ping_adb: None,
            ping_net: None,
            ping_busy: false,
            tools: publish::tools(),
            repo_name: "adb-wizard".to_string(),
            repo_desc: "Монитор Android-устройства через ADB: нагрузка, процессы, экран, консоль"
                .to_string(),
            repo_private: false,
            repo_token: String::new(),
            publish_log: Vec::new(),
            publish_busy: false,
            published_url: None,
        };
        app.refresh_devices();
        app
    }

    fn palette(&self) -> Palette {
        self.theme.palette()
    }

    fn refresh_devices(&mut self) {
        match adb::list_devices() {
            Ok(list) => {
                self.devices = list;
                let online = self
                    .devices
                    .iter()
                    .find(|d| d.state == "device")
                    .map(|d| d.serial.clone());

                let selected_ok = self.selected.as_ref().is_some_and(|sel| {
                    self.devices
                        .iter()
                        .any(|d| &d.serial == sel && d.state == "device")
                });

                if !selected_ok {
                    if let Some(serial) = online {
                        self.select_device(serial);
                    } else {
                        self.monitor = None;
                        self.selected = None;
                        self.snapshot = None;
                        self.info = None;
                        self.status = if self.devices.is_empty() {
                            tr(
                                self.lang,
                                "Устройства не найдены. Подключите телефон по USB и включите отладку.",
                                "No devices found. Connect a phone via USB and enable debugging.",
                            )
                            .into()
                        } else {
                            tr(
                                self.lang,
                                "Устройство найдено, но не готово (offline/unauthorized).",
                                "A device was found but is not ready (offline/unauthorized).",
                            )
                            .into()
                        };
                        self.status_error = true;
                    }
                } else {
                    self.status = format!(
                        "{} {}",
                        tr(self.lang, "Найдено устройств:", "Devices found:"),
                        self.devices.len()
                    );
                    self.status_error = false;
                }
            }
            Err(e) => {
                self.status = format!("adb: {e}");
                self.status_error = true;
            }
        }
    }

    fn select_device(&mut self, serial: String) {
        self.monitor = None;
        self.selected = Some(serial.clone());
        self.info = None;
        self.snapshot = None;
        self.cpu_hist.clear();
        self.gpu_hist.clear();
        self.ram_hist.clear();
        self.bat_hist.clear();
        self.last_cpu_stat = None;
        self.cpu_smooth = None;
        self.last_net = None;
        self.net_rx_rate = None;
        self.net_tx_rate = None;
        self.apps.clear();
        self.apps_loaded = false;
        self.selected_pkg = None;
        self.pkg_info = None;
        self.tech_info = None;
        self.start_monitor();

        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::Info(adb::fetch_info(&serial)));
        });
    }

    fn start_monitor(&mut self) {
        if let Some(serial) = self.selected.clone() {
            self.monitor = Some(adb::start_monitor(
                serial,
                Duration::from_secs_f32(self.interval.max(0.5)),
            ));
            self.status = format!(
                "{}{:.1} {}",
                tr(
                    self.lang,
                    "Мониторинг запущен, интервал ",
                    "Monitoring started, interval ",
                ),
                self.interval,
                tr(self.lang, "с", "s"),
            );
            self.status_error = false;
        }
    }

    fn stop_monitor(&mut self) {
        self.monitor = None;
        self.status = tr(self.lang, "Мониторинг остановлен", "Monitoring stopped").to_string();
        self.status_error = false;
    }

    fn refresh_once(&mut self) {
        let Some(serial) = self.selected.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::Snapshot(adb::collect_snapshot(&serial)));
        });
        self.status = tr(self.lang, "Читаю состояние…", "Reading state…").to_string();
    }

    fn connect_device(&mut self) {
        let addr = self.connect_addr.trim().to_string();
        if addr.is_empty() {
            self.status = tr(
                self.lang,
                "Введите адрес, например 192.168.1.50:5555",
                "Enter an address, e.g. 192.168.1.50:5555",
            )
            .into();
            self.status_error = true;
            return;
        }
        match adb::connect(&addr) {
            Ok(out) => {
                self.status = out.trim().to_string();
                self.status_error = !self.status.to_lowercase().contains("connected");
                self.refresh_devices();
            }
            Err(e) => {
                self.status = format!("adb connect: {e}");
                self.status_error = true;
            }
        }
    }

    fn action(&mut self, shell_cmd: &str, title: &str) {
        let Some(serial) = self.selected.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let title = title.to_string();
        let cmd = shell_cmd.to_string();
        std::thread::spawn(move || {
            let (output, success) = match adb::shell(&serial, &cmd) {
                Ok(o) => (o, true),
                Err(e) => (e, false),
            };
            let _ = tx.send(TaskMsg::Console {
                title,
                output,
                success,
            });
        });
    }

    fn run_command(&mut self) {
        let cmd = self.command.trim().to_string();
        if cmd.is_empty() || self.console_busy {
            return;
        }
        let Some(serial) = self.selected.clone() else {
            self.status = tr(
                self.lang,
                "Сначала подключите устройство",
                "Connect a device first",
            )
            .into();
            self.status_error = true;
            return;
        };
        self.console_busy = true;
        let tx = self.tx.clone();
        let title = format!("adb shell {cmd}");
        std::thread::spawn(move || {
            let (output, success) = match adb::shell(&serial, &cmd) {
                Ok(o) => (o, true),
                Err(e) => (e, false),
            };
            let _ = tx.send(TaskMsg::Console {
                title,
                output,
                success,
            });
        });
    }

    fn request_screenshot(&mut self) {
        if self.screen_pending {
            return;
        }
        let Some(serial) = self.selected.clone() else {
            self.status = tr(
                self.lang,
                "Сначала подключите устройство",
                "Connect a device first",
            )
            .into();
            self.status_error = true;
            return;
        };
        self.screen_pending = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::Screenshot(adb::screencap(&serial)));
        });
    }

    fn load_apps(&mut self) {
        let Some(serial) = self.selected.clone() else {
            self.status = tr(
                self.lang,
                "Сначала подключите устройство",
                "Connect a device first",
            )
            .into();
            self.status_error = true;
            return;
        };
        if self.apps_loading {
            return;
        }
        self.apps_loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::Packages(adb::list_packages(&serial)));
        });
    }

    fn refresh_apps_later(&mut self) {
        let Some(serial) = self.selected.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(900));
            let _ = tx.send(TaskMsg::Packages(adb::list_packages(&serial)));
        });
    }

    fn load_package_info(&mut self) {
        let (Some(serial), Some(pkg)) = (self.selected.clone(), self.selected_pkg.clone()) else {
            return;
        };
        if self.pkg_info_loading {
            return;
        }
        self.pkg_info_loading = true;
        self.pkg_info = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::PackageInfo(adb::package_info(&serial, &pkg)));
        });
    }

    fn load_tech_info(&mut self) {
        let Some(serial) = self.selected.clone() else {
            self.status = tr(
                self.lang,
                "Сначала подключите устройство",
                "Connect a device first",
            )
            .into();
            self.status_error = true;
            return;
        };
        if self.tech_loading {
            return;
        }
        self.tech_loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::TechInfo(adb::tech_info(&serial)));
        });
    }

    fn run_ping(&mut self) {
        let Some(serial) = self.selected.clone() else {
            self.status = tr(
                self.lang,
                "Сначала подключите устройство",
                "Connect a device first",
            )
            .into();
            self.status_error = true;
            return;
        };
        if self.ping_busy {
            return;
        }
        self.ping_busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let adb_ping = adb::ping_adb(&serial);
            let net_ping = adb::ping_net(&serial);
            let _ = tx.send(TaskMsg::Ping(match (adb_ping, net_ping) {
                (Ok(a), Ok(n)) => Ok((a, n)),
                (Ok(a), Err(_)) => Ok((a, -1.0)),
                (Err(e), _) => Err(e),
            }));
        });
    }

    fn save_screenshot(&mut self) {
        let Some(png) = &self.screen_png else {
            self.status = tr(
                self.lang,
                "Сначала снимите экран",
                "Take a screenshot first",
            )
            .into();
            self.status_error = true;
            return;
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("device-screen.png")
            .add_filter("PNG", &["png"])
            .save_file()
        {
            match std::fs::write(&path, png) {
                Ok(_) => {
                    self.status = format!(
                        "{} {}",
                        tr(self.lang, "Скриншот сохранён:", "Screenshot saved:"),
                        path.display()
                    );
                    self.status_error = false;
                }
                Err(e) => {
                    self.status = format!(
                        "{} {e}",
                        tr(self.lang, "Не удалось сохранить:", "Could not save:"),
                    );
                    self.status_error = true;
                }
            }
        }
    }

    fn start_publish(&mut self) {
        if self.publish_busy {
            return;
        }
        self.publish_busy = true;
        self.publish_log.clear();
        self.published_url = None;
        let repo = self.repo_name.trim().to_string();
        let desc = self.repo_desc.trim().to_string();
        let private = self.repo_private;
        let token = self.repo_token.trim().to_string();
        let lang = self.lang;
        let tx = self.tx.clone();
        let (publish_tx, publish_rx) = channel::<PublishEvent>();
        std::thread::spawn(move || {
            while let Ok(ev) = publish_rx.recv() {
                if tx.send(TaskMsg::Publish(ev)).is_err() {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            publish::publish(&repo, &desc, private, &token, lang, &publish_tx);
        });
    }

    fn poll_monitor(&mut self) {
        let mut snaps = Vec::new();
        if let Some(m) = &self.monitor {
            while let Ok(s) = m.rx.try_recv() {
                snaps.push(s);
            }
        }
        for s in snaps {
            self.apply_snapshot(s);
        }
    }

    fn apply_snapshot(&mut self, mut snap: Snapshot) {
        if let Some(e) = &snap.error {
            self.status = format!("{} {e}", tr(self.lang, "Ошибка:", "Error:"));
            self.status_error = true;
            return;
        }

        snap.cores = self.info.as_ref().map(|i| i.cores).unwrap_or(0);

        let mut cpu_value = snap.cpu_percent;
        if let (Some(total), Some(idle)) = (snap.cpu_total_ticks, snap.cpu_idle_ticks) {
            if let Some((prev_total, prev_idle)) = self.last_cpu_stat {
                let dt = total.saturating_sub(prev_total);
                let di = idle.saturating_sub(prev_idle);
                if dt > 0 {
                    cpu_value = Some(
                        ((dt.saturating_sub(di)) as f32 / dt as f32 * 100.0).clamp(0.0, 100.0),
                    );
                }
            }
            self.last_cpu_stat = Some((total, idle));
        }
        if let Some(v) = cpu_value {
            let smoothed = self.cpu_smooth.map(|p| p * 0.55 + v * 0.45).unwrap_or(v);
            self.cpu_smooth = Some(smoothed);
            push_history(&mut self.cpu_hist, smoothed.clamp(0.0, 100.0));
            snap.cpu_percent = Some(smoothed);
        }
        if let Some(gpu) = snap.gpu_percent {
            push_history(&mut self.gpu_hist, gpu.clamp(0.0, 100.0));
        }
        if let (Some(total), Some(used)) = (snap.ram_total_kb, snap.ram_used_kb) {
            if total > 0 {
                push_history(
                    &mut self.ram_hist,
                    (used as f32 / total as f32 * 100.0).clamp(0.0, 100.0),
                );
            }
        }
        if let Some(level) = snap.battery.level {
            push_history(&mut self.bat_hist, level as f32);
        }

        if let (Some(rx), Some(tx)) = (snap.net_rx_bytes, snap.net_tx_bytes) {
            if let Some((prev_rx, prev_tx, at)) = self.last_net {
                let dt = at.elapsed().as_secs_f32().max(0.05);
                if rx >= prev_rx && tx >= prev_tx {
                    self.net_rx_rate = Some((rx - prev_rx) as f32 / dt);
                    self.net_tx_rate = Some((tx - prev_tx) as f32 / dt);
                }
            }
            self.last_net = Some((rx, tx, Instant::now()));
        }

        self.status = format!(
            "{} {}",
            tr(self.lang, "Обновлено в", "Updated at"),
            snap.time
        );
        self.status_error = false;
        self.snapshot = Some(snap);
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        let mut msgs = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            msgs.push(m);
        }
        for msg in msgs {
            match msg {
                TaskMsg::Console {
                    title,
                    output,
                    success,
                } => {
                    self.console_busy = false;
                    self.console_log.push(ConsoleEntry {
                        time: chrono::Local::now().format("%H:%M:%S").to_string(),
                        title,
                        output,
                        success,
                    });
                    if self.console_log.len() > 40 {
                        self.console_log.remove(0);
                    }
                }
                TaskMsg::Info(Ok(info)) => {
                    self.info = Some(info);
                }
                TaskMsg::Info(Err(e)) => {
                    self.status = format!(
                        "{} {e}",
                        tr(
                            self.lang,
                            "Не удалось получить сведения:",
                            "Could not get device info:",
                        )
                    );
                    self.status_error = true;
                }
                TaskMsg::Snapshot(s) => {
                    self.apply_snapshot(s);
                }
                TaskMsg::Screenshot(Ok(img)) => {
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [img.width, img.height],
                        &img.rgba,
                    );
                    self.screen_texture =
                        Some(ctx.load_texture("device_screen", color, TextureOptions::LINEAR));
                    self.screen_w = img.width;
                    self.screen_h = img.height;
                    self.screen_png = Some(img.png);
                    self.screen_pending = false;
                    self.last_screenshot = Instant::now();
                    self.status = format!(
                        "{} {}×{}",
                        tr(self.lang, "Скриншот", "Screenshot"),
                        img.width,
                        img.height
                    );
                    self.status_error = false;
                }
                TaskMsg::Screenshot(Err(e)) => {
                    self.screen_pending = false;
                    self.status = format!("{} {e}", tr(self.lang, "Скриншот:", "Screenshot:"));
                    self.status_error = true;
                }
                TaskMsg::Publish(PublishEvent::Log(line)) => {
                    self.publish_log.push(line);
                }
                TaskMsg::Publish(PublishEvent::Done(res)) => {
                    self.publish_busy = false;
                    match res {
                        Ok(url) => {
                            self.published_url = Some(url.clone());
                            self.publish_log.push(format!(
                                "{} {url}",
                                tr(self.lang, "Публикация завершена:", "Published:")
                            ));
                            self.status =
                                format!("{} {url}", tr(self.lang, "Опубликовано:", "Published:"));
                            self.status_error = false;
                        }
                        Err(e) => {
                            self.publish_log
                                .push(format!("{} {e}", tr(self.lang, "Ошибка:", "Error:")));
                            self.status = format!("GitHub: {e}");
                            self.status_error = true;
                        }
                    }
                }
                TaskMsg::Packages(Ok(list)) => {
                    self.apps_loading = false;
                    self.apps_loaded = true;
                    self.apps = list;
                    if self
                        .selected_pkg
                        .as_ref()
                        .is_some_and(|p| !self.apps.contains(p))
                    {
                        self.selected_pkg = None;
                        self.pkg_info = None;
                    }
                    self.status = format!(
                        "{} {}",
                        tr(self.lang, "Приложений:", "Apps:"),
                        self.apps.len()
                    );
                    self.status_error = false;
                }
                TaskMsg::Packages(Err(e)) => {
                    self.apps_loading = false;
                    self.apps_loaded = true;
                    self.status = format!("{} {e}", tr(self.lang, "Приложения:", "Apps:"));
                    self.status_error = true;
                }
                TaskMsg::PackageInfo(Ok(v)) => {
                    self.pkg_info_loading = false;
                    self.pkg_info = Some(v);
                }
                TaskMsg::PackageInfo(Err(e)) => {
                    self.pkg_info_loading = false;
                    self.pkg_info = Some(format!("{} {e}", tr(self.lang, "Ошибка:", "Error:")));
                }
                TaskMsg::TechInfo(Ok(v)) => {
                    self.tech_loading = false;
                    self.tech_info = Some(v);
                    self.status = tr(
                        self.lang,
                        "Тех. информация загружена",
                        "Technical info loaded",
                    )
                    .into();
                    self.status_error = false;
                }
                TaskMsg::TechInfo(Err(e)) => {
                    self.tech_loading = false;
                    self.status = format!(
                        "{} {e}",
                        tr(self.lang, "Тех. информация:", "Technical info:")
                    );
                    self.status_error = true;
                }
                TaskMsg::Ping(Ok((adb_ping, net_ping))) => {
                    self.ping_busy = false;
                    self.ping_adb = Some(adb_ping);
                    self.ping_net = if net_ping >= 0.0 {
                        Some(net_ping)
                    } else {
                        None
                    };
                    self.status = match self.ping_net {
                        Some(n) => format!("ADB {adb_ping:.1} ms · Internet {n:.1} ms"),
                        None => format!("ADB {adb_ping:.1} ms · Internet: n/a"),
                    };
                    self.status_error = false;
                }
                TaskMsg::Ping(Err(e)) => {
                    self.ping_busy = false;
                    self.status = format!("Ping: {e}");
                    self.status_error = true;
                }
            }
        }
    }

    fn auto_screenshot(&mut self, ctx: &egui::Context) {
        if self.tab != Tab::Screen || !self.screen_auto {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        if !self.screen_pending
            && self.last_screenshot.elapsed()
                >= Duration::from_millis(self.screen_interval_ms.max(100.0) as u64)
        {
            self.request_screenshot();
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(pal.window)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ADB Wizard")
                            .size(20.0)
                            .strong()
                            .color(pal.accent),
                    );
                    ui.label(
                        RichText::new(tr(self.lang, "монитор устройства", "device monitor"))
                            .color(pal.muted),
                    );
                    ui.separator();

                    let current = self.selected.clone().unwrap_or_else(|| {
                        tr(self.lang, "— нет устройства —", "— no device —").to_string()
                    });
                    let mut picked: Option<String> = None;
                    egui::ComboBox::from_id_salt("device_combo")
                        .selected_text(current)
                        .width(340.0)
                        .show_ui(ui, |ui| {
                            if self.devices.is_empty() {
                                ui.label(tr(self.lang, "Список пуст", "The list is empty"));
                            }
                            for d in &self.devices {
                                let mut label = format!("{}  [{}]", d.serial, d.state);
                                if !d.model.is_empty() {
                                    label.push_str("  ");
                                    label.push_str(&d.model);
                                }
                                if ui
                                    .selectable_label(
                                        self.selected.as_deref() == Some(d.serial.as_str()),
                                        label,
                                    )
                                    .clicked()
                                {
                                    picked = Some(d.serial.clone());
                                }
                            }
                        });
                    if let Some(s) = picked {
                        self.select_device(s);
                    }

                    if ui
                        .button(tr(self.lang, "Обновить список", "Refresh list"))
                        .clicked()
                    {
                        self.refresh_devices();
                    }

                    ui.separator();

                    if self.monitor.is_some() {
                        if ui.button(tr(self.lang, "Стоп", "Stop")).clicked() {
                            self.stop_monitor();
                        }
                    } else if ui.button(tr(self.lang, "Мониторинг", "Monitor")).clicked()
                    {
                        self.start_monitor();
                    }
                    if ui.button(tr(self.lang, "Снимок", "Snapshot")).clicked() {
                        self.refresh_once();
                    }

                    if self.ping_busy {
                        ui.spinner();
                    } else if ui.button(tr(self.lang, "Пинг", "Ping")).clicked() {
                        self.run_ping();
                    }
                    if let Some(ms) = self.ping_adb {
                        ui.label(RichText::new(format!("ADB {ms:.0} ms")).color(pal.muted));
                    }
                    if let Some(ms) = self.ping_net {
                        ui.label(RichText::new(format!("Net {ms:.0} ms")).color(pal.muted));
                    }

                    ui.label(RichText::new(tr(self.lang, "интервал", "interval")).color(pal.muted));
                    let resp = ui.add(
                        egui::DragValue::new(&mut self.interval)
                            .range(0.5..=10.0)
                            .speed(0.1)
                            .suffix(tr(self.lang, " с", " s")),
                    );
                    if resp.changed() && self.monitor.is_some() {
                        self.start_monitor();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Wi-Fi:").color(pal.muted));
                    ui.add(
                        TextEdit::singleline(&mut self.connect_addr)
                            .hint_text("192.168.1.50:5555")
                            .desired_width(170.0),
                    );
                    if ui.button(tr(self.lang, "Подключить", "Connect")).clicked() {
                        self.connect_device();
                    }
                    ui.separator();
                    if let Some(info) = &self.info {
                        let name = if info.manufacturer.is_empty() {
                            info.model.clone()
                        } else {
                            format!("{} {}", info.manufacturer, info.model)
                        };
                        ui.label(RichText::new(name).strong());
                        ui.label(
                            RichText::new(format!(
                                "Android {} · {} · {} {}",
                                info.android,
                                info.resolution,
                                info.cores,
                                tr(self.lang, "ядер", "cores")
                            ))
                            .color(pal.muted),
                        );
                    } else if self.selected.is_some() {
                        ui.label(
                            RichText::new(tr(
                                self.lang,
                                "сведения об устройстве загружаются…",
                                "loading device info…",
                            ))
                            .color(pal.muted),
                        );
                    } else {
                        ui.label(
                            RichText::new(tr(
                                self.lang,
                                "устройство не выбрано",
                                "no device selected",
                            ))
                            .color(pal.muted),
                        );
                    }
                });
            });
    }

    fn bottom_bar(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        egui::TopBottomPanel::bottom("bottom_bar")
            .frame(
                egui::Frame::new()
                    .fill(pal.window)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let color = if self.status_error {
                        pal.bad
                    } else {
                        pal.muted
                    };
                    ui.label(RichText::new(&self.status).color(color));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("ADB Wizard {}", env!("CARGO_PKG_VERSION")))
                                .color(pal.muted),
                        );
                        if self.monitor.is_some() {
                            ui.spinner();
                            ui.label(RichText::new("live").color(pal.good));
                        }
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                tr(self.lang, "устройств:", "devices:"),
                                self.devices.len()
                            ))
                            .color(pal.muted),
                        );
                    });
                });
            });
    }

    fn tabs(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tab, label) in [
                    (Tab::Overview, tr(self.lang, "Обзор", "Overview")),
                    (Tab::Processes, tr(self.lang, "Процессы", "Processes")),
                    (Tab::Apps, tr(self.lang, "Приложения", "Apps")),
                    (Tab::Screen, tr(self.lang, "Экран", "Screen")),
                    (Tab::Console, tr(self.lang, "Консоль", "Console")),
                    (Tab::Info, tr(self.lang, "Инфо", "Info")),
                    (Tab::GitHub, "GitHub"),
                ] {
                    let selected = self.tab == tab;
                    let text = RichText::new(label).size(15.0);
                    let text = if selected {
                        text.strong().color(pal.accent)
                    } else {
                        text.color(pal.text)
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        if self.tab != tab && tab == Tab::Screen {
                            self.request_screenshot();
                        }
                        self.tab = tab;
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for lang in Lang::ALL {
                        let selected = self.lang == lang;
                        let text = RichText::new(lang.label()).size(13.0);
                        let text = if selected {
                            text.strong().color(pal.accent)
                        } else {
                            text.color(pal.muted)
                        };
                        if ui.selectable_label(selected, text).clicked() {
                            self.lang = lang;
                        }
                    }
                    ui.label(RichText::new(tr(self.lang, "Язык", "Lang")).color(pal.muted));
                    ui.separator();

                    let mut picked = self.theme;
                    egui::ComboBox::from_id_salt("theme_combo")
                        .selected_text(self.theme.label(self.lang))
                        .width(132.0)
                        .show_ui(ui, |ui| {
                            for theme in ThemeKind::ALL {
                                if ui
                                    .selectable_label(self.theme == theme, theme.label(self.lang))
                                    .clicked()
                                {
                                    picked = theme;
                                }
                            }
                        });
                    ui.colored_label(pal.accent, "●");
                    ui.label(RichText::new(tr(self.lang, "Тема", "Theme")).color(pal.muted));
                    if picked != self.theme {
                        self.theme = picked;
                        setup_style(ui.ctx(), &picked.palette());
                    }
                });
            });
            ui.separator();
            match self.tab {
                Tab::Overview => self.overview(ui),
                Tab::Processes => self.processes(ui),
                Tab::Apps => self.apps_tab(ui),
                Tab::Screen => self.screen(ui),
                Tab::Console => self.console(ui),
                Tab::Info => self.info_tab(ui),
                Tab::GitHub => self.github(ui),
            }
        });
    }

    fn overview(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        let snap = self.snapshot.clone();
        let info = self.info.clone();
        let pal = self.palette();

        if let Some(info) = &info {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        info.manufacturer.trim(),
                        info.model.trim()
                    ))
                    .size(19.0)
                    .strong(),
                );
                ui.separator();
                ui.label(format!("Android {}", info.android));
                ui.label(RichText::new(format!("SDK {}", info.sdk)).color(pal.muted));
                ui.label(RichText::new(&info.abi).color(pal.muted));
                ui.separator();
                ui.label(format!("{} · {}", info.resolution, info.density));
                if let Some(s) = &snap {
                    if let Some(up) = s.uptime_secs {
                        ui.label(
                            RichText::new(format!("uptime {}", human_uptime(up))).color(pal.muted),
                        );
                    }
                }
            });
        }

        ui.add_space(6.0);

        if let Some(s) = &snap {
            let cpu = s.cpu_percent.unwrap_or(0.0);
            let ram_pct = match (s.ram_total_kb, s.ram_used_kb) {
                (Some(t), Some(u)) if t > 0 => u as f32 / t as f32 * 100.0,
                _ => 0.0,
            };
            let storage_pct = match (s.storage_total_kb, s.storage_used_kb) {
                (Some(t), Some(u)) if t > 0 => u as f32 / t as f32 * 100.0,
                _ => 0.0,
            };
            let battery = s.battery.level.unwrap_or(0) as f32;
            let cores = info
                .as_ref()
                .map(|i| i.cores)
                .filter(|c| *c > 0)
                .unwrap_or(s.cores);

            ui.columns(4, |cols| {
                metric_card(
                    &mut cols[0],
                    &pal,
                    "CPU",
                    &format!("{cpu:.0}%"),
                    &format!("{cores} {}", tr(self.lang, "ядер", "cores")),
                    Some(cpu / 100.0),
                    load_color(cpu, &pal),
                );
                metric_card(
                    &mut cols[1],
                    &pal,
                    tr(self.lang, "Память", "Memory"),
                    &format!("{ram_pct:.0}%"),
                    &format!(
                        "{} {} {}",
                        human_kb(s.ram_used_kb.unwrap_or(0)),
                        tr(self.lang, "из", "of"),
                        human_kb(s.ram_total_kb.unwrap_or(0))
                    ),
                    Some(ram_pct / 100.0),
                    load_color(ram_pct, &pal),
                );
                let bat_sub = match (s.battery.temp_c, s.battery.status.as_str()) {
                    (Some(t), st) if !st.is_empty() => format!("{t:.0} °C · {st}"),
                    (Some(t), _) => format!("{t:.0} °C"),
                    _ => tr(self.lang, "нет данных", "no data").to_string(),
                };
                metric_card(
                    &mut cols[2],
                    &pal,
                    tr(self.lang, "Батарея", "Battery"),
                    &format!("{battery:.0}%"),
                    &bat_sub,
                    Some(battery / 100.0),
                    battery_color(battery, &pal),
                );
                metric_card(
                    &mut cols[3],
                    &pal,
                    tr(self.lang, "Накопитель /data", "Storage /data"),
                    &format!("{storage_pct:.0}%"),
                    &format!(
                        "{} {} {}",
                        human_kb(s.storage_used_kb.unwrap_or(0)),
                        tr(self.lang, "из", "of"),
                        human_kb(s.storage_total_kb.unwrap_or(0))
                    ),
                    Some(storage_pct / 100.0),
                    load_color(storage_pct, &pal),
                );
            });

            ui.add_space(8.0);
            let battery_line_color = self
                .bat_hist
                .back()
                .map(|v| battery_color(*v, &pal))
                .unwrap_or(pal.good);
            ui.columns(4, |cols| {
                sparkline(
                    &mut cols[0],
                    &pal,
                    self.lang,
                    tr(self.lang, "Загрузка CPU", "CPU load"),
                    &self.cpu_hist,
                    0.0,
                    100.0,
                    pal.accent,
                    "%",
                );
                sparkline(
                    &mut cols[1],
                    &pal,
                    self.lang,
                    tr(self.lang, "Занятая память", "Used memory"),
                    &self.ram_hist,
                    0.0,
                    100.0,
                    pal.accent2,
                    "%",
                );
                sparkline(
                    &mut cols[2],
                    &pal,
                    self.lang,
                    tr(self.lang, "Батарея", "Battery"),
                    &self.bat_hist,
                    0.0,
                    100.0,
                    battery_line_color,
                    "%",
                );
                sparkline(
                    &mut cols[3],
                    &pal,
                    self.lang,
                    "GPU",
                    &self.gpu_hist,
                    0.0,
                    100.0,
                    pal.warn,
                    "%",
                );
            });

            ui.horizontal_wrapped(|ui| {
                if let Some(rx) = self.net_rx_rate {
                    ui.label(
                        RichText::new(format!(
                            "{} {}/{}",
                            tr(self.lang, "Сеть ↓", "Net ↓"),
                            human_bytes(rx as u64),
                            tr(self.lang, "с", "s")
                        ))
                        .color(pal.muted),
                    );
                }
                if let Some(tx) = self.net_tx_rate {
                    ui.label(
                        RichText::new(format!(
                            "↑ {}/{}",
                            human_bytes(tx as u64),
                            tr(self.lang, "с", "s")
                        ))
                        .color(pal.muted),
                    );
                }
                if let Some(rx) = s.net_rx_bytes {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            tr(self.lang, "всего ↓", "total ↓"),
                            human_bytes(rx)
                        ))
                        .color(pal.muted),
                    );
                }
                if let Some(tx) = s.net_tx_bytes {
                    ui.label(RichText::new(format!("↑ {}", human_bytes(tx))).color(pal.muted));
                }
            });
        } else {
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr(
                    self.lang,
                    "Получаю первый снимок состояния…",
                    "Getting the first snapshot…",
                ));
            });
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new(tr(self.lang, "Быстрые действия", "Quick actions")).strong());
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr(self.lang, "Домой", "Home")).clicked() {
                self.action("input keyevent 3", tr(self.lang, "Домой", "Home"));
            }
            if ui.button(tr(self.lang, "Назад", "Back")).clicked() {
                self.action("input keyevent 4", tr(self.lang, "Назад", "Back"));
            }
            if ui.button(tr(self.lang, "Недавние", "Recents")).clicked() {
                self.action("input keyevent 187", tr(self.lang, "Недавние", "Recents"));
            }
            if ui
                .button(tr(self.lang, "Экран вкл/выкл", "Screen on/off"))
                .clicked()
            {
                self.action("input keyevent 26", tr(self.lang, "Питание", "Power"));
            }
            if ui.button(tr(self.lang, "Разбудить", "Wake")).clicked() {
                self.action("input keyevent 224", tr(self.lang, "Разбудить", "Wake"));
            }
            if ui
                .button(tr(self.lang, "Громкость +", "Volume +"))
                .clicked()
            {
                self.action(
                    "input keyevent 24",
                    tr(self.lang, "Громкость +", "Volume +"),
                );
            }
            if ui
                .button(tr(self.lang, "Громкость −", "Volume −"))
                .clicked()
            {
                self.action(
                    "input keyevent 25",
                    tr(self.lang, "Громкость −", "Volume −"),
                );
            }
            if ui.button(tr(self.lang, "Скриншот", "Screenshot")).clicked() {
                self.tab = Tab::Screen;
                self.request_screenshot();
            }
            if ui
                .button(RichText::new(tr(self.lang, "Перезагрузить", "Reboot")).color(pal.bad))
                .clicked()
            {
                self.confirm_reboot = true;
            }
        });

        ui.add_space(6.0);
        ui.label(
            RichText::new(tr(
                self.lang,
                "Подсказка: вкладка «Процессы» показывает, что именно ест CPU и память.",
                "Tip: the “Processes” tab shows what is using CPU and memory.",
            ))
            .color(pal.muted),
        );
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.label(
                RichText::new(tr(
                    self.lang,
                    "Устройство не подключено",
                    "Device not connected",
                ))
                .size(24.0)
                .strong(),
            );
            ui.add_space(8.0);
            ui.label(tr(
                self.lang,
                "1. На телефоне включите «Для разработчиков» → «Отладка по USB».",
                "1. On the phone, enable “Developer options” → “USB debugging”.",
            ));
            ui.label(tr(
                self.lang,
                "2. Подключите кабель и подтвердите запрос на компьютере.",
                "2. Connect the cable and confirm the prompt on the computer.",
            ));
            ui.label(tr(
                self.lang,
                "3. Нажмите «Обновить список».",
                "3. Press “Refresh list”.",
            ));
            ui.add_space(12.0);
            if ui
                .button(tr(self.lang, "Обновить список", "Refresh list"))
                .clicked()
            {
                self.refresh_devices();
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(tr(
                    self.lang,
                    "Или подключитесь по Wi-Fi: adb tcpip 5555, затем адрес в верхней панели.",
                    "Or connect over Wi-Fi: adb tcpip 5555, then enter the address in the top bar.",
                ))
                .color(pal.muted),
            );
        });
    }

    fn processes(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        let pal = self.palette();
        let (cpu, mem) = match &self.snapshot {
            Some(s) => (s.top_cpu.clone(), s.top_mem.clone()),
            None => (Vec::new(), Vec::new()),
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(
                    self.lang,
                    "Что потребляет ресурсы",
                    "Resource consumers",
                ))
                .strong(),
            );
            if ui
                .button(tr(self.lang, "Обновить сейчас", "Refresh now"))
                .clicked()
            {
                self.refresh_once();
            }
            ui.label(
                RichText::new(tr(
                    self.lang,
                    "данные обновляются вместе с мониторингом",
                    "data updates together with monitoring",
                ))
                .color(pal.muted),
            );
        });
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            process_list(
                &mut cols[0],
                tr(self.lang, "Топ по CPU", "Top by CPU"),
                &cpu,
                "%",
                pal.accent,
                &pal,
                self.lang,
            );
            process_list(
                &mut cols[1],
                tr(self.lang, "Топ по памяти (PSS)", "Top by memory (PSS)"),
                &mem,
                tr(self.lang, "МБ", "MB"),
                pal.accent2,
                &pal,
                self.lang,
            );
        });
    }

    fn apps_tab(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        let pal = self.palette();
        let lang = self.lang;
        if !self.apps_loaded && !self.apps_loading {
            self.load_apps();
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(lang, "Приложения пользователя", "User apps")).strong());
            if self.apps_loading {
                ui.spinner();
            } else if ui
                .button(tr(lang, "Обновить список", "Refresh list"))
                .clicked()
            {
                self.load_apps();
            }
            if ui
                .button(tr(lang, "Тех. инфо устройства", "Device tech info"))
                .clicked()
            {
                self.tab = Tab::Info;
                self.load_tech_info();
            }
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(lang, "Поиск:", "Search:")).color(pal.muted));
            ui.add(
                TextEdit::singleline(&mut self.apps_filter)
                    .hint_text(tr(lang, "часть имени пакета", "part of package name"))
                    .desired_width(280.0),
            );
            if ui.button(tr(lang, "Сбросить", "Reset")).clicked() {
                self.apps_filter.clear();
            }
        });

        let filter = self.apps_filter.trim().to_lowercase();
        let filtered: Vec<String> = self
            .apps
            .iter()
            .filter(|p| filter.is_empty() || p.to_lowercase().contains(&filter))
            .cloned()
            .collect();

        ui.label(
            RichText::new(format!(
                "{} {} / {}",
                tr(lang, "Найдено:", "Found:"),
                filtered.len(),
                self.apps.len()
            ))
            .color(pal.muted),
        );

        if let Some(pkg) = self.selected_pkg.clone() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&pkg).monospace().color(pal.accent));
                if ui.button(tr(lang, "Настройки", "Settings")).clicked() {
                    let cmd = format!(
                        "am start -a android.settings.APPLICATION_DETAILS_SETTINGS -d package:{pkg}"
                    );
                    self.action(&cmd, tr(lang, "Настройки приложения", "App settings"));
                }
                if ui.button(tr(lang, "Остановить", "Force stop")).clicked() {
                    let cmd = format!("am force-stop {pkg}");
                    self.action(&cmd, tr(lang, "Остановка приложения", "Force stop"));
                }
                if ui
                    .button(tr(lang, "Очистить данные", "Clear data"))
                    .clicked()
                {
                    let cmd = format!("pm clear {pkg}");
                    self.action(&cmd, tr(lang, "Очистка данных", "Clear data"));
                }
                if ui.button(tr(lang, "Отключить", "Disable")).clicked() {
                    let cmd = format!("pm disable-user --user 0 {pkg}");
                    self.action(&cmd, tr(lang, "Отключение", "Disable"));
                    self.refresh_apps_later();
                }
                if ui.button(tr(lang, "Включить", "Enable")).clicked() {
                    let cmd = format!("pm enable {pkg}");
                    self.action(&cmd, tr(lang, "Включение", "Enable"));
                    self.refresh_apps_later();
                }
                if ui
                    .button(RichText::new(tr(lang, "Удалить", "Uninstall")).color(pal.bad))
                    .clicked()
                {
                    let cmd = format!("pm uninstall --user 0 {pkg}");
                    self.action(&cmd, tr(lang, "Удаление", "Uninstall"));
                    self.refresh_apps_later();
                }
            });
        }

        ui.separator();
        let list_height = (ui.available_height() * 0.45).max(120.0);
        ScrollArea::vertical()
            .max_height(list_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.apps.is_empty() && !self.apps_loading {
                    ui.label(
                        RichText::new(tr(
                            lang,
                            "Список пуст. Нажмите «Обновить список».",
                            "The list is empty. Press “Refresh list”.",
                        ))
                        .color(pal.muted),
                    );
                }
                for pkg in &filtered {
                    let selected = self.selected_pkg.as_deref() == Some(pkg.as_str());
                    if ui
                        .selectable_label(selected, RichText::new(pkg).monospace())
                        .clicked()
                    {
                        self.selected_pkg = Some(pkg.clone());
                        self.pkg_info = None;
                    }
                }
            });

        if let Some(pkg) = self.selected_pkg.clone() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} {pkg}",
                        tr(lang, "Тех. информация:", "Tech info:")
                    ))
                    .strong(),
                );
                if self.pkg_info_loading {
                    ui.spinner();
                } else if ui.button(tr(lang, "Показать", "Show")).clicked() {
                    self.load_package_info();
                }
                if let Some(info) = &self.pkg_info {
                    if ui.button(tr(lang, "Копировать", "Copy")).clicked() {
                        ui.ctx().copy_text(info.clone());
                        self.status = tr(lang, "Скопировано", "Copied").into();
                        self.status_error = false;
                    }
                }
            });
            if let Some(info) = &self.pkg_info {
                ScrollArea::vertical()
                    .max_height(ui.available_height().max(100.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(info).monospace())
                                .wrap()
                                .selectable(true),
                        );
                    });
            } else if !self.pkg_info_loading {
                ui.label(
                    RichText::new(tr(
                        lang,
                        "Нажмите «Показать», чтобы прочитать dumpsys package.",
                        "Press “Show” to read dumpsys package.",
                    ))
                    .color(pal.muted),
                );
            }
        }
    }

    fn info_tab(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        let pal = self.palette();
        let lang = self.lang;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(lang, "Техническая информация", "Technical information")).strong(),
            );
            if self.tech_loading {
                ui.spinner();
            } else if ui.button(tr(lang, "Загрузить", "Load")).clicked() {
                self.load_tech_info();
            }
            if let Some(info) = &self.tech_info {
                if ui.button(tr(lang, "Копировать", "Copy")).clicked() {
                    ui.ctx().copy_text(info.clone());
                    self.status = tr(lang, "Скопировано", "Copied").into();
                    self.status_error = false;
                }
            }
            ui.separator();
            if self.ping_busy {
                ui.spinner();
            } else if ui.button(tr(lang, "Пинг", "Ping")).clicked() {
                self.run_ping();
            }
            if let Some(ms) = self.ping_adb {
                ui.label(RichText::new(format!("ADB: {ms:.1} ms")).color(pal.muted));
            }
            if let Some(ms) = self.ping_net {
                ui.label(RichText::new(format!("Internet: {ms:.1} ms")).color(pal.muted));
            }
        });
        ui.separator();

        if let Some(info) = &self.tech_info {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(info).monospace())
                            .wrap()
                            .selectable(true),
                    );
                });
        } else {
            ui.label(
                RichText::new(tr(
                    lang,
                    "Нажмите «Загрузить»: getprop, ip addr, df, uname, battery, display, cpuinfo.",
                    "Press “Load”: getprop, ip addr, df, uname, battery, display, cpuinfo.",
                ))
                .color(pal.muted),
            );
        }
    }

    fn screen(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        let pal = self.palette();
        let lang = self.lang;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(tr(lang, "Обновить экран", "Refresh screen"))
                .clicked()
            {
                self.request_screenshot();
            }
            ui.checkbox(&mut self.screen_auto, tr(lang, "Трансляция", "Streaming"));
            if self.screen_auto {
                ui.label(RichText::new(tr(lang, "каждые", "every")).color(pal.muted));
                ui.add(
                    egui::DragValue::new(&mut self.screen_interval_ms)
                        .range(150.0..=3000.0)
                        .speed(10.0)
                        .suffix(" ms"),
                );
            }
            ui.checkbox(
                &mut self.screen_tap,
                tr(lang, "Управление касанием", "Tap control"),
            );
            if self.screen_pending {
                ui.spinner();
                ui.label(tr(lang, "снимаю…", "capturing…"));
            }
            if ui.button(tr(lang, "Сохранить PNG", "Save PNG")).clicked() {
                self.save_screenshot();
            }
            ui.separator();
            if ui.button(tr(lang, "Домой", "Home")).clicked() {
                self.action("input keyevent 3", tr(lang, "Домой", "Home"));
            }
            if ui.button(tr(lang, "Назад", "Back")).clicked() {
                self.action("input keyevent 4", tr(lang, "Назад", "Back"));
            }
            if ui.button(tr(lang, "Недавние", "Recents")).clicked() {
                self.action("input keyevent 187", tr(lang, "Недавние", "Recents"));
            }
        });

        let buttons = self.custom_buttons.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr(lang, "Кнопки:", "Buttons:")).color(pal.muted));
            for btn in &buttons {
                if ui.button(&btn.label).clicked() {
                    self.action(&btn.command, &btn.label);
                }
            }
        });
        ui.collapsing(
            tr(lang, "Редактор кнопок", "Button editor"),
            |ui| {
                let mut remove = None;
                for (i, btn) in self.custom_buttons.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut btn.label)
                                .desired_width(120.0)
                                .hint_text(tr(lang, "Название", "Label")),
                        );
                        ui.add(
                            TextEdit::singleline(&mut btn.command)
                                .desired_width(320.0)
                                .hint_text("input keyevent 85"),
                        );
                        if ui.button("✕").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.custom_buttons.remove(i);
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(tr(lang, "Добавить кнопку", "Add button"))
                        .clicked()
                    {
                        self.custom_buttons.push(CustomButton {
                            label: tr(lang, "Новая кнопка", "New button").to_string(),
                            command: "input keyevent 3".to_string(),
                        });
                    }
                    if ui
                        .button(tr(lang, "Сбросить пресеты", "Reset presets"))
                        .clicked()
                    {
                        self.custom_buttons = default_buttons();
                    }
                });
            },
        );
        ui.separator();

        if let Some(tex) = &self.screen_texture {
            let avail = ui.available_size();
            let tex_id = tex.id();
            let tex_size = tex.size_vec2();
            let screen_w = self.screen_w;
            let screen_h = self.screen_h;
            if tex_size.x > 0.0 && tex_size.y > 0.0 {
                let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y).min(1.0);
                let display = tex_size * scale;
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(4.0);
                    let resp = ui.add(
                        egui::Image::new((tex_id, display))
                            .bg_fill(Color32::BLACK)
                            .sense(egui::Sense::click()),
                    );
                    if self.screen_tap && resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let r = resp.rect;
                            let rel_x = ((pos.x - r.left()) / r.width()).clamp(0.0, 1.0);
                            let rel_y = ((pos.y - r.top()) / r.height()).clamp(0.0, 1.0);
                            let dx = (rel_x * screen_w as f32).round() as i64;
                            let dy = (rel_y * screen_h as f32).round() as i64;
                            self.action(
                                &format!("input tap {dx} {dy}"),
                                tr(lang, "Касание", "Tap"),
                            );
                        }
                    }
                    ui.label(RichText::new(format!("{screen_w}×{screen_h}")).color(pal.muted));
                });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    RichText::new(tr(self.lang, "Экран ещё не снят", "No screenshot yet"))
                        .size(18.0)
                        .strong(),
                );
                ui.label(tr(
                    self.lang,
                    "Нажмите «Обновить экран» — появится копия экрана телефона.",
                    "Press “Refresh screen” to get a copy of the phone screen.",
                ));
            });
        }
    }

    fn console(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr(self.lang, "Быстрые команды:", "Quick commands:"))
                    .color(pal.muted),
            );
            for (ru, en, cmd) in PRESETS {
                if ui.button(tr(self.lang, ru, en)).clicked() {
                    self.command = (*cmd).to_string();
                    self.run_command();
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new("adb shell").monospace().color(pal.accent));
            let width = (ui.available_width() - 150.0).max(120.0);
            let resp = ui.add(
                TextEdit::singleline(&mut self.command)
                    .hint_text("dumpsys battery")
                    .desired_width(width)
                    .font(egui::TextStyle::Monospace),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui.button(tr(self.lang, "Выполнить", "Run")).clicked();
            if (enter || clicked) && !self.console_busy {
                self.run_command();
            }
            if self.console_busy {
                ui.spinner();
            }
        });

        ui.horizontal(|ui| {
            if ui
                .button(tr(
                    self.lang,
                    "Копировать последний вывод",
                    "Copy last output",
                ))
                .clicked()
            {
                if let Some(e) = self.console_log.last() {
                    ui.ctx().copy_text(e.output.clone());
                    self.status = tr(self.lang, "Вывод скопирован", "Output copied").into();
                    self.status_error = false;
                }
            }
            if ui.button(tr(self.lang, "Очистить", "Clear")).clicked() {
                self.console_log.clear();
            }
            ui.label(
                RichText::new(tr(self.lang, "Enter — выполнить", "Enter — run")).color(pal.muted),
            );
        });

        ui.separator();
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.console_log.is_empty() {
                    ui.label(
                        RichText::new(tr(
                            self.lang,
                            "Здесь появится вывод команд.",
                            "Command output will appear here.",
                        ))
                        .color(pal.muted),
                    );
                }
                for e in &self.console_log {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.time).monospace().color(pal.muted));
                        ui.label(
                            RichText::new(&e.title)
                                .monospace()
                                .strong()
                                .color(if e.success { pal.good } else { pal.bad }),
                        );
                    });
                    if !e.output.trim().is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(&e.output).monospace())
                                .wrap()
                                .selectable(true),
                        );
                    }
                    ui.separator();
                }
            });
    }

    fn github(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        ui.label(
            RichText::new(tr(self.lang, "Публикация на GitHub", "Publish to GitHub"))
                .size(19.0)
                .strong(),
        );
        ui.label(tr(
            self.lang,
            "Кнопка создаст git-коммит в папке проекта и отправит репозиторий на GitHub через GitHub CLI (gh).",
            "The button creates a git commit in the project folder and pushes the repository to GitHub via GitHub CLI (gh).",
        ));
        ui.add_space(8.0);

        egui::Grid::new("github_form")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr(self.lang, "Имя репозитория", "Repository name"));
                ui.add(
                    TextEdit::singleline(&mut self.repo_name)
                        .desired_width(260.0)
                        .hint_text("adb-wizard"),
                );
                ui.end_row();

                ui.label(tr(self.lang, "Описание", "Description"));
                ui.add(
                    TextEdit::singleline(&mut self.repo_desc)
                        .desired_width(420.0)
                        .hint_text(tr(
                            self.lang,
                            "краткое описание проекта",
                            "short project description",
                        )),
                );
                ui.end_row();

                ui.label(tr(self.lang, "Видимость", "Visibility"));
                ui.checkbox(
                    &mut self.repo_private,
                    tr(self.lang, "приватный репозиторий", "private repository"),
                );
                ui.end_row();

                ui.label(tr(self.lang, "Токен GitHub", "GitHub token"));
                ui.add(
                    TextEdit::singleline(&mut self.repo_token)
                        .password(true)
                        .desired_width(420.0)
                        .hint_text(tr(
                            self.lang,
                            "необязательно: ghp_… с правами repo, не сохраняется",
                            "optional: ghp_… with repo scope, not saved",
                        )),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.publish_busy {
                ui.spinner();
                ui.label(tr(self.lang, "Публикую…", "Publishing…"));
            } else if ui
                .add(
                    egui::Button::new(
                        RichText::new(tr(self.lang, "Опубликовать на GitHub", "Publish to GitHub"))
                            .strong()
                            .color(pal.on_accent),
                    )
                    .fill(pal.accent),
                )
                .clicked()
            {
                self.start_publish();
            }
            if ui
                .button(tr(self.lang, "Проверить инструменты", "Check tools"))
                .clicked()
            {
                self.tools = publish::tools();
            }
            if ui
                .button(tr(
                    self.lang,
                    "Открыть github.com/new",
                    "Open github.com/new",
                ))
                .clicked()
            {
                let _ = webbrowser::open("https://github.com/new");
            }
            if ui
                .button(tr(self.lang, "Создать токен", "Create token"))
                .clicked()
            {
                let _ = webbrowser::open(
                    "https://github.com/settings/tokens/new?scopes=repo&description=ADB%20Wizard",
                );
            }
            if let Some(url) = &self.published_url {
                if ui
                    .button(tr(self.lang, "Открыть репозиторий", "Open repository"))
                    .clicked()
                {
                    let _ = webbrowser::open(url);
                }
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            match &self.tools.git {
                Some(v) => ui.label(RichText::new(format!("git: {v}")).color(pal.good)),
                None => ui.label(
                    RichText::new(tr(self.lang, "git: не найден", "git: not found")).color(pal.bad),
                ),
            };
            ui.separator();
            match &self.tools.gh {
                Some(v) => ui.label(RichText::new(format!("gh: {v}")).color(pal.good)),
                None => ui.label(
                    RichText::new(tr(self.lang, "gh: не найден", "gh: not found")).color(pal.bad),
                ),
            };
        });

        if self.tools.gh.is_none() {
            ui.add_space(6.0);
            egui::Frame::group(ui.style())
                .inner_margin(10.0)
                .fill(pal.warn_bg)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(tr(
                            self.lang,
                            "GitHub CLI не установлен — автоматическая отправка недоступна.",
                            "GitHub CLI is not installed — automatic publishing is unavailable.",
                        ))
                        .color(pal.warn)
                        .strong(),
                    );
                    ui.label(tr(
                        self.lang,
                        "1. Скачайте и установите gh: https://cli.github.com/",
                        "1. Download and install gh: https://cli.github.com/",
                    ));
                    ui.label(tr(
                        self.lang,
                        "2. В терминале выполните: gh auth login",
                        "2. Run in a terminal: gh auth login",
                    ));
                    ui.label(tr(
                        self.lang,
                        "3. Вернитесь сюда и нажмите «Проверить инструменты», затем «Опубликовать».",
                        "3. Come back, press “Check tools”, then “Publish”.",
                    ));
                    ui.label(tr(
                        self.lang,
                        "Либо вставьте Personal Access Token в поле «Токен» (права repo).",
                        "Or paste a Personal Access Token into the “Token” field (repo scope).",
                    ));
                    ui.label(
                        RichText::new(tr(
                            self.lang,
                            "Локальный git-коммит кнопка сделает и без gh — проект будет готов к push.",
                            "The button still makes a local git commit without gh — the project will be ready to push.",
                        ))
                        .color(pal.muted),
                    );
                    if ui
                        .button(tr(self.lang, "Скачать GitHub CLI", "Download GitHub CLI"))
                        .clicked()
                    {
                        let _ = webbrowser::open("https://cli.github.com/");
                    }
                });
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new(tr(self.lang, "Журнал", "Log")).strong());
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.publish_log.is_empty() {
                    ui.label(
                        RichText::new(tr(
                            self.lang,
                            "Журнал публикации пуст.",
                            "Publish log is empty.",
                        ))
                        .color(pal.muted),
                    );
                }
                for line in &self.publish_log {
                    ui.monospace(line);
                }
            });
    }

    fn reboot_window(&mut self, ctx: &egui::Context) {
        if !self.confirm_reboot {
            return;
        }
        let pal = self.palette();
        egui::Window::new(tr(self.lang, "Перезагрузить устройство?", "Reboot device?"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(tr(
                    self.lang,
                    "Устройство перезагрузится. Несохранённые данные могут пропасть.",
                    "The device will reboot. Unsaved data may be lost.",
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr(self.lang, "Отмена", "Cancel")).clicked() {
                        self.confirm_reboot = false;
                    }
                    if ui
                        .button(
                            RichText::new(tr(self.lang, "Перезагрузить", "Reboot")).color(pal.bad),
                        )
                        .clicked()
                    {
                        self.confirm_reboot = false;
                        self.action("reboot", tr(self.lang, "Перезагрузка", "Reboot"));
                    }
                });
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_monitor();
        self.poll_tasks(ctx);
        self.auto_screenshot(ctx);

        if self.monitor.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.screen_pending {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        self.top_bar(ctx);
        self.bottom_bar(ctx);
        self.tabs(ctx);
        self.reboot_window(ctx);
    }
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let mut out: String = name.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn push_history(hist: &mut VecDeque<f32>, value: f32) {
    hist.push_back(value);
    while hist.len() > 120 {
        hist.pop_front();
    }
}

fn load_color(pct: f32, pal: &Palette) -> Color32 {
    if pct < 60.0 {
        pal.good
    } else if pct < 85.0 {
        pal.warn
    } else {
        pal.bad
    }
}

fn battery_color(pct: f32, pal: &Palette) -> Color32 {
    if pct <= 15.0 {
        pal.bad
    } else if pct <= 35.0 {
        pal.warn
    } else {
        pal.good
    }
}

fn metric_card(
    ui: &mut egui::Ui,
    pal: &Palette,
    title: &str,
    value: &str,
    sub: &str,
    fraction: Option<f32>,
    color: Color32,
) {
    egui::Frame::new()
        .fill(pal.window)
        .stroke(Stroke::new(1.0_f32, pal.stroke))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.label(RichText::new(title).small().color(pal.muted));
            ui.label(RichText::new(value).size(25.0).strong());
            if let Some(f) = fraction {
                ui.add(
                    egui::ProgressBar::new(f.clamp(0.0, 1.0))
                        .fill(color)
                        .desired_height(7.0),
                );
            }
            ui.label(RichText::new(sub).small().color(pal.muted));
        });
}

fn sparkline(
    ui: &mut egui::Ui,
    pal: &Palette,
    lang: Lang,
    title: &str,
    data: &VecDeque<f32>,
    min: f32,
    max: f32,
    color: Color32,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        if let Some(v) = data.back() {
            ui.label(RichText::new(format!("{v:.0}{unit}")).color(color).strong());
        } else {
            ui.label(RichText::new(tr(lang, "н/д", "n/a")).color(pal.muted));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{} {}", data.len(), tr(lang, "точек", "points")))
                    .small()
                    .color(pal.muted),
            );
        });
    });

    let desired = Vec2::new(ui.available_width(), 58.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(4), pal.extreme);

    let grid = pal.stroke;
    for f in [0.25_f32, 0.5, 0.75] {
        let y = rect.bottom() - rect.height() * f;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0_f32, grid),
        );
    }

    if data.len() < 2 {
        return;
    }

    let n = data.len();
    let points: Vec<Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = rect.left() + rect.width() * (i as f32 / (n - 1) as f32);
            let t = ((v - min) / (max - min)).clamp(0.0, 1.0);
            let y = rect.bottom() - rect.height() * t;
            Pos2::new(x, y)
        })
        .collect();

    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(2.0_f32, color),
    ));
    if let Some(last) = points.last() {
        painter.circle_filled(*last, 5.0, color.gamma_multiply(0.25));
        painter.circle_filled(*last, 2.5, color);
    }
}

fn process_list(
    ui: &mut egui::Ui,
    title: &str,
    list: &[ProcStat],
    unit: &str,
    color: Color32,
    pal: &Palette,
    lang: Lang,
) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.label(RichText::new(title).strong());
    });
    if list.is_empty() {
        ui.label(
            RichText::new(tr(
                lang,
                "Нет данных. Включите мониторинг или нажмите «Обновить сейчас».",
                "No data. Start monitoring or press “Refresh now”.",
            ))
            .color(pal.muted),
        );
        return;
    }

    let max = list
        .iter()
        .map(|p| p.value)
        .fold(0.0_f32, f32::max)
        .max(0.01);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 8.0).max(120.0))
        .show(ui, |ui| {
            for (i, p) in list.iter().enumerate() {
                if i > 0 {
                    ui.add_space(1.0);
                }
                ui.horizontal(|ui| {
                    let name = truncate_name(&p.name, 30);
                    ui.label(RichText::new(format!("{name:<30}")).monospace());
                    let value = if unit == "%" {
                        format!("{:.1}%", p.value)
                    } else {
                        format!("{:.1} {unit}", p.value)
                    };
                    ui.label(RichText::new(format!("{value:>8}")).monospace().strong());
                    ui.add(
                        egui::ProgressBar::new((p.value / max).clamp(0.0, 1.0))
                            .fill(color)
                            .desired_width(60.0),
                    );
                });
            }
        });
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    #[cfg(windows)]
    {
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, "segoe_ui".to_owned());
            }
        }
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") {
            fonts.font_data.insert(
                "consolas".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.insert(0, "consolas".to_owned());
            }
        }
    }

    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context, pal: &Palette) {
    let mut visuals = if pal.is_light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };

    visuals.panel_fill = pal.panel;
    visuals.window_fill = pal.window;
    visuals.extreme_bg_color = pal.extreme;
    visuals.faint_bg_color = pal.faint;
    visuals.code_bg_color = pal.extreme;
    visuals.override_text_color = Some(pal.text);
    visuals.hyperlink_color = pal.accent;
    visuals.warn_fg_color = pal.warn;
    visuals.error_fg_color = pal.bad;
    visuals.window_stroke = Stroke::new(1.0_f32, pal.stroke);
    visuals.selection.bg_fill = pal.accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0_f32, pal.accent);

    visuals.widgets.noninteractive.bg_fill = pal.panel;
    visuals.widgets.noninteractive.weak_bg_fill = pal.panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.stroke);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text);

    visuals.widgets.inactive.bg_fill = pal.widget;
    visuals.widgets.inactive.weak_bg_fill = pal.widget;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.stroke);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal.text);

    visuals.widgets.hovered.bg_fill = pal.widget_hover;
    visuals.widgets.hovered.weak_bg_fill = pal.widget_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, pal.text);

    visuals.widgets.active.bg_fill = pal.accent.gamma_multiply(0.45);
    visuals.widgets.active.weak_bg_fill = pal.accent.gamma_multiply(0.45);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, pal.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, pal.text);

    visuals.widgets.open = visuals.widgets.hovered;

    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);

    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            FontId::new(13.0, egui::FontFamily::Monospace),
        );
    });
}
