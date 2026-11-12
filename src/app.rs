use crate::adb::{self, Monitor, ScreenImage, Snapshot};
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
    Screen,
    Console,
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
}

struct ConsoleEntry {
    time: String,
    title: String,
    output: String,
    success: bool,
}

const GREEN: Color32 = Color32::from_rgb(102, 187, 106);
const YELLOW: Color32 = Color32::from_rgb(255, 202, 40);
const RED: Color32 = Color32::from_rgb(239, 83, 80);
const BLUE: Color32 = Color32::from_rgb(66, 165, 245);
const ORANGE: Color32 = Color32::from_rgb(255, 152, 0);
const MUTED: Color32 = Color32::from_rgb(150, 158, 170);

const PRESETS: &[(&str, &str)] = &[
    ("Батарея", "dumpsys battery"),
    ("Память", "dumpsys meminfo | head -n 60"),
    ("Пакеты", "pm list packages -3"),
    ("Логи", "logcat -d -t 200"),
    ("Свойства", "getprop | head -n 80"),
    ("Диск", "df -h"),
    ("Топ CPU", "top -b -n 1 -m 15"),
    ("Яркость", "settings get system screen_brightness"),
    ("Экран", "dumpsys display | head -n 40"),
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
    ram_hist: VecDeque<f32>,
    bat_hist: VecDeque<f32>,
    last_net: Option<(u64, u64, Instant)>,
    net_rx_rate: Option<f32>,
    net_tx_rate: Option<f32>,

    tab: Tab,
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
    last_screenshot: Instant,

    tools: Tools,
    repo_name: String,
    repo_desc: String,
    repo_private: bool,
    publish_log: Vec<String>,
    publish_busy: bool,
    published_url: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);

        let (tx, rx) = channel();
        let mut app = Self {
            devices: Vec::new(),
            selected: None,
            info: None,
            snapshot: None,
            monitor: None,
            interval: 2.0,
            status: "Поиск устройств…".to_string(),
            status_error: false,
            cpu_hist: VecDeque::new(),
            ram_hist: VecDeque::new(),
            bat_hist: VecDeque::new(),
            last_net: None,
            net_rx_rate: None,
            net_tx_rate: None,
            tab: Tab::Overview,
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
            screen_auto: false,
            last_screenshot: Instant::now(),
            tools: publish::tools(),
            repo_name: "adb-wizard".to_string(),
            repo_desc: "Монитор Android-устройства через ADB: нагрузка, процессы, экран, консоль".to_string(),
            repo_private: false,
            publish_log: Vec::new(),
            publish_busy: false,
            published_url: None,
        };
        app.refresh_devices();
        app
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
                            "Устройства не найдены. Подключите телефон по USB и включите отладку.".into()
                        } else {
                            "Устройство найдено, но не готово (offline/unauthorized).".into()
                        };
                        self.status_error = true;
                    }
                } else {
                    self.status = format!("Найдено устройств: {}", self.devices.len());
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
        self.ram_hist.clear();
        self.bat_hist.clear();
        self.last_net = None;
        self.net_rx_rate = None;
        self.net_tx_rate = None;
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
            self.status = format!("Мониторинг запущен, интервал {:.1} с", self.interval);
            self.status_error = false;
        }
    }

    fn stop_monitor(&mut self) {
        self.monitor = None;
        self.status = "Мониторинг остановлен".to_string();
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
        self.status = "Читаю состояние…".to_string();
    }

    fn connect_device(&mut self) {
        let addr = self.connect_addr.trim().to_string();
        if addr.is_empty() {
            self.status = "Введите адрес, например 192.168.1.50:5555".into();
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
            self.status = "Сначала подключите устройство".into();
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
            self.status = "Сначала подключите устройство".into();
            self.status_error = true;
            return;
        };
        self.screen_pending = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(TaskMsg::Screenshot(adb::screencap(&serial)));
        });
    }

    fn save_screenshot(&mut self) {
        let Some(png) = &self.screen_png else {
            self.status = "Сначала снимите экран".into();
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
                    self.status = format!("Скриншот сохранён: {}", path.display());
                    self.status_error = false;
                }
                Err(e) => {
                    self.status = format!("Не удалось сохранить: {e}");
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
            publish::publish(&repo, &desc, private, &publish_tx);
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
            self.status = format!("Ошибка: {e}");
            self.status_error = true;
            return;
        }

        snap.cores = self.info.as_ref().map(|i| i.cores).unwrap_or(0);

        if let Some(v) = snap.cpu_percent {
            push_history(&mut self.cpu_hist, v.clamp(0.0, 100.0));
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

        self.status = format!("Обновлено в {}", snap.time);
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
                    self.status = format!("Не удалось получить сведения: {e}");
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
                    self.screen_texture = Some(ctx.load_texture(
                        "device_screen",
                        color,
                        TextureOptions::LINEAR,
                    ));
                    self.screen_w = img.width;
                    self.screen_h = img.height;
                    self.screen_png = Some(img.png);
                    self.screen_pending = false;
                    self.last_screenshot = Instant::now();
                    self.status = format!("Скриншот {}×{}", img.width, img.height);
                    self.status_error = false;
                }
                TaskMsg::Screenshot(Err(e)) => {
                    self.screen_pending = false;
                    self.status = format!("Скриншот: {e}");
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
                            self.publish_log
                                .push(format!("Публикация завершена: {url}"));
                            self.status = format!("Опубликовано: {url}");
                            self.status_error = false;
                        }
                        Err(e) => {
                            self.publish_log.push(format!("Ошибка: {e}"));
                            self.status = format!("GitHub: {e}");
                            self.status_error = true;
                        }
                    }
                }
            }
        }
    }

    fn auto_screenshot(&mut self, ctx: &egui::Context) {
        if self.tab != Tab::Screen || !self.screen_auto {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(200));
        if !self.screen_pending && self.last_screenshot.elapsed() >= Duration::from_millis(1200) {
            self.request_screenshot();
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("ADB Wizard").size(20.0).strong());
                ui.label(RichText::new("монитор устройства").color(MUTED));
                ui.separator();

                let current = self
                    .selected
                    .clone()
                    .unwrap_or_else(|| "— нет устройства —".to_string());
                let mut picked: Option<String> = None;
                egui::ComboBox::from_id_salt("device_combo")
                    .selected_text(current)
                    .width(340.0)
                    .show_ui(ui, |ui| {
                        if self.devices.is_empty() {
                            ui.label("Список пуст");
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

                if ui.button("Обновить список").clicked() {
                    self.refresh_devices();
                }

                ui.separator();

                if self.monitor.is_some() {
                    if ui.button("Стоп").clicked() {
                        self.stop_monitor();
                    }
                } else if ui.button("Мониторинг").clicked() {
                    self.start_monitor();
                }
                if ui.button("Снимок").clicked() {
                    self.refresh_once();
                }

                ui.label("интервал");
                let resp = ui.add(
                    egui::DragValue::new(&mut self.interval)
                        .range(0.5..=10.0)
                        .speed(0.1)
                        .suffix(" с"),
                );
                if resp.changed() && self.monitor.is_some() {
                    self.start_monitor();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Wi-Fi:");
                ui.add(
                    TextEdit::singleline(&mut self.connect_addr)
                        .hint_text("192.168.1.50:5555")
                        .desired_width(170.0),
                );
                if ui.button("Подключить").clicked() {
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
                            "Android {} · {} · {} ядер",
                            info.android, info.resolution, info.cores
                        ))
                        .color(MUTED),
                    );
                } else if self.selected.is_some() {
                    ui.label(RichText::new("сведения об устройстве загружаются…").color(MUTED));
                } else {
                    ui.label(RichText::new("устройство не выбрано").color(MUTED));
                }
            });
            ui.add_space(4.0);
        });
    }

    fn bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let color = if self.status_error { RED } else { MUTED };
                ui.label(RichText::new(&self.status).color(color));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("ADB Wizard 0.1").color(MUTED));
                    if self.monitor.is_some() {
                        ui.spinner();
                        ui.label(RichText::new("live").color(GREEN));
                    }
                    ui.label(RichText::new(format!("устройств: {}", self.devices.len())).color(MUTED));
                });
            });
        });
    }

    fn tabs(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Overview, "Обзор");
                ui.selectable_value(&mut self.tab, Tab::Processes, "Процессы");
                ui.selectable_value(&mut self.tab, Tab::Screen, "Экран");
                ui.selectable_value(&mut self.tab, Tab::Console, "Консоль");
                ui.selectable_value(&mut self.tab, Tab::GitHub, "GitHub");
            });
            ui.separator();
            match self.tab {
                Tab::Overview => self.overview(ui),
                Tab::Processes => self.processes(ui),
                Tab::Screen => self.screen(ui),
                Tab::Console => self.console(ui),
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
                ui.label(RichText::new(format!("SDK {}", info.sdk)).color(MUTED));
                ui.label(RichText::new(&info.abi).color(MUTED));
                ui.separator();
                ui.label(format!("{} · {}", info.resolution, info.density));
                if let Some(s) = &snap {
                    if let Some(up) = s.uptime_secs {
                        ui.label(RichText::new(format!("uptime {}", human_uptime(up))).color(MUTED));
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
                    "CPU",
                    &format!("{cpu:.0}%"),
                    &format!("{cores} ядер"),
                    Some(cpu / 100.0),
                    load_color(cpu),
                );
                metric_card(
                    &mut cols[1],
                    "Память",
                    &format!("{ram_pct:.0}%"),
                    &format!(
                        "{} из {}",
                        human_kb(s.ram_used_kb.unwrap_or(0)),
                        human_kb(s.ram_total_kb.unwrap_or(0))
                    ),
                    Some(ram_pct / 100.0),
                    load_color(ram_pct),
                );
                let bat_sub = match (s.battery.temp_c, s.battery.status.as_str()) {
                    (Some(t), st) if !st.is_empty() => format!("{t:.0} °C · {st}"),
                    (Some(t), _) => format!("{t:.0} °C"),
                    _ => "нет данных".to_string(),
                };
                metric_card(
                    &mut cols[2],
                    "Батарея",
                    &format!("{battery:.0}%"),
                    &bat_sub,
                    Some(battery / 100.0),
                    battery_color(battery),
                );
                metric_card(
                    &mut cols[3],
                    "Накопитель /data",
                    &format!("{storage_pct:.0}%"),
                    &format!(
                        "{} из {}",
                        human_kb(s.storage_used_kb.unwrap_or(0)),
                        human_kb(s.storage_total_kb.unwrap_or(0))
                    ),
                    Some(storage_pct / 100.0),
                    load_color(storage_pct),
                );
            });

            ui.add_space(8.0);
            let battery_line_color = self
                .bat_hist
                .back()
                .map(|v| battery_color(*v))
                .unwrap_or(GREEN);
            ui.columns(3, |cols| {
                sparkline(
                    &mut cols[0],
                    "Загрузка CPU",
                    &self.cpu_hist,
                    0.0,
                    100.0,
                    ORANGE,
                    "%",
                );
                sparkline(
                    &mut cols[1],
                    "Занятая память",
                    &self.ram_hist,
                    0.0,
                    100.0,
                    BLUE,
                    "%",
                );
                sparkline(
                    &mut cols[2],
                    "Батарея",
                    &self.bat_hist,
                    0.0,
                    100.0,
                    battery_line_color,
                    "%",
                );
            });

            ui.horizontal_wrapped(|ui| {
                if let Some(rx) = self.net_rx_rate {
                    ui.label(
                        RichText::new(format!("Сеть ↓ {}/с", human_bytes(rx as u64)))
                            .color(MUTED),
                    );
                }
                if let Some(tx) = self.net_tx_rate {
                    ui.label(
                        RichText::new(format!("↑ {}/с", human_bytes(tx as u64))).color(MUTED),
                    );
                }
                if let Some(rx) = s.net_rx_bytes {
                    ui.label(
                        RichText::new(format!(
                            "всего ↓ {}",
                            human_bytes(rx)
                        ))
                        .color(MUTED),
                    );
                }
                if let Some(tx) = s.net_tx_bytes {
                    ui.label(RichText::new(format!("↑ {}", human_bytes(tx))).color(MUTED));
                }
            });
        } else {
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Получаю первый снимок состояния…");
            });
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new("Быстрые действия").strong());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Домой").clicked() {
                self.action("input keyevent 3", "Домой");
            }
            if ui.button("Назад").clicked() {
                self.action("input keyevent 4", "Назад");
            }
            if ui.button("Недавние").clicked() {
                self.action("input keyevent 187", "Недавние");
            }
            if ui.button("Экран вкл/выкл").clicked() {
                self.action("input keyevent 26", "Питание");
            }
            if ui.button("Разбудить").clicked() {
                self.action("input keyevent 224", "Разбудить");
            }
            if ui.button("Громкость +").clicked() {
                self.action("input keyevent 24", "Громкость +");
            }
            if ui.button("Громкость −").clicked() {
                self.action("input keyevent 25", "Громкость −");
            }
            if ui.button("Скриншот").clicked() {
                self.tab = Tab::Screen;
                self.request_screenshot();
            }
            if ui.button(RichText::new("Перезагрузить").color(RED)).clicked() {
                self.confirm_reboot = true;
            }
        });

        ui.add_space(6.0);
        ui.label(RichText::new("Подсказка: вкладка «Процессы» показывает, что именно ест CPU и память.").color(MUTED));
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.label(RichText::new("Устройство не подключено").size(24.0).strong());
            ui.add_space(8.0);
            ui.label("1. На телефоне включите «Для разработчиков» → «Отладка по USB».");
            ui.label("2. Подключите кабель и подтвердите запрос на компьютере.");
            ui.label("3. Нажмите «Обновить список».");
            ui.add_space(12.0);
            if ui.button("Обновить список").clicked() {
                self.refresh_devices();
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new("Или подключитесь по Wi-Fi: adb tcpip 5555, затем адрес в верхней панели.")
                    .color(MUTED),
            );
        });
    }

    fn processes(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        let (cpu, mem) = match &self.snapshot {
            Some(s) => (s.top_cpu.clone(), s.top_mem.clone()),
            None => (Vec::new(), Vec::new()),
        };

        ui.horizontal(|ui| {
            ui.label(RichText::new("Что потребляет ресурсы").strong());
            if ui.button("Обновить сейчас").clicked() {
                self.refresh_once();
            }
            ui.label(
                RichText::new("данные обновляются вместе с мониторингом")
                    .color(MUTED),
            );
        });
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            process_list(&mut cols[0], "Топ по CPU", &cpu, "%", ORANGE);
            process_list(&mut cols[1], "Топ по памяти (PSS)", &mem, "МБ", BLUE);
        });
    }

    fn screen(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            self.empty_state(ui);
            return;
        }

        ui.horizontal(|ui| {
            if ui.button("Обновить экран").clicked() {
                self.request_screenshot();
            }
            ui.checkbox(&mut self.screen_auto, "Автообновление");
            if self.screen_pending {
                ui.spinner();
                ui.label("снимаю…");
            }
            if ui.button("Сохранить PNG").clicked() {
                self.save_screenshot();
            }
            ui.separator();
            if ui.button("Домой").clicked() {
                self.action("input keyevent 3", "Домой");
            }
            if ui.button("Назад").clicked() {
                self.action("input keyevent 4", "Назад");
            }
            if ui.button("Недавние").clicked() {
                self.action("input keyevent 187", "Недавние");
            }
        });
        ui.separator();

        if let Some(tex) = &self.screen_texture {
            let avail = ui.available_size();
            let tex_size = tex.size_vec2();
            if tex_size.x > 0.0 && tex_size.y > 0.0 {
                let scale = (avail.x / tex_size.x)
                    .min(avail.y / tex_size.y)
                    .min(1.0);
                let display = tex_size * scale;
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.add(egui::Image::new((tex.id(), display)).bg_fill(Color32::BLACK));
                    ui.label(
                        RichText::new(format!("{}×{}", self.screen_w, self.screen_h))
                            .color(MUTED),
                    );
                });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new("Экран ещё не снят").size(18.0).strong());
                ui.label("Нажмите «Обновить экран» — появится копия экрана телефона.");
            });
        }
    }

    fn console(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Быстрые команды:").color(MUTED));
            for (label, cmd) in PRESETS {
                if ui.button(*label).clicked() {
                    self.command = (*cmd).to_string();
                    self.run_command();
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new("adb shell").monospace().color(GREEN));
            let width = (ui.available_width() - 150.0).max(120.0);
            let resp = ui.add(
                TextEdit::singleline(&mut self.command)
                    .hint_text("dumpsys battery")
                    .desired_width(width)
                    .font(egui::TextStyle::Monospace),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui.button("Выполнить").clicked();
            if (enter || clicked) && !self.console_busy {
                self.run_command();
            }
            if self.console_busy {
                ui.spinner();
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Копировать последний вывод").clicked() {
                if let Some(e) = self.console_log.last() {
                    ui.ctx().copy_text(e.output.clone());
                    self.status = "Вывод скопирован".into();
                    self.status_error = false;
                }
            }
            if ui.button("Очистить").clicked() {
                self.console_log.clear();
            }
            ui.label(RichText::new("Enter — выполнить").color(MUTED));
        });

        ui.separator();
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.console_log.is_empty() {
                    ui.label(RichText::new("Здесь появится вывод команд.").color(MUTED));
                }
                for e in &self.console_log {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.time).monospace().color(MUTED));
                        ui.label(
                            RichText::new(&e.title)
                                .monospace()
                                .strong()
                                .color(if e.success { GREEN } else { RED }),
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
        ui.label(RichText::new("Публикация на GitHub").size(19.0).strong());
        ui.label(
            "Кнопка создаст git-коммит в папке проекта и отправит репозиторий на GitHub через GitHub CLI (gh).",
        );
        ui.add_space(8.0);

        egui::Grid::new("github_form")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Имя репозитория");
                ui.add(
                    TextEdit::singleline(&mut self.repo_name)
                        .desired_width(260.0)
                        .hint_text("adb-wizard"),
                );
                ui.end_row();

                ui.label("Описание");
                ui.add(
                    TextEdit::singleline(&mut self.repo_desc)
                        .desired_width(420.0)
                        .hint_text("краткое описание проекта"),
                );
                ui.end_row();

                ui.label("Видимость");
                ui.checkbox(&mut self.repo_private, "приватный репозиторий");
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.publish_busy {
                ui.spinner();
                ui.label("Публикую…");
            } else if ui.button(RichText::new("Опубликовать на GitHub").strong()).clicked() {
                self.start_publish();
            }
            if ui.button("Проверить инструменты").clicked() {
                self.tools = publish::tools();
            }
            if ui.button("Открыть github.com/new").clicked() {
                let _ = webbrowser::open("https://github.com/new");
            }
            if let Some(url) = &self.published_url {
                if ui.button("Открыть репозиторий").clicked() {
                    let _ = webbrowser::open(url);
                }
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            match &self.tools.git {
                Some(v) => ui.label(RichText::new(format!("git: {v}")).color(GREEN)),
                None => ui.label(RichText::new("git: не найден").color(RED)),
            };
            ui.separator();
            match &self.tools.gh {
                Some(v) => ui.label(RichText::new(format!("gh: {v}")).color(GREEN)),
                None => ui.label(RichText::new("gh: не найден").color(RED)),
            };
        });

        if self.tools.gh.is_none() {
            ui.add_space(6.0);
            egui::Frame::group(ui.style())
                .inner_margin(10.0)
                .fill(Color32::from_rgb(48, 40, 22))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("GitHub CLI не установлен — автоматическая отправка недоступна.")
                            .color(YELLOW)
                            .strong(),
                    );
                    ui.label("1. Скачайте и установите gh: https://cli.github.com/");
                    ui.label("2. В терминале выполните: gh auth login");
                    ui.label("3. Вернитесь сюда и нажмите «Проверить инструменты», затем «Опубликовать».");
                    ui.label(
                        RichText::new(
                            "Локальный git-коммит кнопка сделает и без gh — проект будет готов к push.",
                        )
                        .color(MUTED),
                    );
                    if ui.button("Скачать GitHub CLI").clicked() {
                        let _ = webbrowser::open("https://cli.github.com/");
                    }
                });
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Журнал").strong());
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.publish_log.is_empty() {
                    ui.label(RichText::new("Журнал публикации пуст.").color(MUTED));
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
        egui::Window::new("Перезагрузить устройство?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Устройство перезагрузится. Несохранённые данные могут пропасть.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Отмена").clicked() {
                        self.confirm_reboot = false;
                    }
                    if ui
                        .button(RichText::new("Перезагрузить").color(RED))
                        .clicked()
                    {
                        self.confirm_reboot = false;
                        self.action("reboot", "Перезагрузка");
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

fn load_color(pct: f32) -> Color32 {
    if pct < 60.0 {
        GREEN
    } else if pct < 85.0 {
        YELLOW
    } else {
        RED
    }
}

fn battery_color(pct: f32) -> Color32 {
    if pct <= 15.0 {
        RED
    } else if pct <= 35.0 {
        YELLOW
    } else {
        GREEN
    }
}

fn metric_card(
    ui: &mut egui::Ui,
    title: &str,
    value: &str,
    sub: &str,
    fraction: Option<f32>,
    color: Color32,
) {
    egui::Frame::group(ui.style())
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.label(RichText::new(title).small().color(MUTED));
            ui.label(RichText::new(value).size(25.0).strong());
            if let Some(f) = fraction {
                ui.add(
                    egui::ProgressBar::new(f.clamp(0.0, 1.0))
                        .fill(color)
                        .desired_height(7.0),
                );
            }
            ui.label(RichText::new(sub).small().color(MUTED));
        });
}

