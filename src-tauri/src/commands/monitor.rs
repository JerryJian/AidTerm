use std::collections::HashMap;
use std::process::Command as StdCommand;
use std::sync::Mutex;
use std::time::Instant;

use sysinfo::{Disks, Networks, System};
use tauri::State;

use crate::session::SessionManager;

const MARKERS: &str = "__AID_MONITOR__";

/// Read a `/proc/stat` line like `cpu  user nice system idle iowait irq softirq steal`.
/// Returns (total, idle) jiffies. idle includes iowait.
fn parse_cpu_stat(line: &str) -> Option<(u64, u64)> {
    let rest = line.strip_prefix("cpu ")?;
    let nums: Vec<u64> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    if nums.len() < 8 {
        return None;
    }
    let idle = nums[3] + nums[4];
    let total: u64 = nums[..8].iter().sum();
    Some((total, idle))
}

/// Count `cpuN` lines (physical/virtual cores) in `/proc/stat`.
fn parse_cpu_cores(stat: &str) -> u32 {
    let n = stat
        .lines()
        .filter(|l| l.starts_with("cpu") && !l.starts_with("cpu "))
        .filter(|l| {
            l.strip_prefix("cpu")
                .map(|rest| rest.chars().next().map_or(false, |c| c.is_ascii_digit()))
                .unwrap_or(false)
        })
        .count();
    n.max(1) as u32
}

fn parse_u64(s: &str) -> u64 {
    s.parse().unwrap_or(0)
}

