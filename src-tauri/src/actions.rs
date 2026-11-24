//! Acțiunile elementare pe care PioCare le poate face asupra sistemului.
//! Fiecare acțiune are trei etape: `capture` (salvează starea de dinainte),
//! `apply` (face schimbarea) și `undo` (readuce starea salvată).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Root {
    HKCU,
    HKLM,
}

/// O valoare de registru păstrată exact (tip + octeți), ca anularea să fie fidelă.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RegData {
    pub kind: u32,
    pub hex: String,
}

impl RegData {
    pub fn dword(v: u32) -> Self {
        RegData { kind: 4, hex: to_hex(&v.to_le_bytes()) }
    }
    pub fn sz(s: &str) -> Self {
        let mut b: Vec<u8> = s.encode_utf16().chain(std::iter::once(0)).flat_map(|c| c.to_le_bytes()).collect();
        b.shrink_to_fit();
        RegData { kind: 1, hex: to_hex(&b) }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StartType {
    Auto,
    Delayed,
    Manual,
    Disabled,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionSpec {
    RegSetValue { root: Root, path: String, name: String, value: RegData },
    RegDeleteValue { root: Root, path: String, name: String },
    RegDeleteKey { root: Root, path: String },
    ServiceStartType { name: String, start: StartType },
    /// Protecția sistemului (puncte de restaurare) pe discul C:.
    SystemProtection { enabled: bool },
    /// Regulă de BLOCARE în Windows Firewall, creată de PioCare (nume „PioCare-…”, grup „PioCare”).
    FirewallBlock(crate::firewall::FwRule),
    /// Profil de firewall (Domain / Private / Public): pornit + intrările nesolicitate blocate.
    FirewallProfileOn { profile: String },
    /// Folder/fișier rămas de la un program dezinstalat: mutat în carantină (pe același disc), nu șters.
    Quarantine { path: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Before {
    Protection { enabled: bool },
    /// Unde a fost mutat în carantină.
    Quarantined { stored: String },
    FwRule { existed: bool },
    FwProfile { enabled: bool, inbound: String },
    /// Starea unei valori: dacă exista cheia și ce valoare era (None = nu exista).
    Value { key_existed: bool, value: Option<RegData> },
    /// Cheia întreagă, exportată cu `reg export` (fișierul e în folderul operației).
    KeyExport { file: Option<String> },
    Service { start: StartType },
}

impl ActionSpec {
    pub fn needs_admin(&self) -> bool {
        match self {
            ActionSpec::RegSetValue { root, .. }
            | ActionSpec::RegDeleteValue { root, .. }
            | ActionSpec::RegDeleteKey { root, .. } => *root == Root::HKLM,
            ActionSpec::ServiceStartType { .. }
            | ActionSpec::SystemProtection { .. }
            | ActionSpec::FirewallBlock(_)
            | ActionSpec::FirewallProfileOn { .. } => true,
            ActionSpec::Quarantine { path } => crate::uninstall::quarantine_needs_admin(path),
        }
    }

    /// Text scurt, în română, pentru interfață.
    pub fn describe(&self) -> String {
        match self {
            ActionSpec::RegSetValue { root, path, name, .. } => crate::i18n::t(&format!("Setează {root:?}\\{path} → {name}"), &format!("Set {root:?}\\{path} → {name}")),
            ActionSpec::RegDeleteValue { root, path, name } => crate::i18n::t(&format!("Șterge valoarea {root:?}\\{path} → {name}"), &format!("Delete value {root:?}\\{path} → {name}")),
            ActionSpec::RegDeleteKey { root, path } => crate::i18n::t(&format!("Șterge cheia {root:?}\\{path}"), &format!("Delete key {root:?}\\{path}")),
            ActionSpec::ServiceStartType { name, start } => crate::i18n::t(&format!("Serviciul {name}: pornire {start:?}"), &format!("Service {name}: startup {start:?}")),
            ActionSpec::SystemProtection { enabled } => {
                if *enabled { crate::i18n::t("Protecția sistemului pe C: pornită", "System Protection on C: turned on") } else { crate::i18n::t("Protecția sistemului pe C: oprită", "System Protection on C: turned off") }
            }
            ActionSpec::FirewallBlock(r) => crate::i18n::t(&format!("Firewall: blochează {}", r.display), &format!("Firewall: block {}", r.display)),
            ActionSpec::FirewallProfileOn { profile } => crate::i18n::t(&format!("Firewall: profilul {profile} pornit, intrări blocate"), &format!("Firewall: {profile} profile on, inbound blocked")),
            ActionSpec::Quarantine { path } => crate::i18n::t(&format!("Mută în carantină: {path}"), &format!("Move to quarantine: {path}")),
        }
    }
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("hex invalid".into());
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string())).collect()
}

/// Verificări de bun-simț: nu lăsăm nicio regulă să atingă rădăcini întregi.
fn check_path(path: &str) -> Result<(), String> {
    let p = path.trim_matches('\\');
    let depth = p.split('\\').filter(|s| !s.is_empty()).count();
    if p.is_empty() || depth < 2 || p.contains("..") {
        return Err(crate::i18n::t(&format!("cale de registru refuzată (prea generală): {path}"), &format!("registry path refused (too broad): {path}")));
    }
    Ok(())
}

#[cfg(windows)]
pub use win::{apply, capture, undo};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::sys::run_hidden;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    fn hkey(root: Root) -> RegKey {
        match root {
            Root::HKCU => RegKey::predef(HKEY_CURRENT_USER),
            Root::HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
        }
    }

    fn reg_path(root: Root, path: &str) -> String {
        format!("{}\\{}", if root == Root::HKCU { "HKCU" } else { "HKLM" }, path.trim_matches('\\'))
    }

    fn u32_to_type(k: u32) -> RegType {
        match k {
            1 => REG_SZ,
            2 => REG_EXPAND_SZ,
            3 => REG_BINARY,
            4 => REG_DWORD,
            5 => REG_DWORD_BIG_ENDIAN,
            6 => REG_LINK,
            7 => REG_MULTI_SZ,
            8 => REG_RESOURCE_LIST,
            9 => REG_FULL_RESOURCE_DESCRIPTOR,
            10 => REG_RESOURCE_REQUIREMENTS_LIST,
            11 => REG_QWORD,
            _ => REG_NONE,
        }
    }

    fn type_to_u32(t: &RegType) -> u32 {
        match t {
            REG_SZ => 1,
            REG_EXPAND_SZ => 2,
            REG_BINARY => 3,
            REG_DWORD => 4,
            REG_DWORD_BIG_ENDIAN => 5,
            REG_LINK => 6,
            REG_MULTI_SZ => 7,
            REG_RESOURCE_LIST => 8,
            REG_FULL_RESOURCE_DESCRIPTOR => 9,
            REG_RESOURCE_REQUIREMENTS_LIST => 10,
            REG_QWORD => 11,
            _ => 0,
        }
    }

    fn read_value(root: Root, path: &str, name: &str) -> (bool, Option<RegData>) {
        match hkey(root).open_subkey(path) {
            Err(_) => (false, None),
            Ok(k) => match k.get_raw_value(name) {
                Ok(v) => (true, Some(RegData { kind: type_to_u32(&v.vtype), hex: to_hex(&v.bytes) })),
                Err(_) => (true, None),
            },
        }
    }

    fn write_value(root: Root, path: &str, name: &str, d: &RegData) -> Result<(), String> {
        let (k, _) = hkey(root).create_subkey(path).map_err(|e| format!("{}: {e}", reg_path(root, path)))?;
        let v = RegValue { bytes: from_hex(&d.hex)?, vtype: u32_to_type(d.kind) };
        k.set_raw_value(name, &v).map_err(|e| format!("{}\\{name}: {e}", reg_path(root, path)))
    }

    fn delete_value(root: Root, path: &str, name: &str) -> Result<(), String> {
        match hkey(root).open_subkey_with_flags(path, KEY_SET_VALUE) {
            Err(_) => Ok(()),
            Ok(k) => match k.delete_value(name) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("{}\\{name}: {e}", reg_path(root, path))),
            },
        }
    }

