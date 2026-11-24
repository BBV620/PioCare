//! Modulul „Componente și servicii”: harta serviciilor Windows și propuneri pe profil.
//! Totul aici doar CITEȘTE. Aplicarea trece prin motor (copie, jurnal, anulare).
//!
//! Regula de siguranță principală: un serviciu se propune spre OPRIRE (disabled) doar dacă
//! niciun alt serviciu activ nu depinde de el. Altfel propunerea coboară la „manual”, care
//! e mereu sigur: Windows îl pornește singur când e nevoie de el.

use crate::actions::{ActionSpec, StartType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

const RULES: &str = include_str!("../../rules/services.json");

/// Servicii vitale: PioCare nu le schimbă niciodată, orice ar spune un pachet de reguli.
pub const CRITICAL: &[&str] = &[
    "RpcSs", "RpcEptMapper", "DcomLaunch", "LSM", "SamSs", "EventLog", "PlugPlay", "Power", "ProfSvc",
    "Schedule", "Winmgmt", "CryptSvc", "Dhcp", "Dnscache", "nsi", "BFE", "MpsSvc", "WinDefend", "WdNisSvc",
    "SecurityHealthService", "wscsvc", "Sense", "gpsvc", "AudioSrv", "AudioEndpointBuilder", "Themes",
    "UserManager", "CoreMessagingRegistrar", "StateRepository", "TimeBrokerSvc", "SystemEventsBroker",
    "BrokerInfrastructure", "LanmanWorkstation", "NlaSvc", "netprofm", "Wcmsvc", "WlanSvc", "EventSystem",
    "ShellHWDetection", "Appinfo", "TrustedInstaller", "wuauserv", "BITS", "UsoSvc", "WaaSMedicSvc",
    "vmms", "vmcompute", "KeyIso", "VaultSvc", "Netlogon", "SgrmBroker", "msiserver", "FontCache",
    "Audiosrv", "DeviceInstall", "DsmSvc", "camsvc", "AppXSvc", "ClipSVC", "LicenseManager", "sppsvc",
    "WinHttpAutoProxySvc", "iphlpsvc", "Netman", "SENS", "seclogon", "VSS", "swprv", "SDRSVC",
];

pub fn is_critical(name: &str) -> bool {
    CRITICAL.iter().any(|c| c.eq_ignore_ascii_case(name))
}

#[derive(Deserialize)]
struct Rule {
    id: String,
    service: String,
    proposal: StartType,
    risk: String,
    profiles: Vec<String>,
    #[serde(default)]
    needed_for: Vec<String>,
    why: String,
}

#[derive(Deserialize)]
struct Rules {
    schema_version: u32,
    version: String,
    features: BTreeMap<String, String>,
    rules: Vec<Rule>,
}

fn rules() -> Result<Rules, String> {
    let r: Rules = serde_json::from_str(RULES).map_err(|e| format!("rules/services.json: {e}"))?;
    if r.schema_version > crate::rules::SUPPORTED_SCHEMA {
        return Err("pachetul de reguli pentru servicii cere o versiune mai nouă a aplicației".into());
    }
    Ok(r)
}

#[derive(Serialize, Clone, Debug)]
pub struct Svc {
    pub name: String,
    pub display: String,
    pub running: bool,
    /// auto / delayed / manual / disabled / boot / system / ?
    pub start: String,
    pub depends_on: Vec<String>,
    pub required_by: Vec<String>,
    pub user_instance: bool,
    pub critical: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Proposal {
    pub service: String,
    pub display: String,
    pub running: bool,
    pub current: String,
    pub proposed: StartType,
    pub risk: String,
    pub why: String,
    pub rule_id: String,
    pub note: Option<String>,
    pub needed_for: Vec<String>,
}

#[derive(Serialize)]
pub struct Analysis {
    pub rules_version: String,
    pub features: BTreeMap<String, String>,
    pub total_services: usize,
    pub running_services: usize,
    pub proposals: Vec<Proposal>,
    /// Reguli sărite din motive de siguranță (pentru transparență).
    pub skipped: Vec<String>,
}

fn rank(start: &str) -> u8 {
    match start {
        "auto" | "delayed" => 3,
        "manual" => 2,
        "disabled" => 1,
        _ => 0,
    }
}

fn start_str(s: StartType) -> &'static str {
    match s {
        StartType::Auto => "auto",
        StartType::Delayed => "delayed",
        StartType::Manual => "manual",
        StartType::Disabled => "disabled",
    }
}

#[cfg(windows)]
pub fn list() -> Result<Vec<Svc>, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let running = running_set()?;
    let base = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Services")
        .map_err(|e| e.to_string())?;
    let mut out: Vec<Svc> = vec![];
    for name in base.enum_keys().flatten() {
        let Ok(k) = base.open_subkey(&name) else { continue };
        let ty: u32 = k.get_value("Type").unwrap_or(0);
        // Doar servicii Win32 (0x10 propriu proces, 0x20 proces comun), inclusiv cele per utilizator (0x50/0x60).
        if ty & 0x30 == 0 {
            continue;
        }
        let start: u32 = match k.get_value("Start") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let delayed: u32 = k.get_value("DelayedAutostart").unwrap_or(0);
        let mut display: String = k.get_value("DisplayName").unwrap_or_default();
        if display.starts_with('@') || display.is_empty() {
            display = running.get(&name.to_lowercase()).map(|(_, d)| d.clone()).unwrap_or(name.clone());
        }
        let deps: Vec<String> = k.get_value("DependOnService").unwrap_or_default();
        out.push(Svc {
            critical: is_critical(&name),
            user_instance: ty & 0x80 != 0 || ty & 0x40 != 0,
            running: running.get(&name.to_lowercase()).map_or(false, |(r, _)| *r),
            start: match (start, delayed) {
                (2, 1) => "delayed",
                (2, _) => "auto",
                (3, _) => "manual",
                (4, _) => "disabled",
                (0, _) => "boot",
                (1, _) => "system",
                _ => "?",
            }
            .into(),
            depends_on: deps.into_iter().filter(|d| !d.trim().is_empty()).collect(),
            required_by: vec![],
            display,
            name,
        });
    }
    // Dependențele inverse („cine depinde de mine”).
    let idx: HashMap<String, usize> = out.iter().enumerate().map(|(i, s)| (s.name.to_lowercase(), i)).collect();
    let pairs: Vec<(usize, String)> = out
        .iter()
        .flat_map(|s| s.depends_on.iter().filter_map(|d| idx.get(&d.to_lowercase()).map(|&i| (i, s.name.clone()))))
        .collect();
    for (i, by) in pairs {
        out[i].required_by.push(by);
    }
    out.sort_by(|a, b| a.display.to_lowercase().cmp(&b.display.to_lowercase()));
    Ok(out)
}

