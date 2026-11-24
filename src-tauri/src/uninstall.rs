//! Dezinstalatorul încorporat.
//! Pașii: (1) lista programelor; (2) rulează dezinstalatorul OFICIAL al programului;
//! (3) caută resturile lăsate în urmă; (4) le șterge prin motor: cheile de registru cu copie,
//! folderele mutate în carantină (anulabil), serviciile orfane doar oprite.
//! Nu se atinge nimic comun altor programe sau Windows-ului.

use crate::actions::{ActionSpec, Root, StartType};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Program {
    /// „HKLM|cale” sau „HKCU|cale” către cheia Uninstall.
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub install_location: String,
    pub install_date: String,
    pub size_kb: u64,
    pub uninstall_string: String,
    pub machine: bool,
    pub msi: bool,
    /// Componentă folosită de alte programe (Visual C++, .NET, runtime-uri, drivere).
    pub shared: bool,
}

fn is_shared_component(name: &str, publisher: &str) -> bool {
    let n = name.to_lowercase();
    let p = publisher.to_lowercase();
    ["redistributable", "runtime", ".net", "directx", "visual c++", "webview2", "driver", "chipset", "sdk", "framework", "vcredist", "physx"]
        .iter()
        .any(|k| n.contains(k))
        || (p.contains("microsoft") && (n.contains("update for") || n.contains("hotfix")))
}

fn split_id(id: &str) -> Option<(Root, String)> {
    let (r, p) = id.split_once('|')?;
    let root = match r {
        "HKLM" => Root::HKLM,
        "HKCU" => Root::HKCU,
        _ => return None,
    };
    if !p.to_lowercase().contains("\\currentversion\\uninstall\\") {
        return None;
    }
    Some((root, p.to_string()))
}

#[cfg(windows)]
fn hk(root: Root) -> winreg::RegKey {
    use winreg::enums::*;
    winreg::RegKey::predef(if root == Root::HKCU { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE })
}