fn parse_mem_kb(meminfo: &str, key: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            if k.trim() == key {
                v.split_whitespace().next().map(parse_u64)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Parse `/proc/net/dev` lines `  eth0: rx_bytes ... tx_bytes ...` -> name -> (rx, tx).
fn parse_net(dev: &str) -> HashMap<String, (u64, u64)> {
    let mut out = HashMap::new();
    for line in dev.lines() {
        let line = line.trim_start();
        if let Some((name, rest)) = line.split_once(':') {
            let name = name.trim();
            if name.is_empty() || name == "lo" {
                continue;
            }
            let fields: Vec<u64> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            if fields.len() >= 16 {
                out.insert(name.to_string(), (fields[0], fields[8]));
            }
        }
    }
    out
}

#[derive(Clone, Default)]
struct PrevSample {
    cpu: Option<(u64, u64)>,
    net: HashMap<String, (u64, u64)>,
    at: Option<Instant>,
}

pub struct MonitorState {
    prev: Mutex<HashMap<String, PrevSample>>,
}

impl Default for MonitorState {
    fn default() -> Self {
        Self {
            prev: Mutex::new(HashMap::new()),
        }
    }
}

impl MonitorState {
    pub fn clear(&self, session_id: &str) {
        if let Ok(mut map) = self.prev.lock() {
            map.remove(session_id);
        }
    }
}

#[derive(serde::Serialize)]
pub struct DiskMetric {
    pub mount: String,
    pub total_mb: u64,
    pub used_mb: u64,
}

#[derive(serde::Serialize)]
pub struct NetMetric {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

#[derive(serde::Serialize)]
pub struct GpuMetric {
    pub vendor: String,
    pub name: String,
    pub utilization: f64,
    pub mem_total_mb: u64,
    pub mem_used_mb: u64,
    pub temperature: f64,
}

#[derive(serde::Serialize)]
pub struct MonitorMetrics {
    pub cpu_percent: f64,
    pub cpu_cores: u32,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub mem_total_mb: u64,
    pub mem_used_mb: u64,
    pub swap_total_mb: u64,
    pub swap_used_mb: u64,
    pub disks: Vec<DiskMetric>,
    pub nets: Vec<NetMetric>,
    pub gpus: Vec<GpuMetric>,
}

/// Parse a GPU section emitted by the monitor command. Returns an empty vec
/// when the vendor probe produced nothing usable (GPU absent / tool missing).
fn parse_gpu(section: &str) -> Vec<GpuMetric> {
    let section = section.trim();
    if section.starts_with("__AID_GPU_NVIDIA__") {
        let mut gpus = Vec::new();
        for line in section.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 5 {
                continue;
            }
            let name = parts[0].trim().to_string();
            let util = parts[1].trim().parse::<f64>().unwrap_or(0.0);
            let mem_total = parts[2].trim().parse::<u64>().unwrap_or(0);
            let mem_used = parts[3].trim().parse::<u64>().unwrap_or(0);
            let temp = parts[4].trim().parse::<f64>().unwrap_or(0.0);
            gpus.push(GpuMetric {
                vendor: "nvidia".into(),
                name,
                utilization: util.clamp(0.0, 100.0),
                mem_total_mb: mem_total,
                mem_used_mb: mem_used,
                temperature: temp,
            });
        }
        gpus
    } else if section.starts_with("__AID_GPU_AMD__") {
        let json_text = section.lines().skip(1).collect::<Vec<_>>().join("\n");
        parse_amd_gpu(&json_text)
    } else if section.starts_with("__AID_GPU_INTEL__") {
        let json_text = section.lines().skip(1).collect::<Vec<_>>().join("\n");
        parse_intel_gpu(&json_text)
    } else {
        Vec::new()
    }
}

/// Tolerant parse of `rocm-smi --json`. Key names differ across versions, so
/// we scan object values by substring and treat unknown/missing fields as 0.
fn parse_amd_gpu(json_text: &str) -> Vec<GpuMetric> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_text) else {
        return Vec::new();
    };
    let Some(cards) = v.as_object() else {
        return Vec::new();
    };
    let mut gpus = Vec::new();
    for (card, val) in cards {
        if !card.starts_with("card") {
            continue;
        }
        let Some(obj) = val.as_object() else {
            continue;
        };
        let mut name = card.clone();
        let mut util = 0.0;
        let mut mem_used_b = 0u64;
        let mut mem_total_b = 0u64;
        let mut temp = 0.0;
        for (k, v) in obj {
            let kk = k.to_lowercase();
            let Some(s) = v.as_str() else {
                continue;
            };
            let num = s.split_whitespace().next().unwrap_or("").trim_end_matches('%');
            if kk.contains("product name") {
                name = s.trim().to_string();
            } else if kk.contains("gpu use") {
                util = num.parse::<f64>().unwrap_or(0.0);
            } else if kk.contains("memory used") {
                mem_used_b = num.parse::<u64>().unwrap_or(0);
            } else if kk.contains("memory total") {
                mem_total_b = num.parse::<u64>().unwrap_or(0);
            } else if kk.contains("temperature") {
                temp = num.parse::<f64>().unwrap_or(0.0);
            }
        }
        gpus.push(GpuMetric {
            vendor: "amd".into(),
            name,
            utilization: util.clamp(0.0, 100.0),
            mem_total_mb: mem_total_b / (1024 * 1024),
            mem_used_mb: mem_used_b / (1024 * 1024),
            temperature: temp,
        });
    }
    gpus
}

/// Tolerant parse of `intel_gpu_top -J` single JSON sample. Reports the max
/// engine busy as utilization; VRAM/temperature are not exposed.
fn parse_intel_gpu(json_text: &str) -> Vec<GpuMetric> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_text) else {
        return Vec::new();
    };
    let mut util: f64 = 0.0;
    if let Some(engines) = v.get("engines").and_then(|e| e.as_array()) {
        for e in engines {
            if let Some(b) = e.get("busy").and_then(|b| b.as_f64()) {
                util = util.max(b);
            }
        }
    }
    vec![GpuMetric {
        vendor: "intel".into(),
        name: "Intel GPU".into(),
        utilization: util.clamp(0.0, 100.0),
        mem_total_mb: 0,
        mem_used_mb: 0,
        temperature: 0.0,
    }]
}