/// Serviciile pornite acum (nume mic → (rulează, nume afișat)), prin Service Control Manager.
#[cfg(windows)]
fn running_set() -> Result<HashMap<String, (bool, String)>, String> {
    use windows_sys::Win32::System::Services::*;
    let read = |p: *const u16| -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        unsafe {
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        }
    };
    let mut map = HashMap::new();
    unsafe {
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_ENUMERATE_SERVICE);
        if scm.is_null() {
            return Err(format!("Service Control Manager: {}", std::io::Error::last_os_error()));
        }
        let mut needed = 0u32;
        let mut count = 0u32;
        let mut resume = 0u32;
        // Maximum acceptat de EnumServicesStatusEx: 256 KB (peste, RPC dă eroarea 1783). Restul vine prin `resume`.
        let buf: Vec<u64> = vec![0; 32 * 1024];
        loop {
            let ok = EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                0x30, // SERVICE_WIN32
                3,    // SERVICE_STATE_ALL
                buf.as_ptr() as *mut u8,
                (buf.len() * 8) as u32,
                &mut needed,
                &mut count,
                &mut resume,
                std::ptr::null(),
            );
            let items = std::slice::from_raw_parts(buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, count as usize);
            for it in items {
                let name = read(it.lpServiceName);
                map.insert(name.to_lowercase(), (it.ServiceStatusProcess.dwCurrentState == 4, read(it.lpDisplayName)));
            }
            if ok != 0 {
                break;
            }
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(234) {
                // 234 = ERROR_MORE_DATA
                CloseServiceHandle(scm);
                return Err(format!("enumerare servicii: {err}"));
            }
        }
        CloseServiceHandle(scm);
    }
    Ok(map)
}

/// Toți cei care depind (direct sau indirect) de `name`.
fn dependents_all(svcs: &HashMap<String, &Svc>, name: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut stack = vec![name.to_lowercase()];
    let mut out = vec![];
    while let Some(n) = stack.pop() {
        if let Some(s) = svcs.get(&n) {
            for d in &s.required_by {
                if seen.insert(d.to_lowercase()) {
                    out.push(d.clone());
                    stack.push(d.to_lowercase());
                }
            }
        }
    }
    out
}