#[cfg(windows)]
pub fn list() -> Vec<Program> {
    let sources = [
        (Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (Root::HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (Root::HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ];
    let mut out: Vec<Program> = vec![];
    for (root, base) in sources {
        let Ok(key) = hk(root).open_subkey(base) else { continue };
        for sub in key.enum_keys().flatten() {
            let Ok(k) = key.open_subkey(&sub) else { continue };
            let name: String = k.get_value("DisplayName").unwrap_or_default();
            let uninst: String = k.get_value("UninstallString").unwrap_or_default();
            let system: u32 = k.get_value("SystemComponent").unwrap_or(0);
            let parent: Result<String, _> = k.get_value("ParentKeyName");
            if name.trim().is_empty() || uninst.trim().is_empty() || system == 1 || parent.is_ok() {
                continue;
            }
            let msi: u32 = k.get_value("WindowsInstaller").unwrap_or(0);
            let is_msi = msi == 1 || sub.starts_with('{') && uninst.to_lowercase().contains("msiexec");
            let p = Program {
                id: format!("{}|{base}\\{sub}", if root == Root::HKCU { "HKCU" } else { "HKLM" }),
                name: name.trim().to_string(),
                publisher: k.get_value::<String, _>("Publisher").unwrap_or_default().trim().to_string(),
                version: k.get_value("DisplayVersion").unwrap_or_default(),
                install_location: k.get_value::<String, _>("InstallLocation").unwrap_or_default().trim().trim_matches('"').to_string(),
                install_date: k.get_value("InstallDate").unwrap_or_default(),
                size_kb: k.get_value::<u32, _>("EstimatedSize").unwrap_or(0) as u64,
                uninstall_string: uninst,
                machine: root == Root::HKLM,
                msi: is_msi,
                shared: is_shared_component(&name, &k.get_value::<String, _>("Publisher").unwrap_or_default()),
            };
            // Același program poate apărea de două ori (ex. 32 + 64 biți): păstrăm o singură intrare pe nume+versiune.
            if !out.iter().any(|o| o.name == p.name && o.version == p.version && o.publisher == p.publisher) {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(windows)]
pub fn exists(id: &str) -> bool {
    split_id(id).map_or(false, |(r, p)| hk(r).open_subkey(p).is_ok())
}

/// Desparte linia de comandă a dezinstalatorului în (program, parametri).
pub fn split_command(cmd: &str, msi_guid: Option<&str>) -> Option<(String, String)> {
    let c = cmd.trim();
    let lower = c.to_lowercase();
    if lower.starts_with("msiexec") || lower.contains("\\msiexec.exe") {
        // Multe programe MSI scriu „/I{GUID}” (modificare); pentru dezinstalare e „/X{GUID}”.
        let guid = msi_guid.map(|g| g.to_string()).or_else(|| {
            let s = c.find('{')?;
            let e = c[s..].find('}')? + s;
            Some(c[s..=e].to_string())
        })?;
        return Some(("msiexec.exe".into(), format!("/X{guid}")));
    }
    if let Some(rest) = c.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_string(), rest[end + 1..].trim().to_string()));
    }
    match lower.find(".exe") {
        Some(i) => Some((c[..i + 4].to_string(), c[i + 4..].trim().to_string())),
        None => Some((c.to_string(), String::new())),
    }
}

/// Pornește dezinstalatorul oficial și așteaptă să se închidă. Pentru programele instalate
/// pentru toți utilizatorii cere direct drepturi de administrator (fereastra UAC).
#[cfg(windows)]
pub fn run(id: &str) -> Result<u32, String> {
    let prog = list().into_iter().find(|p| p.id == id).ok_or_else(|| crate::i18n::t("programul nu mai este instalat", "the program is no longer installed"))?;
    let guid = id.rsplit('\\').next().filter(|g| g.starts_with('{') && g.ends_with('}'));
    let (exe, params) = split_command(&prog.uninstall_string, if prog.msi { guid } else { None })
        .ok_or_else(|| crate::i18n::t("nu înțeleg comanda de dezinstalare", "cannot understand the uninstall command"))?;
    crate::sys::shell_run_wait(&exe, &params, prog.machine)
}

// ---------------- resturi ----------------

#[derive(Serialize, Clone, Debug)]
pub struct Leftover {
    pub id: String,
    /// registry / folder / shortcut / startup / service
    pub kind: String,
    pub location: String,
    pub size_bytes: u64,
    pub needs_admin: bool,
    pub action: ActionSpec,
}

/// Normalizare pentru comparații: litere mici, fără versiuni, paranteze și semne.
pub fn norm(s: &str) -> String {
    let mut t = s.to_lowercase();
    for junk in ["(x64)", "(x86)", "(64-bit)", "(32-bit)", "64-bit", "32-bit", " x64", " x86"] {
        t = t.replace(junk, " ");
    }
    // Scoatem versiunile de la coadă (ex. „7-Zip 23.01” → „7-zip”).
    let words: Vec<&str> = t.split_whitespace().collect();
    let mut keep: Vec<&str> = words.clone();
    while let Some(last) = keep.last() {
        if last.chars().any(|c| c.is_ascii_digit()) && last.chars().all(|c| c.is_ascii_digit() || c == '.' || c == 'v' || c == '-') && keep.len() > 1 {
            keep.pop();
        } else {
            break;
        }
    }
    keep.join(" ").chars().filter(|c| c.is_alphanumeric() || *c == '+').collect()
}

fn norm_publisher(s: &str) -> String {
    let mut t = format!(" {} ", s.to_lowercase().replace([',', '.'], " "));
    for suffix in [" inc ", " ltd ", " llc ", " gmbh ", " corporation ", " corp ", " co ", " srl ", " s r l ", " sa ", " ag ", " limited ", " software "] {
        t = t.replace(suffix, " ");
    }
    t.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Producători ale căror foldere/chei „mamă” nu se șterg niciodată (sunt comune multor produse).
const SHARED_PUBLISHERS: &[&str] = &["microsoft", "windows", "intel", "nvidia", "amd", "advancedmicrodevices", "realtek", "google", "apple", "oracle", "adobe", "mozilla"];

/// Nume de foldere/chei care nu se ating niciodată, chiar dacă seamănă cu numele programului.
const NEVER_NAMES: &[&str] = &[
    "microsoft", "windows", "classes", "policies", "wow6432node", "commonfiles", "packages", "programs", "temp", "startup",
    "desktop", "documents", "programfiles", "programfilesx86", "programdata", "appdata", "local", "roaming", "locallow", "bin",
    "app", "apps", "data", "common", "shared", "tools", "users", "public", "system", "system32", "drivers", "cache", "logs",
    "installer", "installers", "setup", "update", "updater", "uninstall", "application", "program", "x64", "x86", "default",
    "config", "settings", "user", "crashpad", "plugins", "runtime", "service", "services", "client", "launcher", "assets",
];

#[cfg(windows)]
fn env(v: &str) -> Option<PathBuf> {
    std::env::var(v).ok().map(PathBuf::from)
}

/// Rădăcinile în care PioCare are voie să caute și să mute foldere rămase.
#[cfg(windows)]
pub fn allowed_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "APPDATA", "LOCALAPPDATA"] {
        if let Some(p) = env(var) {
            v.push(p);
        }
    }
    if let Some(l) = env("LOCALAPPDATA") {
        v.push(l.join("Programs"));
    }
    for base in [env("APPDATA"), env("ProgramData")].into_iter().flatten() {
        v.push(base.join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    if let Some(u) = env("USERPROFILE") {
        v.push(u.join("Desktop"));
    }
    if let Some(p) = env("PUBLIC") {
        v.push(p.join("Desktop"));
    }
    v
}

/// Un folder/fișier poate fi mutat în carantină doar dacă e DIRECT sub o rădăcină permisă
/// (sau la doi pași, sub un folder de producător) și nu are un nume protejat.
#[cfg(windows)]
pub fn check_quarantine_path(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let lower = path.to_lowercase();
    if lower.contains("..") || !p.is_absolute() {
        return Err(crate::i18n::t("cale invalidă", "invalid path"));
    }
    let win = env("SystemRoot").map(|w| w.to_string_lossy().to_lowercase()).unwrap_or_else(|| "c:\\windows".into());
    if lower.starts_with(&win) {
        return Err(crate::i18n::t("folderul Windows nu se atinge", "the Windows folder is never touched"));
    }
    let leaf = p.file_name().map(|f| norm(&f.to_string_lossy())).unwrap_or_default();
    if leaf.is_empty() || NEVER_NAMES.contains(&leaf.as_str()) {
        return Err(crate::i18n::t("nume de folder protejat", "protected folder name"));
    }
    for root in allowed_roots() {
        if let Ok(rest) = p.strip_prefix(&root) {
            let depth = rest.components().count();
            if (1..=2).contains(&depth) {
                return Ok(());
            }
        }
    }
    Err(crate::i18n::t("calea nu e într-un loc permis pentru carantină", "the path is not in a location allowed for quarantine"))
}

/// Unde se mută ceva în carantină: pe același disc, ca mutarea să fie instantanee.
#[cfg(windows)]
pub fn quarantine_target(path: &str, op_id: &str, idx: usize) -> PathBuf {
    let p = PathBuf::from(path);
    let leaf = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| "x".into());
    let user = env("USERPROFILE").map(|u| path.to_lowercase().starts_with(&u.to_string_lossy().to_lowercase())).unwrap_or(false);
    let base = if user {
        env("LOCALAPPDATA").unwrap_or_default().join("PioCare\\carantina")
    } else if path.to_lowercase().starts_with("c:") {
        env("ProgramData").unwrap_or_else(|| PathBuf::from("C:\\ProgramData")).join("PioCare\\carantina")
    } else {
        PathBuf::from(&path[..3]).join(".piocare-carantina")
    };
    base.join(op_id).join(format!("{idx:02}_{leaf}"))
}

#[cfg(windows)]
pub fn quarantine_needs_admin(path: &str) -> bool {
    let lower = path.to_lowercase();
    let user = env("USERPROFILE").map(|u| lower.starts_with(&u.to_string_lossy().to_lowercase())).unwrap_or(false);
    !user
}

fn dir_size(p: &Path) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if m.is_file() {
        return m.len();
    }
    std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let path = e.path();
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => 0,
                Ok(m) if m.is_dir() => dir_size(&path),
                Ok(m) => m.len(),
                Err(_) => 0,
            }
        })
        .sum()
}

