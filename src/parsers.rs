//! Чистые парсеры вывода adb. Здесь нет запуска процессов, поэтому всё легко тестируется.

use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    pub state: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcStat {
    pub name: String,
    pub pid: Option<u32>,
    /// Для CPU — проценты, для памяти — мегабайты.
    pub value: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatteryInfo {
    pub level: Option<u8>,
    pub temp_c: Option<f32>,
    pub status: String,
    pub voltage_mv: Option<u32>,
    pub health: String,
    pub plugged: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceInfo {
    pub manufacturer: String,
    pub model: String,
    pub android: String,
    pub sdk: String,
    pub abi: String,
    pub resolution: String,
    pub density: String,
    pub cores: usize,
}

/// Разбирает вывод вида:
/// `__ADBW_MEM__` ... `__ADBW_CPU__` ... в map: "MEM" -> содержимое секции.
pub fn split_marked(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current: Option<String> = None;
    let mut buf = String::new();

    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("__ADBW_") && t.ends_with("__") && t.len() > 10 {
            if let Some(key) = current.take() {
                map.insert(key, std::mem::take(&mut buf));
            }
            let key = t
                .trim_matches('_')
                .trim_start_matches("ADBW_")
                .to_string();
            current = Some(key);
        } else if current.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if let Some(key) = current {
        map.insert(key, buf);
    }
    map
}

pub fn parse_devices(s: &str) -> Vec<Device> {
    let mut out = Vec::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with('*')
            || line.starts_with("adb ")
        {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let Some(state) = parts.next() else { continue };
        let mut model = String::new();
        for p in parts {
            if let Some(m) = p.strip_prefix("model:") {
                model = m.to_string();
            }
        }
        out.push(Device {
            serial: serial.to_string(),
            state: state.to_string(),
            model,
        });
    }
    out
}

/// Возвращает (всего КБ, занято КБ).
pub fn parse_meminfo(s: &str) -> Option<(u64, u64)> {
    let mut total = None;
    let mut available = None;
    let mut free = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;

    for line in s.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let kb = v
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse::<u64>()
            .ok();
        match k.trim() {
            "MemTotal" => total = kb,
            "MemAvailable" => available = kb,
            "MemFree" => free = kb.unwrap_or(0),
            "Buffers" => buffers = kb.unwrap_or(0),
            "Cached" => cached = kb.unwrap_or(0),
            _ => {}
        }
    }

    let total = total?;
    let used = match available {
        Some(a) => total.saturating_sub(a),
        None => total.saturating_sub(free + buffers + cached),
    };
    Some((total, used))
}

pub fn parse_uptime(s: &str) -> Option<u64> {
    s.split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .map(|v| v as u64)
}

/// Возвращает (занято КБ, всего КБ).
pub fn parse_df(s: &str) -> Option<(u64, u64)> {
    let line = s.lines().filter(|l| !l.trim().is_empty()).last()?;
    let p: Vec<&str> = line.split_whitespace().collect();
    if p.len() < 3 {
        return None;
    }
    let total = p[1].parse::<u64>().ok()?;
    let used = p[2].parse::<u64>().ok()?;
    Some((used, total))
}

/// Возвращает (принято байт, отправлено байт) по всем интерфейсам, кроме lo.
pub fn parse_net(s: &str) -> Option<(u64, u64)> {
    let mut rx = 0u64;
    let mut tx = 0u64;
    let mut found = false;
    for line in s.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if name.trim() == "lo" || name.contains('|') {
            continue;
        }
        let p: Vec<&str> = rest.split_whitespace().collect();
        if p.len() < 9 {
            continue;
        }
        let (Ok(r), Ok(t)) = (p[0].parse::<u64>(), p[8].parse::<u64>()) else {
            continue;
        };
        rx += r;
        tx += t;
        found = true;
    }
    found.then_some((rx, tx))
}