    fn svc_key(name: &str) -> String {
        format!("SYSTEM\\CurrentControlSet\\Services\\{name}")
    }

    fn read_service(name: &str) -> Result<StartType, String> {
        if name.is_empty() || name.contains(['\\', '/']) {
            return Err("nume de serviciu invalid".into());
        }
        if crate::services::is_critical(name) {
            return Err(crate::i18n::t(&format!("{name} este un serviciu vital al Windows; PioCare nu îl schimbă"), &format!("{name} is a vital Windows service; PioCare never changes it")));
        }
        let k = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(svc_key(name))
            .map_err(|_| crate::i18n::t(&format!("serviciul {name} nu există"), &format!("service {name} does not exist")))?;
        let start: u32 = k.get_value("Start").map_err(|e| format!("{name}: {e}"))?;
        let delayed: u32 = k.get_value("DelayedAutostart").unwrap_or(0);
        Ok(match start {
            2 if delayed == 1 => StartType::Delayed,
            2 => StartType::Auto,
            3 => StartType::Manual,
            4 => StartType::Disabled,
            // 0 = boot, 1 = system: drivere. PioCare nu le atinge niciodată.
            _ => return Err(crate::i18n::t(&format!("{name} este driver (Start={start}); refuz"), &format!("{name} is a driver (Start={start}); refused"))),
        })
    }