#[cfg(windows)]
pub fn leftovers(prog: &Program, still_installed: &[Program]) -> Vec<Leftover> {
    let mut out: Vec<Leftover> = vec![];
    let n = norm(&prog.name);
    let pubn = norm_publisher(&prog.publisher);
    let loc = prog.install_location.trim_end_matches('\\').to_string();
    let loc_leaf = Path::new(&loc).file_name().map(|f| norm(&f.to_string_lossy())).unwrap_or_default();
    let names: Vec<String> = [n.clone(), loc_leaf.clone()]
        .into_iter()
        .filter(|s| s.len() >= 3 && !NEVER_NAMES.contains(&s.as_str()) && !SHARED_PUBLISHERS.contains(&s.as_str()))
        .collect();
    // Folderele încă folosite de alte programe instalate nu se ating.
    let other_locs: Vec<String> = still_installed
        .iter()
        .filter(|p| p.id != prog.id && !p.install_location.trim().is_empty())
        .map(|p| p.install_location.trim_end_matches('\\').to_lowercase())
        .collect();
    if names.is_empty() {
        return out;
    }
    let same_publisher_left = still_installed.iter().any(|p| p.id != prog.id && !pubn.is_empty() && norm_publisher(&p.publisher) == pubn);
    let shared_pub = pubn.is_empty() || SHARED_PUBLISHERS.iter().any(|s| pubn.starts_with(s));
    // Numele altor programe încă instalate: o cheie/un folder care se potrivește și cu ele e comun (ex. „Python”
    // pentru mai multe versiuni de Python, „PostgreSQL” pentru 16 și 18) și nu se atinge.
    let other_names: Vec<String> = still_installed
        .iter()
        .filter(|p| p.id != prog.id)
        .flat_map(|p| {
            let leaf = Path::new(p.install_location.trim_end_matches('\\')).file_name().map(|f| norm(&f.to_string_lossy())).unwrap_or_default();
            [norm(&p.name), leaf]
        })
        .filter(|s| !s.is_empty())
        .collect();
    let matches = |leaf: &str| {
        let l = norm(leaf);
        names.contains(&l) && !other_names.contains(&l)
    };
    let mut push = |kind: &str, location: String, size: u64, action: ActionSpec| {
        let id = format!("{kind}|{location}");
        if !out.iter().any(|o| o.id == id) {
            out.push(Leftover { id, kind: kind.into(), needs_admin: action.needs_admin(), location, size_bytes: size, action });
        }
    };

    // 1. Cheia de dezinstalare, dacă dezinstalatorul n-a scos-o.
    if let Some((root, path)) = split_id(&prog.id) {
        if hk(root).open_subkey(&path).is_ok() {
            push("registry", format!("{}\\{path}", if root == Root::HKCU { "HKCU" } else { "HKLM" }), 0, ActionSpec::RegDeleteKey { root, path });
        }
    }

    // 2. Chei Software\<Program> și Software\<Producător>\<Program>.
    for (root, base) in [
        (Root::HKCU, "Software"),
        (Root::HKLM, "SOFTWARE"),
        (Root::HKLM, "SOFTWARE\\WOW6432Node"),
    ] {
        let Ok(sw) = hk(root).open_subkey(base) else { continue };
        for k in sw.enum_keys().flatten() {
            let kn = norm(&k);
            if NEVER_NAMES.contains(&kn.as_str()) {
                continue;
            }
            if matches(&k) {
                let path = format!("{base}\\{k}");
                push("registry", format!("{}\\{path}", if root == Root::HKCU { "HKCU" } else { "HKLM" }), 0, ActionSpec::RegDeleteKey { root, path });
            } else if !pubn.is_empty() && norm_publisher(&k) == pubn && !shared_pub {
                if let Ok(pk) = sw.open_subkey(&k) {
                    let subs: Vec<String> = pk.enum_keys().flatten().collect();
                    for s in subs.iter().filter(|s| matches(s)) {
                        let path = format!("{base}\\{k}\\{s}");
                        push("registry", format!("{}\\{path}", if root == Root::HKCU { "HKCU" } else { "HKLM" }), 0, ActionSpec::RegDeleteKey { root, path });
                    }
                    // Cheia producătorului întreagă: doar dacă nu mai e alt produs al lui și nu e un producător „comun”.
                    let only_this = subs.iter().all(|s| matches(s));
                    if !same_publisher_left && !shared_pub && only_this {
                        let path = format!("{base}\\{k}");
                        push("registry", format!("{}\\{path}", if root == Root::HKCU { "HKCU" } else { "HKLM" }), 0, ActionSpec::RegDeleteKey { root, path });
                    }
                }
            }
        }
    }

    // 3. Porniri automate care trimit în folderul programului sau poartă numele lui, iar fișierul lipsește.
    for (root, base) in [
        (Root::HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
        (Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"),
        (Root::HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ] {
        let Ok(k) = hk(root).open_subkey(base) else { continue };
        for (vname, _) in k.enum_values().flatten() {
            let cmd: String = k.get_value(&vname).unwrap_or_default();
            let Some(exe) = crate::cleaner::exe_from_command(&cmd) else { continue };
            let in_loc = !loc.is_empty() && exe.to_lowercase().starts_with(&loc.to_lowercase());
            if (in_loc || matches(&vname)) && crate::cleaner::file_state(&exe) == crate::cleaner::FileState::Missing {
                push(
                    "startup",
                    format!("{}\\{base} → {vname}", if root == Root::HKCU { "HKCU" } else { "HKLM" }),
                    0,
                    ActionSpec::RegDeleteValue { root, path: base.into(), name: vname },
                );
            }
        }
    }

    // 4. Servicii rămase ale programului (fișierul lipsește): doar se opresc, nu se șterg.
    if !loc.is_empty() {
        if let Ok(svcs) = crate::services::list() {
            for s in svcs.iter().filter(|s| !s.critical && s.start != "disabled") {
                let image: String = hk(Root::HKLM)
                    .open_subkey(format!("SYSTEM\\CurrentControlSet\\Services\\{}", s.name))
                    .and_then(|k| k.get_value("ImagePath"))
                    .unwrap_or_default();
                let Some(exe) = crate::cleaner::exe_from_command(&image) else { continue };
                if exe.to_lowercase().starts_with(&loc.to_lowercase()) && crate::cleaner::file_state(&exe) == crate::cleaner::FileState::Missing {
                    push("service", format!("{} ({})", s.display, s.name), 0, ActionSpec::ServiceStartType { name: s.name.clone(), start: StartType::Disabled });
                }
            }
        }
    }

    // 5. Foldere și scurtături rămase.
    let mut candidates: Vec<PathBuf> = vec![];
    if !loc.is_empty() && Path::new(&loc).exists() {
        candidates.push(PathBuf::from(&loc));
    }
    for root in allowed_roots() {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.trim_end_matches(".lnk").trim_end_matches(".url");
            if matches(stem) {
                candidates.push(e.path());
            } else if !pubn.is_empty()
                && norm_publisher(&name) == pubn
                && e.path().is_dir()
                // Pentru producătorii „comuni” (Microsoft, Intel…) căutăm în folderul lor doar sub Program Files.
                && (!shared_pub || root.to_string_lossy().to_lowercase().contains("program files"))
            {
                if let Ok(rd2) = std::fs::read_dir(e.path()) {
                    let subs: Vec<_> = rd2.flatten().collect();
                    for s in &subs {
                        let sname = s.file_name().to_string_lossy().into_owned();
                        if matches(sname.trim_end_matches(".lnk")) {
                            candidates.push(s.path());
                        }
                    }
                    let only_this = subs.iter().all(|s| matches(&s.file_name().to_string_lossy()));
                    if !same_publisher_left && !shared_pub && only_this {
                        candidates.push(e.path());
                    }
                }
            }
        }
    }
    for c in candidates {
        let s = c.to_string_lossy().into_owned();
        if check_quarantine_path(&s).is_err() {
            continue;
        }
        let sl = s.to_lowercase();
        if other_locs.iter().any(|o| o.starts_with(&sl) || sl.starts_with(o.as_str())) {
            continue; // e (sau conține) folderul altui program încă instalat
        }
        let kind = if s.to_lowercase().ends_with(".lnk") || s.to_lowercase().ends_with(".url") || s.to_lowercase().contains("\\start menu\\") { "shortcut" } else { "folder" };
        push(kind, s.clone(), dir_size(&c), ActionSpec::Quarantine { path: s });
    }

    // Dacă un folder e deja în listă, conținutul lui nu mai trebuie listat separat.
    let folders: Vec<String> = out.iter().filter(|o| o.kind == "folder").map(|o| o.location.to_lowercase() + "\\").collect();
    out.retain(|o| !(o.kind != "registry" && o.kind != "startup" && o.kind != "service" && folders.iter().any(|f| o.location.to_lowercase().starts_with(f))));
    out
}

// ---------------- foldere orfane ----------------

/// Foldere care nu se propun niciodată (Windows, drivere, componente comune, securitate).
const ORPHAN_NEVER: &[&str] = &[
    "microsoft", "windows", "windowsapps", "packages", "packagecache", "commonfiles", "windowsdefender", "windowsdefenderadvancedthreatprotection",
    "windowsmail", "windowsmediaplayer", "windowsnt", "windowsphotoviewer", "windowsportabledevices", "windowspowershell", "windowssidebar",
    "internetexplorer", "modifiablewindowsapps", "msbuild", "referenceassemblies", "dotnet", "nvidia", "nvidiacorporation", "intel", "amd",
    "ati", "realtek", "malwarebytes", "mozilla", "google", "regid19910908com.microsoft", "ssh", "usoshared", "usoprivate", "comms", "softwaredistribution",
    "temp", "tmp", "crashdumps", "connecteddevicesplatform", "d3dscache", "packagemanagement", "programs", "publisher", "virtualstore",
    "package", "desktop.ini", "piocare", "piogrup", "azor", "baukurier", "piocut", "piotile", "pionest", "claude", "anthropic",
    "nodejs", "npm", "npmcache", "python", "pip", "git", "githubcli", "rustup", "cargo", "uninstallinformation", "installer",
    "applicationdata", "startmenu", "templates", "documents", "regid", "oem", "boot", "recovery", "drivers", "hyperv", "vmware",
];

#[derive(Serialize, Clone, Debug)]
pub struct Orphan {
    pub id: String,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub days_unused: u64,
    pub needs_admin: bool,
    /// Producător cunoscut ca lăsând resturi (Avast, AVG, IObit…): propus bifat.
    pub likely: bool,
}

const KNOWN_LEFTOVER: &[&str] = &["avast", "avg", "evorim", "iobit", "itop", "ccleaner", "piriform", "mcafee", "norton", "kaspersky", "bitdefender", "advancedsystemoptimizer", "systweak", "driverbooster", "wondershare", "babylon", "ask", "opencandy", "utorrent", "bittorrent"];

#[cfg(windows)]
pub fn orphans() -> Vec<Orphan> {
    use std::collections::HashSet;
    let installed = list();
    // Nume „în viață”: programe instalate (nume, producător, folder), primul cuvânt semnificativ din nume.
    let mut tokens: HashSet<String> = HashSet::new();
    let mut live_paths: Vec<String> = vec![];
    for p in &installed {
        tokens.insert(norm(&p.name));
        let pubn = norm_publisher(&p.publisher);
        if pubn.len() >= 3 {
            tokens.insert(pubn);
        }
        if let Some(first) = p.name.split_whitespace().next() {
            let f = norm(first);
            if f.len() >= 4 {
                tokens.insert(f);
            }
        }
        let loc = p.install_location.trim_end_matches('\\').to_lowercase();
        if loc.len() > 3 {
            if let Some(leaf) = Path::new(&loc).file_name() {
                tokens.insert(norm(&leaf.to_string_lossy()));
            }
            live_paths.push(loc);
        }
    }
    // Căi folosite acum: servicii, sarcini, programe la pornire, procese care rulează.
    if let Ok(svcs) = crate::services::list() {
        for s in svcs {
            if let Ok(k) = hk(Root::HKLM).open_subkey(format!("SYSTEM\\CurrentControlSet\\Services\\{}", s.name)) {
                let img: String = k.get_value("ImagePath").unwrap_or_default();
                if let Some(e) = crate::cleaner::exe_from_command(&img) {
                    live_paths.push(e.to_lowercase());
                }
            }
        }
    }
    for t in crate::startup::list() {
        if let Some(p) = t.path {
            live_paths.push(p.to_lowercase());
        }
    }
    for t in crate::tasks::list() {
        if let Some(p) = t.file {
            live_paths.push(p.to_lowercase());
        }
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    for p in sys.processes().values() {
        if let Some(e) = p.exe() {
            live_paths.push(e.to_string_lossy().to_lowercase());
        }
    }

    let roots = [
        (std::env::var("ProgramData").ok(), true),
        (std::env::var("ProgramFiles").ok(), true),
        (std::env::var("ProgramFiles(x86)").ok(), true),
        (std::env::var("APPDATA").ok(), false),
        (std::env::var("LOCALAPPDATA").ok(), false),
        (std::env::var("LOCALAPPDATA").ok().map(|l| format!("{l}\\Programs")), false),
    ];
    let now = std::time::SystemTime::now();
    let mut out = vec![];
    for (root, admin) in roots {
        let Some(root) = root else { continue };
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(m) = std::fs::symlink_metadata(&path) else { continue };
            if !m.is_dir() || m.file_type().is_symlink() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let n = norm(&name);
            if n.len() < 3 || name.starts_with('.') || name.starts_with('{') || ORPHAN_NEVER.iter().any(|x| n == *x || n.starts_with(x)) {
                continue;
            }
            // Se potrivește cu ceva instalat? (în ambele sensuri: „Avast Software” ↔ „avast”)
            if tokens.iter().any(|t| t.len() >= 3 && (n.starts_with(t.as_str()) || t.starts_with(n.as_str()))) {
                continue;
            }
            let pl = path.to_string_lossy().to_lowercase();
            if live_paths.iter().any(|lp| lp.starts_with(&format!("{pl}\\")) || *lp == pl) {
                continue; // în uz
            }
            let s = path.to_string_lossy().into_owned();
            if check_quarantine_path(&s).is_err() {
                continue;
            }
            let days = m.modified().ok().and_then(|t| now.duration_since(t).ok()).map(|d| d.as_secs() / 86_400).unwrap_or(0);
            let likely = KNOWN_LEFTOVER.iter().any(|k| n.starts_with(k));
            // Doar producătorii cunoscuți că lasă resturi. Restul folderelor „nepotrivite” sunt de obicei
            // setări ale unor programe portabile sau per-utilizator, pe care PioCare nu le poate dovedi orfane.
            if !likely {
                continue;
            }
            out.push(Orphan { id: s.clone(), path: s, name, size_bytes: dir_size(&path), days_unused: days, needs_admin: admin, likely });
        }
    }
    out.sort_by(|a, b| b.likely.cmp(&a.likely).then(b.size_bytes.cmp(&a.size_bytes)));
    out
}

#[cfg(all(test, windows))]
mod tests {
    /// cargo test print_orphans -- --ignored --nocapture (doar citire)
    #[test]
    #[ignore]
    fn print_orphans() {
        for o in super::orphans() {
            println!("{} | {:.1} MB | {} zile | probabil={}", o.path, o.size_bytes as f64 / 1e6, o.days_unused, o.likely);
        }
    }

    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(norm("7-Zip 23.01 (x64)"), "7zip");
        assert_eq!(norm("Notepad++ (64-bit x64)"), "notepad++");
        assert_eq!(norm("VLC media player"), "vlcmediaplayer");
        assert_eq!(norm_publisher("Igor Pavlov"), "igorpavlov");
        assert_eq!(norm_publisher("Mozilla Corporation"), "mozilla");
    }

    #[test]
    fn commands() {
        assert_eq!(split_command("\"C:\\P\\unins000.exe\"", None).unwrap(), ("C:\\P\\unins000.exe".into(), "".into()));
        assert_eq!(split_command("C:\\P\\u.exe /S", None).unwrap(), ("C:\\P\\u.exe".into(), "/S".into()));
        assert_eq!(split_command("MsiExec.exe /I{ABC-1}", None).unwrap(), ("msiexec.exe".into(), "/X{ABC-1}".into()));
    }

    #[test]
    fn quarantine_paths_are_restricted() {
        let pf = std::env::var("ProgramFiles").unwrap();
        assert!(check_quarantine_path(&format!("{pf}\\SomeApp")).is_ok());
        assert!(check_quarantine_path(&format!("{pf}\\Vendor\\SomeApp")).is_ok());
        assert!(check_quarantine_path(&format!("{pf}\\A\\B\\C")).is_err(), "prea adânc");
        assert!(check_quarantine_path(&pf).is_err(), "rădăcina însăși");
        assert!(check_quarantine_path(&format!("{pf}\\Common Files")).is_err());
        assert!(check_quarantine_path(&format!("{pf}\\Microsoft")).is_err());
        assert!(check_quarantine_path("C:\\Windows\\System32").is_err());
        assert!(check_quarantine_path("C:\\Users").is_err());
    }

    #[test]
    fn quarantine_roundtrip_via_engine() {
        // Folder fals în %LOCALAPPDATA%: mutare în carantină + anulare.
        let la = std::env::var("LOCALAPPDATA").unwrap();
        let dir = format!("{la}\\PioCareTestLeftover{}", std::process::id());
        std::fs::create_dir_all(format!("{dir}\\sub")).unwrap();
        std::fs::write(format!("{dir}\\sub\\a.txt"), b"x").unwrap();
        let root = std::env::temp_dir().join(format!("piocare-q-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let plan = crate::engine::Plan { title: "t".into(), rule_id: None, actions: vec![ActionSpec::Quarantine { path: dir.clone() }] };
        let op = crate::engine::execute(&root, crate::engine::Request::Apply { plan }).unwrap();
        assert_eq!(op.status, crate::journal::OpStatus::Applied, "{:?}", op);
        assert!(!Path::new(&dir).exists());
        let op = crate::engine::execute(&root, crate::engine::Request::Undo { id: op.id }).unwrap();
        assert_eq!(op.status, crate::journal::OpStatus::Undone);
        assert!(Path::new(&format!("{dir}\\sub\\a.txt")).exists());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// cargo test print_programs -- --ignored --nocapture  (doar citire)
    #[test]
    #[ignore]
    fn print_programs() {
        let all = list();
        println!("{} programe", all.len());
        for p in all.iter().take(400) {
            let left = leftovers(p, &all);
            println!("{} | {} | {} | resturi acum: {}", p.name, p.publisher, p.install_location, left.len());
            for l in left.iter().take(6) {
                println!("     [{}] {}", l.kind, l.location);
            }
        }
    }
}
