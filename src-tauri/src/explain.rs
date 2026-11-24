//! „Ce este?”: explicații pe înțelesul oricui pentru servicii și programe.
//! Sursa 1: baza PioCare (rules/explain.json, RO/EN, se actualizează cu pachetele de reguli).
//! Sursa 2, mereu disponibilă: descrierea oficială din Windows + producătorul fișierului.

use crate::i18n::Text;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


#[derive(Deserialize, Clone)]
struct Entry {
    verdict: String,
    what: Text,
    off: Text,
}

#[derive(Deserialize)]
struct Base {
    schema_version: u32,
    version: String,
    services: HashMap<String, Entry>,
    programs: HashMap<String, Entry>,
}

fn base() -> Result<Base, String> {
    let b: Base = serde_json::from_str(&crate::rules::text("explain")).map_err(|e| format!("rules/explain.json: {e}"))?;
    if b.schema_version > crate::rules::SUPPORTED_SCHEMA {
        return Err("explain.json: schema".into());
    }
    Ok(b)
}

fn find<'a>(m: &'a HashMap<String, Entry>, key: &str) -> Option<&'a Entry> {
    m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
}

#[derive(Serialize, Default)]
pub struct Explanation {
    pub kind: String,
    pub name: String,
    /// vital / keep / optional / safe_off / unknown
    pub verdict: String,
    pub what: Option<String>,
    pub off: Option<String>,
    /// Textul vine din baza PioCare (true) sau doar din Windows (false).
    pub known: bool,
    pub windows_description: Option<String>,
    pub path: Option<String>,
    pub company: Option<String>,
    pub file_description: Option<String>,
    pub base_version: String,
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Rezolvă texte de tip „@%SystemRoot%\system32\x.dll,-123” în textul real, în limba Windows.
#[cfg(windows)]
fn indirect(s: &str) -> String {
    if !s.starts_with('@') {
        return s.to_string();
    }
    use windows_sys::Win32::UI::Shell::SHLoadIndirectString;
    let src = wide(s);
    let mut buf = vec![0u16; 1024];
    let hr = unsafe { SHLoadIndirectString(src.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, std::ptr::null_mut()) };
    if hr != 0 {
        return String::new();
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf16_lossy(&buf[..n])
}

/// Producătorul și descrierea dintr-un fișier .exe/.dll (resursa VERSIONINFO).
#[cfg(windows)]
fn version_info(path: &str) -> (Option<String>, Option<String>) {
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let p = wide(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(p.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return (None, None);
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(p.as_ptr(), 0, size, data.as_mut_ptr() as *mut _) == 0 {
            return (None, None);
        }
        // Prima traducere disponibilă (limbă + cod de pagină).
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let tr = wide("\\VarFileInfo\\Translation");
        let code = if VerQueryValueW(data.as_ptr() as *const _, tr.as_ptr(), &mut ptr, &mut len) != 0 && len >= 4 {
            let w = std::slice::from_raw_parts(ptr as *const u16, 2);
            format!("{:04x}{:04x}", w[0], w[1])
        } else {
            "040904b0".into()
        };
        let read = |field: &str| -> Option<String> {
            let q = wide(&format!("\\StringFileInfo\\{code}\\{field}"));
            let mut p2: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut l2 = 0u32;
            if VerQueryValueW(data.as_ptr() as *const _, q.as_ptr(), &mut p2, &mut l2) != 0 && l2 > 0 {
                let s = std::slice::from_raw_parts(p2 as *const u16, l2 as usize);
                let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                let v = String::from_utf16_lossy(&s[..n]).trim().to_string();
                if v.is_empty() { None } else { Some(v) }
            } else {
                None
            }
        };
        (read("CompanyName"), read("FileDescription"))
    }
}

fn expand_path(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if let Some(q) = s.strip_prefix('"') {
        s = q.split('"').next().unwrap_or("").to_string();
    } else if let Some(i) = s.to_lowercase().find(".exe") {
        s.truncate(i + 4);
    }
    for var in ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
        if let Ok(v) = std::env::var(var) {
            s = s.replace(&format!("%{var}%"), &v).replace(&format!("%{}%", var.to_lowercase()), &v);
        }
    }
    if let Some(rest) = s.strip_prefix("\\SystemRoot\\") {
        s = format!("{}\\{rest}", std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()));
    }
    if s.to_lowercase().starts_with("system32\\") {
        s = format!("{}\\{s}", std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()));
    }
    Some(s)
}

