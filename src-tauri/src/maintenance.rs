//! Modulul „Întreținere generală”: fișiere temporare, cache-uri, rapoarte de erori, Coș, DISM.
//! Reguli de siguranță:
//! - se lucrează DOAR în folderele din rules/maintenance.json;
//! - legăturile (symlink/junction) nu se urmează și nu se șterg;
//! - fișierele în uz sunt sărite, nu forțate;
//! - cache-ul unui browser se curăță doar dacă browserul e închis.
//! Curățarea nu are anulare (fișierele sunt refăcute automat); jurnalul o spune clar.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const RULES: &str = include_str!("../../rules/maintenance.json");

#[derive(Deserialize, Clone)]
struct Item {
    id: String,
    title: crate::i18n::Text,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default = "star")]
    pattern: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    min_age_days: u64,
    admin: bool,
    risk: String,
    #[serde(default)]
    default: bool,
    #[serde(default)]
    requires_closed: Vec<String>,
    special: Option<String>,
    why: crate::i18n::Text,
}

fn star() -> String {
    "*".into()
}

#[derive(Deserialize)]
struct Rules {
    schema_version: u32,
    version: String,
    items: Vec<Item>,
}

fn rules() -> Result<Rules, String> {
    let r: Rules = serde_json::from_str(RULES).map_err(|e| format!("rules/maintenance.json: {e}"))?;
    if r.schema_version > crate::rules::SUPPORTED_SCHEMA {
        return Err("pachetul de reguli pentru întreținere cere o versiune mai nouă a aplicației".into());
    }
    Ok(r)
}

#[derive(Serialize, Clone, Debug)]
pub struct ScanItem {
    pub id: String,
    pub title: String,
    pub why: String,
    pub risk: String,
    pub admin: bool,
    pub default: bool,
    /// None = nu se poate măsura (fără administrator sau operație specială).
    pub bytes: Option<u64>,
    pub files: Option<u64>,
    /// Motiv pentru care nu se poate curăța acum (ex. browser deschis).
    pub blocked: Option<String>,
}

#[derive(Serialize)]
pub struct Scan {
    pub rules_version: String,
    pub items: Vec<ScanItem>,
    pub total_bytes: u64,
}

fn expand(s: &str) -> String {
    let mut out = s.to_string();
    for var in ["TEMP", "LOCALAPPDATA", "APPDATA", "ProgramData", "SystemRoot", "USERPROFILE"] {
        if let Ok(v) = std::env::var(var) {
            out = out.replace(&format!("%{var}%"), &v);
        }
    }
    out
}

/// Extinde segmentele „*” din cale (ex. profilurile browserelor).
fn resolve(pattern: &str) -> Vec<PathBuf> {
    let p = expand(pattern);
    if p.contains('%') {
        return vec![]; // variabilă necunoscută: nu ghicim
    }
    let mut bases = vec![PathBuf::new()];
    for (i, seg) in p.split('\\').enumerate() {
        if seg == "*" {
            let mut next = vec![];
            for b in &bases {
                if let Ok(rd) = std::fs::read_dir(b) {
                    for e in rd.flatten() {
                        if e.file_type().map_or(false, |t| t.is_dir() && !t.is_symlink()) && !is_reparse(&e.path()) {
                            next.push(e.path());
                        }
                    }
                }
            }
            bases = next;
        } else {
            for b in bases.iter_mut() {
                if i == 0 {
                    *b = PathBuf::from(format!("{seg}\\"));
                } else {
                    b.push(seg);
                }
            }
        }
    }
    bases.into_iter().filter(|b| b.is_dir()).collect()
}

#[cfg(windows)]
fn is_reparse(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(p).map_or(true, |m| m.file_attributes() & 0x400 != 0)
}

#[cfg(not(windows))]
fn is_reparse(p: &Path) -> bool {
    std::fs::symlink_metadata(p).map_or(true, |m| m.file_type().is_symlink())
}

fn matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.to_lowercase(), name.to_lowercase());
    match p.split_once('*') {
        None => p == n,
        Some((pre, post)) => n.len() >= pre.len() + post.len() && n.starts_with(pre) && n.ends_with(post),
    }
}

