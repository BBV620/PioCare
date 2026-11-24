//! Monitor live: procesor, RAM, procese, temperatura plăcii video (dacă driverul o oferă).
//! Doar citire, fără drivere speciale. Temperatura procesorului NU se citește: pe Windows ar cere un
//! driver de nucleu (ex. WinRing0), semnalat de Microsoft Defender ca vulnerabil.

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, CpuRefreshKind, MemoryRefreshKind};

static SYS: Mutex<Option<System>> = Mutex::new(None);
static GPU: Mutex<Option<(Instant, Option<Gpu>)>> = Mutex::new(None);

#[derive(Serialize, Clone)]
pub struct Gpu {
    pub name: String,
    pub temp_c: Option<f32>,
    pub load_pct: Option<f32>,
}

#[derive(Serialize)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub ram_mb: f64,
    pub cpu_pct: f32,
    pub protected: bool,
}

#[derive(Serialize)]
pub struct Sample {
    pub cpu_pct: f32,
    pub cpu_mhz: u64,
    pub cpu_name: String,
    pub cores: usize,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub swap_used_gb: f64,
    pub processes: Vec<Proc>,
    pub process_count: usize,
    pub gpu: Option<Gpu>,
}

/// Procese care nu se închid niciodată din PioCare (sistem sau PioCare însuși).
const PROTECTED: &[&str] = &[
    "system", "idle", "registry", "smss", "csrss", "wininit", "winlogon", "services", "lsass", "lsaiso", "svchost",
    "dwm", "fontdrvhost", "memory compression", "sihost", "explorer", "ctfmon", "audiodg", "spoolsv", "msmpeng",
    "securityhealthservice", "nissrv", "conhost", "runtimebroker", "searchindexer", "taskhostw", "piocare", "msedgewebview2",
];

fn is_protected(name: &str) -> bool {
    let lower = name.to_lowercase();
    let n = lower.trim_end_matches(".exe");
    PROTECTED.contains(&n)
}

/// GPU NVIDIA prin nvidia-smi (instalat odată cu driverul). Citit cel mult o dată la 5 secunde.
fn gpu() -> Option<Gpu> {
    let mut g = GPU.lock().ok()?;
    if let Some((t, v)) = g.as_ref() {
        if t.elapsed() < Duration::from_secs(5) {
            return v.clone();
        }
    }
    let v = crate::sys::run_hidden("nvidia-smi.exe", &["--query-gpu=name,temperature.gpu,utilization.gpu", "--format=csv,noheader,nounits"])
        .ok()
        .and_then(|s| {
            let parts: Vec<&str> = s.lines().next()?.split(',').map(|p| p.trim()).collect();
            Some(Gpu { name: parts.first()?.to_string(), temp_c: parts.get(1).and_then(|x| x.parse().ok()), load_pct: parts.get(2).and_then(|x| x.parse().ok()) })
        });
    *g = Some((Instant::now(), v.clone()));
    v
}

pub fn sample(by: &str, limit: usize) -> Sample {
    let mut guard = SYS.lock().unwrap_or_else(|e| e.into_inner());
    let sys = guard.get_or_insert_with(|| {
        let mut s = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()).with_memory(MemoryRefreshKind::everything()),
        );
        // Prima măsurătoare de procesor are nevoie de un interval de referință.
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        s.refresh_cpu_all();
        s
    });
    sys.refresh_cpu_all();
    sys.refresh_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_memory().with_cpu().with_exe(sysinfo::UpdateKind::OnlyIfNotSet));
    let cores = sys.cpus().len().max(1);
    let mut procs: Vec<Proc> = sys
        .processes()
        .values()
        .map(|p| {
            let name = p.name().to_string_lossy().into_owned();
            Proc {
                pid: p.pid().as_u32(),
                protected: is_protected(&name) || p.pid().as_u32() == std::process::id(),
                path: p.exe().map(|e| e.to_string_lossy().into_owned()),
                ram_mb: p.memory() as f64 / 1_048_576.0,
                cpu_pct: p.cpu_usage() / cores as f32,
                name,
            }
        })
        .collect();
    let process_count = procs.len();
    if by == "cpu" {
        procs.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        procs.sort_by(|a, b| b.ram_mb.partial_cmp(&a.ram_mb).unwrap_or(std::cmp::Ordering::Equal));
    }
    procs.truncate(limit);
    const GB: f64 = 1_073_741_824.0;
    Sample {
        cpu_pct: sys.global_cpu_usage(),
        cpu_mhz: sys.cpus().iter().map(|c| c.frequency()).max().unwrap_or(0),
        cpu_name: sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        cores,
        ram_used_gb: sys.used_memory() as f64 / GB,
        ram_total_gb: sys.total_memory() as f64 / GB,
        swap_used_gb: sys.used_swap() as f64 / GB,
        processes: procs,
        process_count,
        gpu: gpu(),
    }
}

/// Închide un proces al utilizatorului (fără drepturi de administrator). Procesele protejate sunt refuzate.
pub fn kill(pid: u32, expected_name: &str) -> Result<(), String> {
    let mut guard = SYS.lock().unwrap_or_else(|e| e.into_inner());
    let sys = guard.get_or_insert_with(System::new);
    sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    let p = sys.process(Pid::from_u32(pid)).ok_or_else(|| crate::i18n::t("procesul nu mai rulează", "the process is no longer running"))?;
    let name = p.name().to_string_lossy().into_owned();
    // Verificăm că PID-ul încă aparține aceluiași program (PID-urile se refolosesc).
    if !name.eq_ignore_ascii_case(expected_name) || is_protected(&name) || pid == std::process::id() {
        return Err(crate::i18n::t("acest proces nu poate fi închis din PioCare", "this process cannot be closed from PioCare"));
    }
    if p.kill() { Ok(()) } else { Err(crate::i18n::t("Windows a refuzat închiderea (poate cere administrator)", "Windows refused to close it (it may need administrator)")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_has_sane_values() {
        let s = sample("ram", 10);
        assert!(s.ram_total_gb > 1.0 && s.ram_used_gb > 0.1);
        assert!(s.cores >= 1 && s.process_count > 20);
        assert!(s.processes.len() <= 10);
        assert!(s.processes.windows(2).all(|w| w[0].ram_mb >= w[1].ram_mb));
        assert!(is_protected("svchost.exe") && is_protected("Explorer.EXE") && !is_protected("notepad.exe"));
    }
}