    fn set_service(name: &str, start: StartType) -> Result<(), String> {
        use windows_sys::Win32::System::Services::*;
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let wname = wide(name);
        unsafe {
            let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
            if scm.is_null() {
                return Err(format!("nu pot deschide Service Control Manager ({})", std::io::Error::last_os_error()));
            }
            let h = OpenServiceW(scm, wname.as_ptr(), SERVICE_CHANGE_CONFIG | SERVICE_QUERY_CONFIG);
            if h.is_null() {
                let e = std::io::Error::last_os_error();
                CloseServiceHandle(scm);
                return Err(format!("serviciul {name}: {e}"));
            }
            let code: u32 = match start {
                StartType::Auto | StartType::Delayed => 2,
                StartType::Manual => 3,
                StartType::Disabled => 4,
            };
            let nc = 0xFFFF_FFFFu32; // SERVICE_NO_CHANGE
            let mut ok = ChangeServiceConfigW(
                h, nc, code, nc,
                std::ptr::null(), std::ptr::null(), std::ptr::null_mut(),
                std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
            ) != 0;
            let mut err = if ok { None } else { Some(std::io::Error::last_os_error()) };
            if ok && code == 2 {
                let info = SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: (start == StartType::Delayed) as i32 };
                ok = ChangeServiceConfig2W(h, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, &info as *const _ as *const _) != 0;
                if !ok {
                    err = Some(std::io::Error::last_os_error());
                }
            }
            CloseServiceHandle(h);
            CloseServiceHandle(scm);
            match err {
                None => Ok(()),
                Some(e) => Err(format!("serviciul {name}: {e}")),
            }
        }
    }

    pub fn capture(a: &ActionSpec, op_dir: &Path, idx: usize) -> Result<Before, String> {
        match a {
            ActionSpec::RegSetValue { root, path, name, .. } | ActionSpec::RegDeleteValue { root, path, name } => {
                check_path(path)?;
                let (key_existed, value) = read_value(*root, path, name);
                Ok(Before::Value { key_existed, value })
            }
            ActionSpec::RegDeleteKey { root, path } => {
                check_path(path)?;
                if hkey(*root).open_subkey(path).is_err() {
                    return Ok(Before::KeyExport { file: None });
                }
                let file = format!("step{idx:02}.reg");
                let full = op_dir.join(&file);
                run_hidden("reg.exe", &["export", &reg_path(*root, path), &full.to_string_lossy(), "/y"])?;
                if !full.exists() {
                    return Err(crate::i18n::t("exportul cheii nu a produs fișierul de copie", "the key export did not produce the backup file"));
                }
                Ok(Before::KeyExport { file: Some(file) })
            }
            ActionSpec::ServiceStartType { name, .. } => Ok(Before::Service { start: read_service(name)? }),
            ActionSpec::SystemProtection { .. } => Ok(Before::Protection {
                enabled: crate::sys::protection_enabled_c().ok_or("nu pot citi starea Protecției sistemului")?,
            }),
            ActionSpec::FirewallBlock(r) => {
                r.validate()?;
                Ok(Before::FwRule { existed: crate::firewall::rule_exists(&r.name) })
            }
            ActionSpec::FirewallProfileOn { profile } => {
                let p = crate::firewall::read_profile(profile)?;
                Ok(Before::FwProfile { enabled: p.enabled, inbound: p.inbound })
            }
            ActionSpec::Quarantine { path } => {
                crate::uninstall::check_quarantine_path(path)?;
                if std::fs::symlink_metadata(path).is_err() {
                    return Err(crate::i18n::t(&format!("nu mai există: {path}"), &format!("no longer exists: {path}")));
                }
                let op_id = op_dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                let stored = crate::uninstall::quarantine_target(path, &op_id, idx);
                Ok(Before::Quarantined { stored: stored.to_string_lossy().into_owned() })
            }
        }
    }

    pub fn apply(a: &ActionSpec, before: Option<&Before>) -> Result<(), String> {
        match a {
            ActionSpec::Quarantine { path } => {
                let Some(Before::Quarantined { stored }) = before else {
                    return Err("carantină fără destinație".into());
                };
                let dest = Path::new(stored);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                // Mutare pe același disc: instantanee, fără copiere. Dacă ceva e în uz, eșuează și motorul revine.
                std::fs::rename(path, dest).map_err(|e| crate::i18n::t(&format!("nu pot muta {path} (poate e în uz): {e}"), &format!("cannot move {path} (maybe in use): {e}")))
            }
            ActionSpec::RegSetValue { root, path, name, value } => write_value(*root, path, name, value),
            ActionSpec::RegDeleteValue { root, path, name } => delete_value(*root, path, name),
            ActionSpec::RegDeleteKey { root, path } => match hkey(*root).delete_subkey_all(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("{}: {e}", reg_path(*root, path))),
            },
            ActionSpec::ServiceStartType { name, start } => set_service(name, *start),
            ActionSpec::SystemProtection { enabled } => crate::sys::set_protection_c(*enabled),
            ActionSpec::FirewallBlock(r) => crate::firewall::add_rule(r),
            ActionSpec::FirewallProfileOn { profile } => crate::firewall::set_profile(profile, true, "Block"),
        }
    }

    pub fn undo(a: &ActionSpec, before: &Before, op_dir: &Path) -> Result<(), String> {
        match (a, before) {
            (
                ActionSpec::RegSetValue { root, path, name, .. } | ActionSpec::RegDeleteValue { root, path, name },
                Before::Value { key_existed, value },
            ) => {
                match value {
                    Some(v) => write_value(*root, path, name, v)?,
                    None => delete_value(*root, path, name)?,
                }
                if !key_existed {
                    // Cheia a fost creată de noi: o ștergem doar dacă a rămas goală.
                    if let Ok(k) = hkey(*root).open_subkey(path) {
                        let empty = k.enum_keys().next().is_none() && k.enum_values().next().is_none();
                        if empty {
                            let _ = hkey(*root).delete_subkey(path);
                        }
                    }
                }
                Ok(())
            }
            (ActionSpec::RegDeleteKey { .. }, Before::KeyExport { file }) => match file {
                None => Ok(()),
                Some(f) => run_hidden("reg.exe", &["import", &op_dir.join(f).to_string_lossy()]).map(|_| ()),
            },
            (ActionSpec::ServiceStartType { name, .. }, Before::Service { start }) => set_service(name, *start),
            (ActionSpec::SystemProtection { .. }, Before::Protection { enabled }) => crate::sys::set_protection_c(*enabled),
            (ActionSpec::FirewallBlock(r), Before::FwRule { existed }) => {
                if *existed { Ok(()) } else { crate::firewall::remove_rule(&r.name) }
            }
            (ActionSpec::FirewallProfileOn { profile }, Before::FwProfile { enabled, inbound }) => {
                crate::firewall::set_profile(profile, *enabled, inbound)
            }
            (ActionSpec::Quarantine { path }, Before::Quarantined { stored }) => {
                if std::fs::symlink_metadata(path).is_ok() {
                    return Err(crate::i18n::t(&format!("există deja ceva la {path}; nu suprascriu"), &format!("something already exists at {path}; not overwriting")));
                }
                if std::fs::symlink_metadata(stored).is_err() {
                    return Err(crate::i18n::t(&format!("carantina a fost golită; {path} nu mai poate fi readus"), &format!("the quarantine was emptied; {path} can no longer be restored")));
                }
                if let Some(parent) = Path::new(path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::rename(stored, path).map_err(|e| format!("{path}: {e}"))
            }
            _ => Err("starea salvată nu se potrivește cu acțiunea".into()),
        }
    }
}