/// Parcurge fișierele eligibile; `act` primește (cale, mărime). Întoarce (fișiere, octeți).
fn walk(dir: &Path, item: &Item, cutoff: SystemTime, act: &mut dyn FnMut(&Path, u64) -> bool) -> (u64, u64) {
    let (mut files, mut bytes) = (0u64, 0u64);
    let Ok(rd) = std::fs::read_dir(dir) else { return (0, 0) };
    for e in rd.flatten() {
        let path = e.path();
        if is_reparse(&path) {
            continue; // nu urmăm și nu ștergem legături
        }
        let Ok(m) = std::fs::symlink_metadata(&path) else { continue };
        if m.is_dir() {
            if item.recursive {
                let (f, b) = walk(&path, item, cutoff, act);
                files += f;
                bytes += b;
            }
        } else if m.is_file() && matches(&item.pattern, &e.file_name().to_string_lossy()) {
            let old_enough = m.modified().map_or(false, |t| t <= cutoff);
            if old_enough && act(&path, m.len()) {
                files += 1;
                bytes += m.len();
            }
        }
    }
    (files, bytes)
}

/// Șterge folderele goale rămase (nu și rădăcina).
fn prune(dir: &Path, root: bool) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if !is_reparse(&p) && p.is_dir() {
            prune(&p, false);
        }
    }
    if !root {
        let _ = std::fs::remove_dir(dir); // reușește doar dacă e gol
    }
}

fn cutoff(item: &Item) -> SystemTime {
    SystemTime::now() - Duration::from_secs(item.min_age_days * 86_400)
}

fn running_processes() -> Vec<String> {
    crate::sys::run_hidden("tasklist.exe", &["/fo", "csv", "/nh"])
        .map(|s| {
            s.lines()
                .filter_map(|l| l.split(',').next())
                .map(|n| n.trim_matches('"').trim_end_matches(".exe").to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn blocked_reason(item: &Item, procs: &[String]) -> Option<String> {
    let open: Vec<&String> = item.requires_closed.iter().filter(|p| procs.contains(&p.to_lowercase())).collect();
    if open.is_empty() {
        None
    } else {
        let list = open.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        Some(crate::i18n::t(
            &format!("browserul rulează ({list}); închide-l complet, inclusiv din zona de lângă ceas (unele browsere, ca Edge, rămân pornite în fundal)"),
            &format!("the browser is running ({list}); close it completely, including from the notification area (some browsers, like Edge, keep running in the background)"),
        ))
    }
}

/// Operațiile din carantina PioCare mai vechi de 7 zile (pe toate discurile).
fn quarantine_old_dirs() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![];
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(l).join("PioCare\\carantina"));
    }
    if let Ok(p) = std::env::var("ProgramData") {
        roots.push(PathBuf::from(p).join("PioCare\\carantina"));
    }
    for d in b'D'..=b'Z' {
        roots.push(PathBuf::from(format!("{}:\\.piocare-carantina", d as char)));
    }
    let cutoff = SystemTime::now() - Duration::from_secs(7 * 86_400);
    roots
        .iter()
        .filter_map(|r| std::fs::read_dir(r).ok())
        .flat_map(|rd| rd.flatten())
        .filter(|e| !is_reparse(&e.path()) && e.path().is_dir())
        .filter(|e| e.metadata().and_then(|m| m.modified()).map_or(false, |t| t <= cutoff))
        .map(|e| e.path())
        .collect()
}

fn tree_size(p: &Path) -> (u64, u64) {
    let (mut f, mut b) = (0u64, 0u64);
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if is_reparse(&path) {
                continue;
            }
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.is_dir() => {
                    let (ff, bb) = tree_size(&path);
                    f += ff;
                    b += bb;
                }
                Ok(m) => {
                    f += 1;
                    b += m.len();
                }
                Err(_) => {}
            }
        }
    }
    (f, b)
}

