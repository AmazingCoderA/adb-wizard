//! Запуск adb и сбор снимка состояния устройства.

use crate::parsers::{
    parse_battery, parse_cpuinfo, parse_device_info, parse_devices, parse_df, parse_meminfo,
    parse_meminfo_top, parse_net, parse_uptime, split_marked, BatteryInfo, Device, DeviceInfo,
    ProcStat,
};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const SNAPSHOT_SCRIPT: &str = "\
echo __ADBW_MEM__; cat /proc/meminfo; \
echo __ADBW_UPTIME__; cat /proc/uptime; \
echo __ADBW_DF__; df -k /data; \
echo __ADBW_NET__; cat /proc/net/dev; \
echo __ADBW_BAT__; dumpsys battery; \
echo __ADBW_CPU__; dumpsys cpuinfo; \
echo __ADBW_MEMTOP__; dumpsys meminfo | head -n 100; \
echo __ADBW_END__";

const INFO_SCRIPT: &str = "\
echo __ADBW_MANUF__; getprop ro.product.manufacturer; \
echo __ADBW_MODEL__; getprop ro.product.model; \
echo __ADBW_REL__; getprop ro.build.version.release; \
echo __ADBW_SDK__; getprop ro.build.version.sdk; \
echo __ADBW_ABI__; getprop ro.product.cpu.abi; \
echo __ADBW_SIZE__; wm size; \
echo __ADBW_DENS__; wm density; \
echo __ADBW_NPROC__; nproc; \
echo __ADBW_END__";

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub time: String,
    pub battery: BatteryInfo,
    pub cpu_percent: Option<f32>,
    pub cores: usize,
    pub ram_total_kb: Option<u64>,
    pub ram_used_kb: Option<u64>,
    pub storage_used_kb: Option<u64>,
    pub storage_total_kb: Option<u64>,
    pub uptime_secs: Option<u64>,
    pub net_rx_bytes: Option<u64>,
    pub net_tx_bytes: Option<u64>,
    pub top_cpu: Vec<ProcStat>,
    pub top_mem: Vec<ProcStat>,
    pub error: Option<String>,
}

pub struct ScreenImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub png: Vec<u8>,
}

fn adb_command() -> Command {
    let exe = std::env::var("ADB").unwrap_or_else(|_| "adb".to_string());
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn run_adb_raw(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = adb_command()
        .args(args)
        .output()
        .map_err(|e| format!("Не удалось запустить adb: {e}. Установите platform-tools и добавьте adb в PATH."))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("adb завершился с кодом {:?}", out.status.code())
        };
        return Err(msg);
    }
    Ok(out.stdout)
}

pub fn run_adb_text(args: &[&str]) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&run_adb_raw(args)?).to_string())
}

pub fn shell(serial: &str, script: &str) -> Result<String, String> {
    run_adb_text(&["-s", serial, "shell", script])
}

pub fn list_devices() -> Result<Vec<Device>, String> {
    let out = run_adb_text(&["devices", "-l"])?;
    Ok(parse_devices(&out))
}

pub fn connect(address: &str) -> Result<String, String> {
    run_adb_text(&["connect", address])
}

pub fn fetch_info(serial: &str) -> Result<DeviceInfo, String> {
    let out = shell(serial, INFO_SCRIPT)?;
    Ok(parse_device_info(&out))
}

pub fn collect_snapshot(serial: &str) -> Snapshot {
    let mut snap = Snapshot {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        ..Default::default()
    };

    let out = match shell(serial, SNAPSHOT_SCRIPT) {
        Ok(o) => o,
        Err(e) => {
            snap.error = Some(e);
            return snap;
        }
    };

    let m = split_marked(&out);

    if let Some(v) = m.get("MEM") {
        if let Some((total, used)) = parse_meminfo(v) {
            snap.ram_total_kb = Some(total);
            snap.ram_used_kb = Some(used);
        }
    }
    if let Some(v) = m.get("UPTIME") {
        snap.uptime_secs = parse_uptime(v);
    }
    if let Some(v) = m.get("DF") {
        if let Some((used, total)) = parse_df(v) {
            snap.storage_used_kb = Some(used);
            snap.storage_total_kb = Some(total);
        }
    }
    if let Some(v) = m.get("NET") {
        if let Some((rx, tx)) = parse_net(v) {
            snap.net_rx_bytes = Some(rx);
            snap.net_tx_bytes = Some(tx);
        }
    }
    if let Some(v) = m.get("BAT") {
        snap.battery = parse_battery(v);
    }
    if let Some(v) = m.get("CPU") {
        let (total, procs) = parse_cpuinfo(v);
        snap.cpu_percent = total;
        snap.top_cpu = procs;
    }
    if let Some(v) = m.get("MEMTOP") {
        snap.top_mem = parse_meminfo_top(v);
    }

    snap
}

pub fn screencap(serial: &str) -> Result<ScreenImage, String> {
    let png = run_adb_raw(&["-s", serial, "exec-out", "screencap", "-p"])?;
    match decode_png(png.clone()) {
        Ok(img) => Ok(img),
        Err(_) => {
            // Старые устройства иногда портят бинарный поток. Пробуем через файл.
            let _ = shell(serial, "screencap -p /sdcard/adbwizard_screen.png");
            let png = run_adb_raw(&[
                "-s",
                serial,
                "exec-out",
                "cat",
                "/sdcard/adbwizard_screen.png",
            ])?;
            let _ = shell(serial, "rm -f /sdcard/adbwizard_screen.png");
            decode_png(png)
        }
    }
}

fn decode_png(png: Vec<u8>) -> Result<ScreenImage, String> {
    if png.is_empty() {
        return Err("Пустой ответ screencap".to_string());
    }
    let img = image::load_from_memory(&png)
        .map_err(|e| format!("Не удалось декодировать скриншот: {e}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(ScreenImage {
        width: w as usize,
        height: h as usize,
        rgba: img.into_raw(),
        png,
    })
}

pub struct Monitor {
    pub rx: Receiver<Snapshot>,
    stop: Arc<AtomicBool>,
}

impl Monitor {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start_monitor(serial: String, interval: Duration) -> Monitor {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    std::thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            let snap = collect_snapshot(&serial);
            let failed = snap.error.is_some();
            if tx.send(snap).is_err() {
                break;
            }

            let wait = if failed {
                Duration::from_secs(2)
            } else {
                interval
            };
            let step = Duration::from_millis(100);
            let mut slept = Duration::ZERO;
            while slept < wait {
                if stop_thread.load(Ordering::Relaxed) {
                    return;
                }
                let chunk = step.min(wait - slept);
                std::thread::sleep(chunk);
                slept += chunk;
            }
        }
    });

    Monitor { rx, stop }
}