pub fn parse_battery(s: &str) -> BatteryInfo {
    let mut info = BatteryInfo::default();
    for line in s.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "level" => info.level = v.parse().ok(),
            "temperature" => {
                info.temp_c = v.parse::<f32>().ok().map(|t| t / 10.0);
            }
            "status" => {
                info.status = match v {
                    "1" => "неизвестно",
                    "2" => "заряжается",
                    "3" => "разряжается",
                    "4" => "не заряжается",
                    "5" => "заряжен",
                    "Charging" => "заряжается",
                    "Discharging" => "разряжается",
                    "Full" => "заряжен",
                    "Not charging" => "не заряжается",
                    _ => "—",
                }
                .to_string();
            }
            "voltage" => info.voltage_mv = v.parse().ok(),
            "health" => {
                info.health = match v {
                    "1" => "неизвестно",
                    "2" => "хорошо",
                    "3" => "перегрев",
                    "4" => "мёртвая",
                    "5" => "перегрузка",
                    "6" => "сбой",
                    "7" => "холодная",
                    _ => "—",
                }
                .to_string();
            }
            "AC powered" if v == "true" => info.plugged = "AC".to_string(),
            "USB powered" if v == "true" => info.plugged = "USB".to_string(),
            "Wireless powered" if v == "true" => info.plugged = "беспроводная".to_string(),
            _ => {}
        }
    }
    info
}

/// Возвращает (общая загрузка CPU в %, топ процессов по CPU).
pub fn parse_cpuinfo(s: &str) -> (Option<f32>, Vec<ProcStat>) {
    let mut total = None;
    let mut procs = Vec::new();

    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("CPU usage:") || t.starts_with("CPU:") {
            let sum: f32 = t
                .split_whitespace()
                .filter_map(|tok| tok.strip_suffix('%'))
                .filter_map(|x| x.parse::<f32>().ok())
                .sum();
            if sum > 0.0 {
                total = Some(sum);
            }
            continue;
        }
        if t.is_empty() || t.starts_with("Load:") || t.contains("TOTAL") {
            continue;
        }

        let mut it = t.split_whitespace();
        let Some(pct) = it
            .next()
            .and_then(|x| x.strip_suffix('%'))
            .and_then(|x| x.parse::<f32>().ok())
        else {
            continue;
        };
        let Some(id) = it.next() else { continue };
        let Some((pid_s, name_s)) = id.split_once('/') else {
            continue;
        };
        let name = name_s.trim_end_matches(':').to_string();
        if name.is_empty() {
            continue;
        }
        procs.push(ProcStat {
            name,
            pid: pid_s.parse().ok(),
            value: pct,
        });
    }

    procs.sort_by(|a, b| b.value.total_cmp(&a.value));
    procs.truncate(15);
    (total, procs)
}

/// Топ процессов по PSS из `dumpsys meminfo`.
pub fn parse_meminfo_top(s: &str) -> Vec<ProcStat> {
    let mut out = Vec::new();
    let mut in_section = false;

    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("Total PSS by process:") {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if t.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        let Some((val_s, rest)) = t.split_once("K:") else {
            if !out.is_empty() {
                break;
            }
            continue;
        };
        let Ok(kb) = val_s.replace(',', "").trim().parse::<u64>() else {
            continue;
        };
        let name = rest
            .split(" (pid")
            .next()
            .unwrap_or(rest)
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        out.push(ProcStat {
            name,
            pid: None,
            value: kb as f32 / 1024.0,
        });
    }

    out.sort_by(|a, b| b.value.total_cmp(&a.value));
    out.truncate(15);
    out
}

pub fn parse_device_info(s: &str) -> DeviceInfo {
    let m = split_marked(s);
    let get = |k: &str| {
        m.get(k)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };

    let size_raw = get("SIZE");
    let resolution = size_raw
        .lines()
        .find(|l| l.contains("Physical size"))
        .or_else(|| size_raw.lines().find(|l| l.contains("size")))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();

    let dens_raw = get("DENS");
    let density = dens_raw
        .lines()
        .find(|l| l.contains("Physical density"))
        .or_else(|| dens_raw.lines().find(|l| l.contains("density")))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();

    let cores = get("NPROC")
        .lines()
        .find_map(|l| l.trim().parse::<usize>().ok())
        .unwrap_or(0);

    DeviceInfo {
        manufacturer: get("MANUF"),
        model: get("MODEL"),
        android: get("REL"),
        sdk: get("SDK"),
        abi: get("ABI"),
        resolution,
        density,
        cores,
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} ГБ", b / GB)
    } else if b >= MB {
        format!("{:.0} МБ", b / MB)
    } else if b >= KB {
        format!("{:.0} КБ", b / KB)
    } else {
        format!("{b:.0} Б")
    }
}