fn sparkline(
    ui: &mut egui::Ui,
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
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{} точек", data.len())).small().color(MUTED));
        });
    });

    let desired = Vec2::new(ui.available_width(), 58.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(4), ui.visuals().extreme_bg_color);

    let grid = ui.visuals().widgets.noninteractive.bg_stroke.color;
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

    painter.add(egui::Shape::line(points, Stroke::new(2.0_f32, color)));
}

fn process_list(
    ui: &mut egui::Ui,
    title: &str,
    list: &[ProcStat],
    unit: &str,
    color: Color32,
) {
    ui.label(RichText::new(title).strong());
    if list.is_empty() {
        ui.label(
            RichText::new("Нет данных. Включите мониторинг или нажмите «Обновить сейчас».")
                .color(MUTED),
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
            for p in list {
                ui.horizontal(|ui| {
                    let name = truncate_name(&p.name, 30);
                    ui.label(RichText::new(format!("{name:<30}")).monospace());
                    let value = if unit == "%" {
                        format!("{:.1}%", p.value)
                    } else {
                        format!("{:.1} МБ", p.value)
                    };
                    ui.label(
                        RichText::new(format!("{value:>8}"))
                            .monospace()
                            .strong(),
                    );
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

fn setup_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(18, 20, 24);
    visuals.window_fill = Color32::from_rgb(24, 27, 32);
    visuals.extreme_bg_color = Color32::from_rgb(12, 14, 17);
    visuals.widgets.noninteractive.bg_stroke =
        Stroke::new(1.0_f32, Color32::from_rgb(48, 53, 62));
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
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