#[tauri::command]
pub async fn get_system_metrics(
    manager: State<'_, SessionManager>,
    remote_state: State<'_, MonitorState>,
    local_state: State<'_, LocalMonitorState>,
    session_id: String,
) -> Result<MonitorMetrics, String> {
    // The monitor tool is about the connection target's system. SSH and WSL
    // report a Linux environment (remote / WSL distro) via exec; local
    // sessions report the local host via sysinfo.
    let stype = manager.session_type(&session_id);
    match stype {
        "ssh" | "wsl" => remote_system_metrics(&manager, &remote_state, &session_id).await,
        _ => local_system_metrics(&local_state, &session_id),
    }
}

async fn remote_system_metrics(
    manager: &SessionManager,
    state: &MonitorState,
    session_id: &str,
) -> Result<MonitorMetrics, String> {
    let cmd = format!(
        "printf '{mark}\\n'; cat /proc/stat 2>/dev/null; \
         printf '{mark}\\n'; cat /proc/meminfo 2>/dev/null; \
         printf '{mark}\\n'; cat /proc/loadavg 2>/dev/null; \
         printf '{mark}\\n'; df -Pk 2>/dev/null | tail -n +2; \
         printf '{mark}\\n'; cat /proc/net/dev 2>/dev/null | tail -n +3; \
         printf '{mark}\\n'; \
         if command -v nvidia-smi >/dev/null 2>&1; then \
           printf '__AID_GPU_NVIDIA__\\n'; \
           nvidia-smi --query-gpu=name,utilization.gpu,memory.total,memory.used,temperature.gpu --format=csv,noheader,nounits 2>/dev/null; \
         elif command -v rocm-smi >/dev/null 2>&1; then \
           printf '__AID_GPU_AMD__\\n'; \
           rocm-smi --showuse --showmeminfo vram --showtemp --json 2>/dev/null; \
         elif command -v intel_gpu_top >/dev/null 2>&1; then \
           printf '__AID_GPU_INTEL__\\n'; \
           intel_gpu_top -J -s 1000 -l 2>/dev/null | head -n 1; \
         fi",
        mark = MARKERS,
    );
    let out = manager.exec(session_id, &cmd).await?;

    let sections: Vec<&str> = out.split(MARKERS).collect();
    // sections[0] is leading empty; then [stat, meminfo, loadavg, df, netdev, gpu]
    let get = |idx: usize| sections.get(idx).copied().unwrap_or("");
    let stat = get(1);
    let meminfo = get(2);
    let loadavg = get(3);
    let df_out = get(4);
    let netdev = get(5);
    let gpu_section = get(6);

    let cpu_cores = parse_cpu_cores(stat);
    let cur_cpu = stat.lines().find_map(parse_cpu_stat);
    let cur_net = parse_net(netdev);
    let now = Instant::now();

    let mut prev_guard = state.prev.lock().map_err(|e| e.to_string())?;
    let prev = prev_guard.get(session_id).cloned().unwrap_or_default();
    let dt = prev.at.map(|at| now.duration_since(at).as_secs_f64()).unwrap_or(0.0);

    // CPU usage from delta of two samples.
    let cpu_percent = match (&prev.cpu, cur_cpu) {
        (Some((pt, pid)), Some((t, idle))) => {
            let dtot = t.saturating_sub(*pt);
            let didle = idle.saturating_sub(*pid);
            if dtot > 0 {
                let usage = 1.0 - (didle as f64 / dtot as f64);
                (usage * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            }
        }
        _ => 0.0,
    };

    // Load average (1m 5m 15m).
    let loads: Vec<f64> = loadavg.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    let (load_1, load_5, load_15) = match loads.as_slice() {
        [a, b, c, ..] => (*a, *b, *c),
        [a, b] => (*a, *b, 0.0),
        [a] => (*a, 0.0, 0.0),
        _ => (0.0, 0.0, 0.0),
    };

    // Memory (KB -> MB). Prefer MemAvailable, fall back to MemFree.
    let mem_total_kb = parse_mem_kb(meminfo, "MemTotal");
    let mem_avail_kb = parse_mem_kb(meminfo, "MemAvailable");
    let mem_used_kb = if mem_avail_kb > 0 {
        mem_total_kb.saturating_sub(mem_avail_kb)
    } else {
        mem_total_kb.saturating_sub(parse_mem_kb(meminfo, "MemFree"))
    };
    let swap_total_kb = parse_mem_kb(meminfo, "SwapTotal");
    let swap_used_kb = swap_total_kb.saturating_sub(parse_mem_kb(meminfo, "SwapFree"));

    // Disk usage, filtered to real filesystems.
    let mut disks = Vec::new();
    for line in df_out.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let fstype = fields[0];
        if matches!(fstype, "tmpfs" | "devtmpfs" | "udev" | "overlay" | "squashfs" | "shm") {
            continue;
        }
        let total_kb = parse_u64(fields[1]);
        let used_kb = parse_u64(fields[2]);
        let mount = fields[5].to_string();
        if mount.starts_with("/run")
            || mount.starts_with("/sys")
            || mount.starts_with("/dev")
            || mount.starts_with("/proc")
            || mount.contains("/snap")
        {
            continue;
        }
        disks.push(DiskMetric {
            mount,
            total_mb: total_kb / 1024,
            used_mb: used_kb / 1024,
        });
    }

    // Network rates from delta between samples.
    let prev_net = prev.net.clone();
    let mut nets: Vec<NetMetric> = cur_net
        .iter()
        .map(|(name, (rx, tx))| {
            let (prx, ptx) = prev_net.get(name).copied().unwrap_or((*rx, *tx));
            let (rx_bps, tx_bps) = if dt > 0.0 {
                (
                    (rx.saturating_sub(prx)) as f64 / dt,
                    (tx.saturating_sub(ptx)) as f64 / dt,
                )
            } else {
                (0.0, 0.0)
            };
            NetMetric {
                name: name.clone(),
                rx_bps,
                tx_bps,
            }
        })
        .collect();
    nets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let gpus = parse_gpu(gpu_section);

    prev_guard.insert(
        session_id.to_string(),
        PrevSample {
            cpu: cur_cpu,
            net: cur_net,
            at: Some(now),
        },
    );

    Ok(MonitorMetrics {
        cpu_percent,
        cpu_cores,
        load_1,
        load_5,
        load_15,
        mem_total_mb: mem_total_kb / 1024,
        mem_used_mb: mem_used_kb / 1024,
        swap_total_mb: swap_total_kb / 1024,
        swap_used_mb: swap_used_kb / 1024,
        disks,
        nets,
        gpus,
    })
}