pub fn human_kb(kb: u64) -> String {
    human_bytes(kb.saturating_mul(1024))
}

pub fn human_uptime(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    if d > 0 {
        format!("{d} д {h} ч {m} мин")
    } else if h > 0 {
        format!("{h} ч {m} мин")
    } else {
        format!("{m} мин")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices() {
        let s = "List of devices attached\nemulator-5554 device product:sdk model:Pixel_7 device:emu transport_id:1\nR58M123 offline\n";
        let d = parse_devices(s);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].serial, "emulator-5554");
        assert_eq!(d[0].model, "Pixel_7");
        assert_eq!(d[1].state, "offline");
    }

    #[test]
    fn meminfo() {
        let s = "MemTotal:        7876543 kB\nMemFree:          123456 kB\nMemAvailable:    3456789 kB\nBuffers:           1000 kB\nCached:          200000 kB\n";
        assert_eq!(parse_meminfo(s), Some((7_876_543, 7_876_543 - 3_456_789)));
    }

    #[test]
    fn meminfo_without_available() {
        let s = "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 150 kB\n";
        assert_eq!(parse_meminfo(s), Some((1000, 700)));
    }

    #[test]
    fn df_and_uptime() {
        assert_eq!(
            parse_df("Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/block/dm-5 1000 400 600 40% /data\n"),
            Some((400, 1000))
        );
        assert_eq!(parse_uptime("12345.67 999.0"), Some(12345));
    }

    #[test]
    fn battery() {
        let s = "Current Battery Service state:\n  AC powered: false\n  USB powered: true\n  status: 2\n  health: 2\n  level: 85\n  voltage: 4200\n  temperature: 310\n";
        let b = parse_battery(s);
        assert_eq!(b.level, Some(85));
        assert_eq!(b.temp_c, Some(31.0));
        assert_eq!(b.status, "заряжается");
        assert_eq!(b.plugged, "USB");
    }

    #[test]
    fn cpu() {
        let s = "Load: 1.2 / 1.1 / 1.0\nCPU usage: 12.3% user + 6.7% kernel + 0.2% iowait + 0.1% irq + 0.3% softirq\n  5.0% 1234/com.example: 3.0% user + 2.0% kernel / faults: 10 minor\n  9.0% 42/com.android.systemui: 5.0% user + 4.0% kernel\n";
        let (total, procs) = parse_cpuinfo(s);
        assert!((total.unwrap() - 19.6).abs() < 0.01);
        assert_eq!(procs[0].name, "com.android.systemui");
        assert_eq!(procs[0].pid, Some(42));
        assert_eq!(procs[1].name, "com.example");
    }

    #[test]
    fn mem_top() {
        let s = "Total PSS by process:\n    154,321K: com.example (pid 1234)\n     98,765K: com.other (pid 5678)\n\nTotal PSS by OOXML:\n";
        let p = parse_meminfo_top(s);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "com.example");
        assert!((p[0].value - 150.7).abs() < 0.2);
    }

    #[test]
    fn info() {
        let s = "__ADBW_MANUF__\nGoogle\n__ADBW_MODEL__\nPixel 7\n__ADBW_REL__\n14\n__ADBW_SDK__\n34\n__ADBW_ABI__\narm64-v8a\n__ADBW_SIZE__\nPhysical size: 1080x2400\n__ADBW_DENS__\nPhysical density: 420\n__ADBW_NPROC__\n8\n";
        let i = parse_device_info(s);
        assert_eq!(i.model, "Pixel 7");
        assert_eq!(i.resolution, "1080x2400");
        assert_eq!(i.cores, 8);
    }

    #[test]
    fn net() {
        let s = "Inter-|   Receive |  Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n    lo: 1000 1 0 0 0 0 0 0 1000 1 0 0 0 0 0 0\n  wlan0: 5000 2 0 0 0 0 0 0 7000 3 0 0 0 0 0 0\n";
        assert_eq!(parse_net(s), Some((5000, 7000)));
    }
}
