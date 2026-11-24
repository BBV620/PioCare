//! Modulul „Registru curat”: găsește resturi de registru lăsate de programe dezinstalate.
//! Scanarea doar CITEȘTE. Ștergerea se face prin motor (copie + jurnal + anulare).
//!
//! Principiul de bază: raportăm o intrare doar când putem dovedi că fișierul către care
//! trimite NU mai există. Dacă o cale nu poate fi înțeleasă, dacă discul lipsește (stick USB,
//! disc de rețea) sau dacă accesul e refuzat, intrarea NU se raportează.

use crate::actions::{ActionSpec, Root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const RULES_ID: &str = "clean";

#[derive(Deserialize)]
struct Category {
    enabled: bool,
    title: crate::i18n::Text,
    why: crate::i18n::Text,
}

#[derive(Deserialize)]
struct Rules {
    schema_version: u32,
    version: String,
    categories: HashMap<String, Category>,
    #[serde(default)]
    never_touch: Vec<String>,
    #[serde(default)]
    never_touch_names: Vec<String>,
}

fn rules() -> Result<Rules, String> {
    let r: Rules = serde_json::from_str(&crate::rules::text("registry")).map_err(|e| format!("rules/registry.json: {e}"))?;
    if r.schema_version > crate::rules::SUPPORTED_SCHEMA {
        return Err("pachetul de reguli pentru registru cere o versiune mai nouă a aplicației".into());
    }
    Ok(r)
}

#[derive(Serialize, Clone, Debug)]
pub struct Finding {
    pub id: String,
    pub category: String,
    pub category_title: String,
    pub why: String,
    pub title: String,
    pub location: String,
    pub missing: String,
    pub needs_admin: bool,
    pub action: ActionSpec,
}

#[derive(Serialize)]
pub struct ScanResult {
    pub rules_version: String,
    pub scanned: usize,
    pub findings: Vec<Finding>,
}

/// Starea unui fișier: există, lipsește sigur, sau nu se poate ști.
#[derive(Debug, PartialEq)]
pub enum FileState {
    Present,
    Missing,
    Unknown,
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Extrage calea fișierului dintr-o linie de comandă de tip registru.
/// Întoarce None dacă nu e o cale absolută clară (ex. „msiexec /x …”, nume simplu).
pub fn exe_from_command(cmd: &str) -> Option<String> {
    let s = expand_env(cmd.trim());
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let path = if let Some(stripped) = s.strip_prefix('"') {
        stripped.split('"').next()?.to_string()
    } else {
        // Fără ghilimele: căutăm cea mai scurtă porțiune care se termină în .exe/.dll/etc.
        let lower = s.to_lowercase();
        let mut cut = None;
        for ext in [".exe", ".dll", ".cpl", ".com", ".bat", ".cmd", ".ocx", ".sys", ".msc", ".vbs", ".lnk"] {
            if let Some(i) = lower.find(ext) {
                let end = i + ext.len();
                if lower[end..].chars().next().map_or(true, |c| c == ' ' || c == ',' || c == '"') {
                    cut = Some(cut.map_or(end, |c: usize| c.min(end)));
                }
            }
        }
        match cut {
            Some(end) => s[..end].to_string(),
            None => s.split(',').next()?.to_string(),
        }
    };
    let p = path.trim().trim_end_matches(',');
    let lower = p.to_lowercase();
    // rundll32 <dll>,<punct>: verificăm DLL-ul, nu rundll32.
    if lower.ends_with("rundll32.exe") {
        let rest = s[s.to_lowercase().find("rundll32.exe")? + "rundll32.exe".len()..].trim_start_matches('"').trim();
        return exe_from_command(rest);
    }
    // Doar căi absolute de tip C:\...
    let b = p.as_bytes();
    if b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
        Some(p.to_string())
    } else {
        None
    }
}

pub fn file_state(path: &str) -> FileState {
    let b = path.as_bytes();
    if !(b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) {
        return FileState::Unknown;
    }
    let drive = &path[..3];
    if !std::path::Path::new(drive).exists() {
        return FileState::Unknown; // disc lipsă (USB, rețea): nu putem ști.
    }
    match std::fs::symlink_metadata(path) {
        Ok(_) => FileState::Present,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Verificăm și folderul părinte: dacă nici accesul la el nu merge, nu putem ști.
            match std::path::Path::new(path).parent().map(std::fs::symlink_metadata) {
                Some(Err(pe)) if pe.kind() != std::io::ErrorKind::NotFound => FileState::Unknown,
                _ => FileState::Missing,
            }
        }
        Err(_) => FileState::Unknown,
    }
}