/// Local (host) system metrics for `local`/`wsl` sessions, collected via
/// `sysinfo`. Cross-platform: Windows / Linux / macOS. GPU is best-effort —
/// probes nvidia-smi / rocm-smi / intel_gpu_top when present.
struct LocalSample {
    sys: System,
    disks: Disks,
    networks: Networks,
    at: Option<Instant>,
}

pub struct LocalMonitorState {
    samples: Mutex<HashMap<String, LocalSample>>,
}

impl Default for LocalMonitorState {
    fn default() -> Self {
        Self {
            samples: Mutex::new(HashMap::new()),
        }
    }
}

impl LocalMonitorState {
    pub fn clear(&self, session_id: &str) {
        if let Ok(mut map) = self.samples.lock() {
            map.remove(session_id);
        }
    }
}

/// Build a `StdCommand` that won't open a console window on Windows
/// (`CREATE_NO_WINDOW`), preventing the monitor's per-poll subprocess probes
/// (nvidia-smi / rocm-smi / intel_gpu_top) from flashing a black console.
fn probe_cmd(program: &str) -> StdCommand {
    let mut cmd = StdCommand::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Probe local GPU vendors in order, returning a section string shaped like
/// the SSH monitor's GPU block (`__AID_GPU_*__` + payload) or empty.
fn probe_local_gpu() -> String {
    // NVIDIA
    if let Ok(out) = probe_cmd("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,memory.total,memory.used,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if !text.trim().is_empty() {
                return format!("__AID_GPU_NVIDIA__\n{}", text);
            }
        }
    }

    // AMD (rocm-smi, Linux)
    if let Ok(out) = probe_cmd("rocm-smi")
        .args(["--showuse", "--showmeminfo", "vram", "--showtemp", "--json"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if !text.trim().is_empty() {
                return format!("__AID_GPU_AMD__\n{}", text);
            }
        }
    }

    // Intel
    if let Ok(out) = probe_cmd("intel_gpu_top")
        .args(["-J", "-s", "1000", "-l"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if !text.trim().is_empty() {
                return format!("__AID_GPU_INTEL__\n{}", text);
            }
        }
    }

    String::new()
}