#[cfg(windows)]
fn recycle_bin_size() -> Option<(u64, u64)> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info: SHQUERYRBINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr == 0 { Some((info.i64NumItems as u64, info.i64Size as u64)) } else { None }
}

#[cfg(windows)]
pub fn scan() -> Result<Scan, String> {
    let r = rules()?;
    let procs = running_processes();
    let mut items = vec![];
    for it in &r.items {
        let mut si = ScanItem {
            id: it.id.clone(),
            title: it.title.get(),
            why: it.why.get(),
            risk: it.risk.clone(),
            admin: it.admin,
            default: it.default,
            bytes: None,
            files: None,
            blocked: blocked_reason(it, &procs),
        };
        match it.special.as_deref() {
            Some("recycle_bin") => {
                if let Some((n, b)) = recycle_bin_size() {
                    si.files = Some(n);
                    si.bytes = Some(b);
                }
            }
            Some("quarantine_old") => {
                let (mut f, mut b) = (0, 0);
                for d in quarantine_old_dirs() {
                    let (ff, bb) = tree_size(&d);
                    f += ff;
                    b += bb;
                }
                si.files = Some(f);
                si.bytes = Some(b);
            }
            Some(_) => {}
            None => {
                let dirs: Vec<PathBuf> = it.paths.iter().flat_map(|p| resolve(p)).collect();
                if dirs.is_empty() && it.paths.iter().any(|p| expand(p).contains("Edge") || expand(p).contains("Chrome") || expand(p).contains("Mozilla") || expand(p).contains("Brave") || expand(p).contains("Opera")) {
                    continue; // browserul nu e instalat: nu-l arătăm deloc
                }
                let mut readable = !dirs.is_empty();
                let (mut f, mut b) = (0, 0);
                for d in &dirs {
                    if std::fs::read_dir(d).is_err() {
                        readable = false;
                        continue;
                    }
                    let (ff, bb) = walk(d, it, cutoff(it), &mut |_, _| true);
                    f += ff;
                    b += bb;
                }
                if readable || (!it.admin && dirs.is_empty()) {
                    si.files = Some(f);
                    si.bytes = Some(b);
                }
            }
        }
        items.push(si);
    }
    let total_bytes = items.iter().filter(|i| i.blocked.is_none()).filter_map(|i| i.bytes).sum();
    Ok(Scan { rules_version: r.version, items, total_bytes })
}