#[cfg(windows)]
pub fn analyze(profile: &str, uses: &[String]) -> Result<Analysis, String> {
    let r = rules()?;
    let all = list()?;
    let by: HashMap<String, &Svc> = all.iter().map(|s| (s.name.to_lowercase(), s)).collect();
    let mut props: Vec<Proposal> = vec![];
    let mut skipped = vec![];

    // Mai întâi setul de servicii care vor fi oprite, ca dependenții opriți împreună să nu blocheze.
    let applicable: Vec<&Rule> = r
        .rules
        .iter()
        .filter(|ru| ru.profiles.iter().any(|p| p == profile))
        .filter(|ru| !ru.needed_for.iter().any(|f| uses.contains(f)))
        .collect();
    let to_disable: HashSet<String> = applicable
        .iter()
        .filter(|ru| ru.proposal == StartType::Disabled)
        .map(|ru| ru.service.to_lowercase())
        .collect();

    for ru in applicable {
        let Some(s) = by.get(&ru.service.to_lowercase()) else { continue };
        if s.critical || is_critical(&ru.service) {
            skipped.push(format!("{}: serviciu vital, nu se atinge", s.display));
            continue;
        }
        if s.user_instance || rank(&s.start) == 0 {
            continue;
        }
        let mut proposed = ru.proposal;
        let mut note = None;
        if proposed == StartType::Disabled {
            let blockers: Vec<String> = dependents_all(&by, &s.name)
                .into_iter()
                .filter(|d| {
                    let dl = d.to_lowercase();
                    by.get(&dl).map_or(false, |x| x.start != "disabled") && !to_disable.contains(&dl)
                })
                .collect();
            if !blockers.is_empty() {
                let names: Vec<String> = blockers.iter().map(|b| by.get(&b.to_lowercase()).map_or(b.clone(), |x| x.display.clone())).collect();
                proposed = StartType::Manual;
                note = Some(format!("Rămâne pe manual (nu oprit), pentru că depind de el: {}", names.join(", ")));
            }
        }
        if rank(&s.start) <= rank(start_str(proposed)) {
            continue; // e deja la fel de „oprit” sau mai mult
        }
        props.push(Proposal {
            service: s.name.clone(),
            display: s.display.clone(),
            running: s.running,
            current: s.start.clone(),
            proposed,
            risk: ru.risk.clone(),
            why: ru.why.clone(),
            rule_id: ru.id.clone(),
            note,
            needed_for: ru.needed_for.clone(),
        });
    }
    props.sort_by(|a, b| a.risk.cmp(&b.risk).then(a.display.to_lowercase().cmp(&b.display.to_lowercase())));
    Ok(Analysis {
        rules_version: r.version,
        features: r.features,
        total_services: all.iter().filter(|s| !s.user_instance).count(),
        running_services: all.iter().filter(|s| s.running).count(),
        proposals: props,
        skipped,
    })
}

pub fn to_actions(props: &[Proposal]) -> Vec<ActionSpec> {
    props.iter().map(|p| ActionSpec::ServiceStartType { name: p.service.clone(), start: p.proposed }).collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_and_avoid_critical() {
        let r = rules().unwrap();
        assert!(r.rules.len() > 20);
        for ru in &r.rules {
            assert!(!is_critical(&ru.service), "regula {} atinge un serviciu vital", ru.id);
            for f in &ru.needed_for {
                assert!(r.features.contains_key(f), "regula {}: funcție necunoscută {f}", ru.id);
            }
        }
    }

    #[test]
    fn map_has_dependencies() {
        let all = list().unwrap();
        assert!(all.len() > 100);
        let rpc = all.iter().find(|s| s.name.eq_ignore_ascii_case("RpcSs")).unwrap();
        assert!(rpc.required_by.len() > 5, "RpcSs ar trebui să aibă mulți dependenți");
        assert!(rpc.critical);
    }

    /// Analiză reală, doar citire: cargo test print_analysis -- --ignored --nocapture
    #[test]
    #[ignore]
    fn print_analysis() {
        for profile in ["acasa", "birou", "gaming"] {
            let a = analyze(profile, &[]).unwrap();
            println!("== {profile}: {} servicii, {} pornite, {} propuneri", a.total_services, a.running_services, a.proposals.len());
            for p in &a.proposals {
                println!("  {} ({}) {} -> {:?} [{}] {}", p.display, p.service, p.current, p.proposed, p.risk, p.note.clone().unwrap_or_default());
            }
            for s in &a.skipped {
                println!("  SĂRIT: {s}");
            }
        }
    }
}