fn protected(r: &Rules, text: &str) -> bool {
    let t = text.to_lowercase();
    r.never_touch.iter().any(|p| t.contains(&p.to_lowercase()))
}

fn root_name(root: Root) -> &'static str {
    if root == Root::HKCU { "HKCU" } else { "HKLM" }
}

#[cfg(windows)]
pub fn scan() -> Result<ScanResult, String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let r = rules()?;
    let on = |c: &str| r.categories.get(c).map_or(false, |x| x.enabled);
    let cat = |c: &str| r.categories.get(c).map(|x| (x.title.get(), x.why.get())).unwrap_or_default();
    let hk = |root: Root| RegKey::predef(if root == Root::HKCU { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE });
    let mut out: Vec<Finding> = vec![];
    let mut scanned = 0usize;

    let mut push = |category: &str, title: String, root: Root, location: String, missing: String, action: ActionSpec| {
        let (ct, why) = cat(category);
        out.push(Finding {
            id: format!("{category}|{}", serde_json::to_string(&action).unwrap_or_default()),
            category: category.into(),
            category_title: ct,
            why,
            title,
            location: format!("{}\\{}", root_name(root), location),
            missing,
            needs_admin: action.needs_admin(),
            action,
        });
    };

    // 1. Intrări de dezinstalare orfane.
    if on("uninstall_orphan") {
        let sources = [
            (Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
            (Root::HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
            (Root::HKCU, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ];
        for (root, base) in sources {
            let Ok(key) = hk(root).open_subkey(base) else { continue };
            for sub in key.enum_keys().flatten() {
                scanned += 1;
                let Ok(app) = key.open_subkey(&sub) else { continue };
                let name: String = app.get_value("DisplayName").unwrap_or_default();
                let system: u32 = app.get_value("SystemComponent").unwrap_or(0);
                let uninst: String = app.get_value("UninstallString").unwrap_or_default();
                let quiet: String = app.get_value("QuietUninstallString").unwrap_or_default();
                if system == 1 || uninst.is_empty() || protected(&r, &format!("{name} {uninst} {sub}")) {
                    continue;
                }
                // Dezinstalatorul (și varianta „quiet”, dacă există) trebuie să lipsească sigur.
                let Some(exe) = exe_from_command(&uninst) else { continue };
                if file_state(&exe) != FileState::Missing {
                    continue;
                }
                if let Some(q) = exe_from_command(&quiet) {
                    if file_state(&q) != FileState::Missing {
                        continue;
                    }
                }
                // Dacă folderul de instalare încă există, programul poate fi doar mutat: nu-l raportăm.
                let loc: String = app.get_value("InstallLocation").unwrap_or_default();
                let loc = expand_env(loc.trim().trim_matches('"'));
                if !loc.is_empty() && loc.len() > 3 && file_state(&loc) != FileState::Missing {
                    continue;
                }
                let path = format!("{base}\\{sub}");
                let title = if name.trim().is_empty() { sub.clone() } else { name.trim().to_string() };
                push("uninstall_orphan", title, root, path.clone(), exe, ActionSpec::RegDeleteKey { root, path });
            }
        }
    }

    // 2. Porniri automate către fișiere care nu mai există.
    if on("run_missing") {
        let sources = [
            (Root::HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            (Root::HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
            (Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"),
            (Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
            (Root::HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run"),
            (Root::HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
        ];
        for (root, base) in sources {
            let Ok(key) = hk(root).open_subkey(base) else { continue };
            for (vname, _) in key.enum_values().flatten() {
                scanned += 1;
                let cmd: String = match key.get_value(&vname) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if vname.is_empty()
                    || r.never_touch_names.iter().any(|n| n.eq_ignore_ascii_case(&vname))
                    || protected(&r, &format!("{vname} {cmd}"))
                {
                    continue;
                }
                let Some(exe) = exe_from_command(&cmd) else { continue };
                if file_state(&exe) == FileState::Missing {
                    push(
                        "run_missing",
                        vname.clone(),
                        root,
                        format!("{base} → {vname}"),
                        exe,
                        ActionSpec::RegDeleteValue { root, path: base.into(), name: vname },
                    );
                }
            }
        }
    }

    // 3. App Paths moarte.
    if on("app_paths_missing") {
        let sources = [
            // Fără WOW6432Node: App Paths e o cheie comună 64/32 biți (aceleași intrări), ar apărea dublat.
            (Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths"),
            (Root::HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\App Paths"),
        ];
        for (root, base) in sources {
            let Ok(key) = hk(root).open_subkey(base) else { continue };
            for sub in key.enum_keys().flatten() {
                scanned += 1;
                let Ok(app) = key.open_subkey(&sub) else { continue };
                let target: String = app.get_value("").unwrap_or_default();
                if target.is_empty() || protected(&r, &format!("{sub} {target}")) {
                    continue;
                }
                let Some(exe) = exe_from_command(&target) else { continue };
                if file_state(&exe) == FileState::Missing {
                    let path = format!("{base}\\{sub}");
                    push("app_paths_missing", sub.clone(), root, path.clone(), exe, ActionSpec::RegDeleteKey { root, path });
                }
            }
        }
    }

    // 4. SharedDLLs: numele valorii este chiar calea fișierului.
    if on("shared_dll_missing") {
        let base = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\SharedDLLs";
        if let Ok(key) = hk(Root::HKLM).open_subkey(base) {
            for (vname, _) in key.enum_values().flatten() {
                scanned += 1;
                if protected(&r, &vname) {
                    continue;
                }
                let Some(file) = exe_from_command(&format!("\"{vname}\"")) else { continue };
                if file_state(&file) == FileState::Missing {
                    let short = std::path::Path::new(&file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    push(
                        "shared_dll_missing",
                        short,
                        Root::HKLM,
                        format!("{base} → {vname}"),
                        file,
                        ActionSpec::RegDeleteValue { root: Root::HKLM, path: base.into(), name: vname },
                    );
                }
            }
        }
    }

    out.sort_by(|a, b| a.category.cmp(&b.category).then(a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    Ok(ScanResult { rules_version: r.version, scanned, findings: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_lines() {
        assert_eq!(exe_from_command("\"C:\\Program Files\\X\\unins000.exe\" /S").as_deref(), Some("C:\\Program Files\\X\\unins000.exe"));
        assert_eq!(exe_from_command("C:\\Program Files\\X\\app.exe --tray").as_deref(), Some("C:\\Program Files\\X\\app.exe"));
        assert_eq!(exe_from_command("MsiExec.exe /X{1234}"), None);
        assert_eq!(exe_from_command("notepad.exe"), None);
        assert_eq!(
            exe_from_command("rundll32.exe \"C:\\Prog\\x.dll\",Entry").as_deref(),
            Some("C:\\Prog\\x.dll")
        );
        assert_eq!(
            exe_from_command("C:\\Windows\\system32\\rundll32.exe C:\\Prog\\y.dll,Run").as_deref(),
            Some("C:\\Prog\\y.dll")
        );
        assert_eq!(exe_from_command("").as_deref(), None);
    }

    #[test]
    fn missing_vs_present() {
        assert_eq!(file_state("C:\\Windows\\notepad.exe"), FileState::Present);
        assert_eq!(file_state("C:\\Windows\\piocare-nu-exista-12345.exe"), FileState::Missing);
        assert_eq!(file_state("Q:\\nu\\exista.exe"), FileState::Unknown);
    }

    /// Scanare reală, doar citire: cargo test print_scan -- --ignored --nocapture
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn print_scan() {
        let s = scan().unwrap();
        println!("verificate: {}, găsite: {}", s.scanned, s.findings.len());
        for f in &s.findings {
            println!("[{}] {} | {} | lipsește: {}", f.category, f.title, f.location, f.missing);
        }
    }

    #[test]
    fn rules_parse() {
        let r = rules().unwrap();
        assert!(r.categories.len() >= 4);
    }
}