fn local_loadavg() -> (f64, f64, f64) {
    let l = System::load_average();
    (l.one, l.five, l.fifteen)
}

fn local_mount_ok(mount: &std::path::Path) -> bool {
    let s = mount.to_string_lossy();
    if s.starts_with("/run")
        || s.starts_with("/sys")
        || s.starts_with("/dev")
        || s.starts_with("/proc")
        || s.contains("/snap")
        || s.contains("squashfs")
    {
        return false;
    }
    true
}

fn local_system_metrics(
    state: &LocalMonitorState,
    session_id: &str,
) -> Result<MonitorMetrics, String> {
    // Reuse a cached System per session so CPU/network deltas are correct
    // across successive 2s polls.
    let mut guard = state.samples.lock().map_err(|e| e.to_string())?;
    let now = Instant::now();
    let sample = guard
        .entry(session_id.to_string())
        .or_insert_with(|| LocalSample {
            sys: System::new(),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            at: None,
        });
    let sys = &mut sample.sys;

    sys.refresh_cpu_usage();
    sys.refresh_memory();
    sample.disks.refresh(true);
    sample.networks.refresh(true);

    let cpu_percent = sys.global_cpu_usage().clamp(0.0, 100.0) as f64;
    let cpu_cores = sys.cpus().len().max(1) as u32;
    let (load_1, load_5, load_15) = local_loadavg();

    let mem_total_mb = sys.total_memory() / (1024 * 1024);
    let mem_used_mb = sys.used_memory() / (1024 * 1024);
    let swap_total_mb = sys.total_swap() / (1024 * 1024);
    let swap_used_mb = sys.used_swap() / (1024 * 1024);

    let disks = sample
        .disks
        .list()
        .iter()
        .filter(|d| local_mount_ok(d.mount_point()))
        .map(|d| DiskMetric {
            mount: d.mount_point().to_string_lossy().to_string(),
            total_mb: d.total_space() / (1024 * 1024),
            used_mb: d.total_space().saturating_sub(d.available_space()) / (1024 * 1024),
        })
        .collect::<Vec<_>>();

    // Network rates from delta between samples (received()/transmitted() are
    // the byte counts since the last refresh of `networks`).
    let dt = sample
        .at
        .map(|at| now.duration_since(at).as_secs_f64())
        .unwrap_or(0.0);
    let mut nets: Vec<NetMetric> = sample
        .networks
        .list()
        .iter()
        .filter(|(name, _)| name.as_str() != "lo")
        .map(|(name, data)| NetMetric {
            name: name.clone(),
            rx_bps: if dt > 0.0 {
                data.received() as f64 / dt
            } else {
                0.0
            },
            tx_bps: if dt > 0.0 {
                data.transmitted() as f64 / dt
            } else {
                0.0
            },
        })
        .collect();
    nets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Best-effort GPU.
    let gpus = {
        let section = probe_local_gpu();
        if section.is_empty() {
            Vec::new()
        } else {
            parse_gpu(&section)
        }
    };

    sample.at = Some(now);

    Ok(MonitorMetrics {
        cpu_percent,
        cpu_cores,
        load_1,
        load_5,
        load_15,
        mem_total_mb,
        mem_used_mb,
        swap_total_mb,
        swap_used_mb,
        disks,
        nets,
        gpus,
    })
}