#[cfg(windows)]
pub fn service(name: &str) -> Result<Explanation, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let b = base()?;
    let mut e = Explanation { kind: "service".into(), name: name.into(), base_version: b.version.clone(), ..Default::default() };
    if let Ok(k) = winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!("SYSTEM\\CurrentControlSet\\Services\\{name}")) {
        let desc: String = k.get_value("Description").unwrap_or_default();
        let d = indirect(&desc);
        if !d.is_empty() {
            e.windows_description = Some(d);
        }
        // Pentru serviciile găzduite de svchost, fișierul relevant e DLL-ul din Parameters\ServiceDll.
        let dll: Option<String> = k.open_subkey("Parameters").ok().and_then(|p| p.get_value("ServiceDll").ok());
        let image: String = k.get_value("ImagePath").unwrap_or_default();
        e.path = dll.and_then(|d| expand_path(&d)).or_else(|| expand_path(&image));
    }
    if let Some(p) = &e.path {
        let (c, d) = version_info(p);
        e.company = c;
        e.file_description = d;
    }
    match find(&b.services, name) {
        Some(x) => {
            e.known = true;
            e.verdict = x.verdict.clone();
            e.what = Some(x.what.get());
            e.off = Some(x.off.get());
        }
        None => {
            if crate::services::is_critical(name) {
                e.verdict = "vital".into();
            } else if let Some((why, risk, feats)) = crate::services::rule_info(name) {
                // Nu e în baza de explicații, dar e în regulile de servicii: folosim acea explicație.
                e.known = true;
                e.verdict = if feats.is_empty() && risk == "low" { "safe_off".into() } else { "optional".into() };
                e.what = Some(why);
                e.off = Some(if feats.is_empty() {
                    crate::i18n::t("Nimic vizibil în folosirea obișnuită a PC-ului.", "Nothing visible in normal PC use.")
                } else {
                    crate::i18n::t(
                        &format!("Nu vor mai merge: {}.", feats.join("; ")),
                        &format!("These will stop working: {}.", feats.join("; ")),
                    )
                });
            } else {
                e.verdict = "unknown".into();
            }
        }
    }
    Ok(e)
}

#[cfg(windows)]
pub fn program(name: &str, path: Option<String>) -> Result<Explanation, String> {
    let b = base()?;
    let stem = name.trim().trim_end_matches(".exe").to_lowercase();
    let mut e = Explanation { kind: "program".into(), name: name.into(), base_version: b.version.clone(), path: path.clone(), ..Default::default() };
    if let Some(p) = &path {
        let (c, d) = version_info(p);
        e.company = c;
        e.file_description = d;
    }
    match find(&b.programs, &stem) {
        Some(x) => {
            e.known = true;
            e.verdict = x.verdict.clone();
            e.what = Some(x.what.get());
            e.off = Some(x.off.get());
        }
        None => e.verdict = "unknown".into(),
    }
    Ok(e)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn base_parses() {
        let b = base().unwrap();
        assert!(b.services.len() > 30 && b.programs.len() > 15);
        for (k, v) in b.services.iter().chain(b.programs.iter()) {
            assert!(["vital", "keep", "optional", "safe_off"].contains(&v.verdict.as_str()), "{k}");
        }
    }

    #[test]
    fn windows_fallback_works() {
        let e = service("Dnscache").unwrap();
        assert!(e.known && e.verdict == "vital");
        assert!(e.windows_description.is_some(), "descrierea Windows lipsește");
        let e = service("Netlogon").unwrap();
        assert_eq!(e.verdict, "vital"); // din lista de servicii vitale, chiar fără text în bază
        let p = program("notepad.exe", Some("C:\\Windows\\System32\\notepad.exe".into())).unwrap();
        assert!(p.company.unwrap_or_default().contains("Microsoft"));
    }
}