pub fn needs_admin(ids: &[String]) -> Result<bool, String> {
    Ok(rules()?.items.iter().any(|i| i.admin && ids.contains(&i.id)))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CleanResult {
    pub id: String,
    pub title: String,
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub note: Option<String>,
}

/// Rulează curățarea pentru categoriile alese. Apelat de motor (eventual în procesul ridicat).
#[cfg(windows)]
pub fn clean(ids: &[String]) -> Result<Vec<CleanResult>, String> {
    let r = rules()?;
    let procs = running_processes();
    let mut out = vec![];
    for it in r.items.iter().filter(|i| ids.contains(&i.id)) {
        let mut res = CleanResult { id: it.id.clone(), title: it.title.get(), files: 0, bytes: 0, skipped: 0, note: None };
        if let Some(why) = blocked_reason(it, &procs) {
            res.note = Some(crate::i18n::t(&format!("sărit: {why}"), &format!("skipped: {why}")));
            out.push(res);
            continue;
        }
        match it.special.as_deref() {
            Some("recycle_bin") => {
                use windows_sys::Win32::UI::Shell::SHEmptyRecycleBinW;
                let before = recycle_bin_size();
                // SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND
                let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), 0x1 | 0x2 | 0x4) };
                if let Some((n, b)) = before {
                    if hr == 0 {
                        res.files = n;
                        res.bytes = b;
                    }
                }
                if hr != 0 && before.map_or(false, |(n, _)| n > 0) {
                    res.note = Some(crate::i18n::t(&format!("Windows a refuzat golirea (cod {hr:#x})"), &format!("Windows refused to empty it (code {hr:#x})")));
                }
            }
            Some("delivery_optimization") => {
                match crate::sys::run_hidden(
                    "powershell.exe",
                    &["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; Delete-DeliveryOptimizationCache -Force"],
                ) {
                    Ok(_) => res.note = Some(crate::i18n::t("făcut", "done")),
                    Err(e) => res.note = Some(crate::i18n::t(&format!("eroare: {e}"), &format!("error: {e}"))),
                }
            }
            Some("component_cleanup") => {
                match crate::sys::run_hidden("Dism.exe", &["/Online", "/Cleanup-Image", "/StartComponentCleanup", "/Quiet"]) {
                    Ok(_) => res.note = Some(crate::i18n::t("făcut (DISM)", "done (DISM)")),
                    Err(e) => res.note = Some(crate::i18n::t(&format!("eroare DISM: {e}"), &format!("DISM error: {e}"))),
                }
            }
            Some("quarantine_old") => {
                for d in quarantine_old_dirs() {
                    let (f, b) = tree_size(&d);
                    if std::fs::remove_dir_all(&d).is_ok() {
                        res.files += f;
                        res.bytes += b;
                    } else {
                        res.skipped += 1;
                    }
                }
            }
            Some(other) => res.note = Some(format!("operație necunoscută: {other}")),
            None => {
                for d in it.paths.iter().flat_map(|p| resolve(p)) {
                    let mut skipped = 0u64;
                    let (f, b) = walk(&d, it, cutoff(it), &mut |path, _| {
                        if std::fs::remove_file(path).is_ok() {
                            true
                        } else {
                            skipped += 1;
                            false
                        }
                    });
                    res.files += f;
                    res.bytes += b;
                    res.skipped += skipped;
                    if it.recursive {
                        prune(&d, true);
                    }
                }
            }
        }
        out.push(res);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_matching() {
        assert!(matches("*", "x.tmp"));
        assert!(matches("thumbcache_*.db", "thumbcache_256.db"));
        assert!(!matches("thumbcache_*.db", "iconcache_256.db"));
        assert!(matches("*.dmp", "MINI1.DMP"));
    }

    #[test]
    fn rules_parse() {
        let r = rules().unwrap();
        assert!(r.items.len() >= 10);
        for it in &r.items {
            assert!(it.special.is_some() || !it.paths.is_empty(), "{}", it.id);
            for p in &it.paths {
                assert!(p.starts_with('%'), "{}: căile trebuie să pornească de la o variabilă Windows", it.id);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn cleans_only_old_files_and_skips_links() {
        let base = std::env::temp_dir().join(format!("piocare-maint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("sub")).unwrap();
        std::fs::write(base.join("vechi.tmp"), b"1234").unwrap();
        std::fs::write(base.join("sub").join("vechi2.tmp"), b"12").unwrap();
        // Fișier „nou”: cutoff în trecut îl exclude.
        let item = Item {
            id: "t".into(),
            title: "t".into(),
            paths: vec![],
            pattern: "*".into(),
            recursive: true,
            min_age_days: 0,
            admin: false,
            risk: "low".into(),
            default: true,
            requires_closed: vec![],
            special: None,
            why: Default::default(),
        };
        let past = SystemTime::now() - Duration::from_secs(3600);
        let (f, _) = walk(&base, &item, past, &mut |_, _| true);
        assert_eq!(f, 0, "fișierele noi nu trebuie numărate");
        let future = SystemTime::now() + Duration::from_secs(60);
        let (f, b) = walk(&base, &item, future, &mut |p, _| std::fs::remove_file(p).is_ok());
        assert_eq!((f, b), (2, 6));
        prune(&base, true);
        assert!(base.exists() && !base.join("sub").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// cargo test print_maintenance -- --ignored --nocapture
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn print_maintenance() {
        let s = scan().unwrap();
        for i in &s.items {
            println!("{:<45} {:>10} {:?}", i.title, i.bytes.map_or("?".into(), |b| format!("{:.1} MB", b as f64 / 1e6)), i.blocked);
        }
        println!("total: {:.1} MB", s.total_bytes as f64 / 1e6);
    }
}
