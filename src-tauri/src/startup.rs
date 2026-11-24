//! Modulul „Programe la pornire”: tot ce pornește odată cu Windows.
//! Surse: chei Run (utilizator, sistem, sistem 32 biți), foldere Startup, sarcini programate
//! la autentificare/pornire (în afara celor Microsoft).
//! Dezactivarea se face EXACT ca în Task Manager (StartupApproved), deci nimic nu se șterge;
//! pentru sarcini: Enable/Disable-ScheduledTask. Totul prin motor, cu anulare.

use crate::actions::{ActionSpec, RegData, Root};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const APPROVED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved";

#[derive(Serialize, Clone, Debug)]
pub struct StartupItem {
    pub id: String,
    /// run_user / run_machine / run_machine32 / folder_user / folder_common / task
    pub source: String,
    pub name: String,
    pub command: String,
    pub path: Option<String>,
    pub file_name: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub missing: bool,
    pub needs_admin: bool,
    /// Componentă de securitate Windows: nu se dezactivează din PioCare.
    pub protected: bool,
    pub disable: ActionSpec,
    pub enable: ActionSpec,
}

/// Valoarea StartupApproved: primul octet par = pornit (02/06), impar = oprit (03/07).
fn approved_value(enabled: bool) -> RegData {
    let mut b = vec![0u8; 12];
    if enabled {
        b[0] = 0x02;
    } else {
        b[0] = 0x03;
        // Ca Task Manager: momentul dezactivării (FILETIME), doar informativ.
        let ft = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_nanos() / 100) as u64 + 116_444_736_000_000_000)
            .unwrap_or(0);
        b[4..12].copy_from_slice(&ft.to_le_bytes());
    }
    RegData { kind: 3, hex: crate::actions::to_hex(&b) }
}

#[cfg(windows)]
fn hk(root: Root) -> winreg::RegKey {
    use winreg::enums::*;
    winreg::RegKey::predef(if root == Root::HKCU { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE })
}

#[cfg(windows)]
fn is_enabled(root: Root, sub: &str, value: &str) -> bool {
    hk(root)
        .open_subkey(format!("{APPROVED}\\{sub}"))
        .and_then(|k| k.get_raw_value(value))
        .map(|v| v.bytes.first().map_or(true, |b| b & 1 == 0))
        .unwrap_or(true) // fără înregistrare = pornit
}

#[cfg(windows)]
fn file_info(path: &Option<String>) -> (Option<String>, Option<String>, bool) {
    match path {
        None => (None, None, false),
        Some(p) => {
            let missing = crate::cleaner::file_state(p) == crate::cleaner::FileState::Missing;
            let e = crate::explain::program(&PathBuf::from(p).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(), Some(p.clone()));
            let (c, d) = e.map(|e| (e.company, e.file_description)).unwrap_or((None, None));
            (c, d, missing)
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskRow {
    task_path: String,
    task_name: String,
    state: String,
    #[serde(default)]
    exec: Option<String>,
    #[serde(default)]
    args: Option<String>,
}

#[cfg(windows)]
fn tasks() -> Vec<TaskRow> {
    const PS: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$r = @(Get-ScheduledTask | Where-Object {
  $_.TaskPath -notlike '\Microsoft\*' -and
  ($_.Triggers | Where-Object { $_.CimClass.CimClassName -in 'MSFT_TaskLogonTrigger','MSFT_TaskBootTrigger' })
} | ForEach-Object {
  $a = $_.Actions | Select-Object -First 1
  [pscustomobject]@{ TaskPath = $_.TaskPath; TaskName = $_.TaskName; State = [string]$_.State; Exec = [string]$a.Execute; Args = [string]$a.Arguments }
})
ConvertTo-Json -InputObject $r -Compress
"#;
    crate::sys::run_hidden("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PS])
        .ok()
        .and_then(|s| serde_json::from_str(s.trim()).ok())
        .unwrap_or_default()
}

#[cfg(windows)]
pub fn list() -> Vec<StartupItem> {
    let mut out = vec![];

    // 1. Chei Run.
    for (source, root, path, approved_sub) in [
        ("run_user", Root::HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\Run", "Run"),
        ("run_machine", Root::HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run", "Run"),
        ("run_machine32", Root::HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run", "Run32"),
    ] {
        let Ok(k) = hk(root).open_subkey(path) else { continue };
        for (name, _) in k.enum_values().flatten() {
            if name.is_empty() {
                continue;
            }
            let cmd: String = k.get_value(&name).unwrap_or_default();
            let exe = crate::cleaner::exe_from_command(&cmd);
            let (publisher, description, missing) = file_info(&exe);
            let ap = format!("{APPROVED}\\{approved_sub}");
            out.push(StartupItem {
                id: format!("{source}|{name}"),
                source: source.into(),
                enabled: is_enabled(root, approved_sub, &name),
                file_name: exe.as_ref().and_then(|p| PathBuf::from(p).file_name().map(|f| f.to_string_lossy().into_owned())),
                command: cmd,
                path: exe,
                publisher,
                description,
                missing,
                needs_admin: root == Root::HKLM,
                protected: false,
                disable: ActionSpec::RegSetValue { root, path: ap.clone(), name: name.clone(), value: approved_value(false) },
                enable: ActionSpec::RegSetValue { root, path: ap, name: name.clone(), value: approved_value(true) },
                name,
            });
        }
    }

    // 2. Foldere Startup.
    let folders = [
        ("folder_user", Root::HKCU, std::env::var("APPDATA").ok().map(|a| PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup"))),
        ("folder_common", Root::HKLM, std::env::var("ProgramData").ok().map(|a| PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs\\StartUp"))),
    ];
    for (source, root, dir) in folders {
        let Some(dir) = dir else { continue };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            if file.eq_ignore_ascii_case("desktop.ini") || !e.path().is_file() {
                continue;
            }
            let full = e.path().to_string_lossy().into_owned();
            let ap = format!("{APPROVED}\\StartupFolder");
            let target = if file.to_lowercase().ends_with(".lnk") { None } else { Some(full.clone()) };
            let (publisher, description, _) = file_info(&target);
            out.push(StartupItem {
                id: format!("{source}|{file}"),
                source: source.into(),
                name: file.trim_end_matches(".lnk").to_string(),
                command: full.clone(),
                path: Some(full),
                file_name: Some(file.clone()),
                publisher,
                description,
                enabled: is_enabled(root, "StartupFolder", &file),
                missing: false,
                needs_admin: root == Root::HKLM,
                protected: false,
                disable: ActionSpec::RegSetValue { root, path: ap.clone(), name: file.clone(), value: approved_value(false) },
                enable: ActionSpec::RegSetValue { root, path: ap, name: file, value: approved_value(true) },
            });
        }
    }

    // 3. Sarcini programate la autentificare / pornire (fără cele Microsoft).
    for t in tasks() {
        let exe = t.exec.as_deref().filter(|s| !s.is_empty()).map(|s| {
            let s = s.trim_matches('"');
            crate::cleaner::exe_from_command(&format!("\"{s}\"")).unwrap_or_else(|| s.to_string())
        });
        let (publisher, description, missing) = file_info(&exe.clone().filter(|p| p.contains(':')));
        let full = format!("{}{}", t.task_path, t.task_name);
        out.push(StartupItem {
            id: format!("task|{full}"),
            source: "task".into(),
            name: t.task_name.clone(),
            command: format!("{} {}", t.exec.clone().unwrap_or_default(), t.args.clone().unwrap_or_default()).trim().to_string(),
            file_name: exe.as_ref().and_then(|p| PathBuf::from(p).file_name().map(|f| f.to_string_lossy().into_owned())),
            path: exe,
            publisher,
            description,
            enabled: t.state != "Disabled",
            missing,
            needs_admin: true,
            protected: false,
            disable: ActionSpec::ScheduledTask { path: t.task_path.clone(), name: t.task_name.clone(), enabled: false },
            enable: ActionSpec::ScheduledTask { path: t.task_path, name: t.task_name, enabled: true },
        });
    }

    for i in out.iter_mut() {
        let c = i.command.to_lowercase();
        i.protected = ["securityhealth", "windowsdefender"].iter().any(|n| i.name.to_lowercase().contains(n))
            || c.contains("securityhealthsystray") || c.contains("\\windows defender\\");
    }
    out.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

// ---------------- sarcini programate (pentru motor) ----------------

fn valid_task(path: &str, name: &str) -> Result<(), String> {
    let bad = |s: &str| s.is_empty() || s.contains(['"', '\'', '`', '$', '\n', '\r']);
    if bad(name) || path.contains(['"', '\'', '`', '$']) || !path.starts_with('\\') {
        return Err(crate::i18n::t("sarcină programată invalidă", "invalid scheduled task"));
    }
    if path.to_lowercase().starts_with("\\microsoft\\") {
        return Err(crate::i18n::t("sarcinile Microsoft nu se ating", "Microsoft tasks are never touched"));
    }
    Ok(())
}

fn ps(script: &str, path: &str, name: &str) -> Result<String, String> {
    crate::sys::run_hidden_env(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &format!("$ErrorActionPreference='Stop'; {script}")],
        &[("PC_TPATH", path), ("PC_TNAME", name)],
    )
}

pub fn task_enabled(path: &str, name: &str) -> Result<bool, String> {
    valid_task(path, name)?;
    let s = ps("[string](Get-ScheduledTask -TaskPath $env:PC_TPATH -TaskName $env:PC_TNAME).State", path, name)?;
    Ok(s.trim() != "Disabled")
}

pub fn set_task(path: &str, name: &str, enabled: bool) -> Result<(), String> {
    valid_task(path, name)?;
    let cmd = if enabled { "Enable-ScheduledTask" } else { "Disable-ScheduledTask" };
    ps(&format!("{cmd} -TaskPath $env:PC_TPATH -TaskName $env:PC_TNAME | Out-Null"), path, name).map(|_| ())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn approved_values() {
        let on = crate::actions::from_hex(&approved_value(true).hex).unwrap();
        let off = crate::actions::from_hex(&approved_value(false).hex).unwrap();
        assert_eq!((on.len(), on[0] & 1), (12, 0));
        assert_eq!((off.len(), off[0] & 1), (12, 1));
    }

    #[test]
    fn microsoft_tasks_refused() {
        assert!(valid_task("\\Microsoft\\Windows\\Defrag\\", "ScheduledDefrag").is_err());
        assert!(valid_task("\\", "Ok Task").is_ok());
        assert!(valid_task("\\", "x\"; rm").is_err());
    }

    #[test]
    fn disable_and_undo_user_run_entry() {
        // O intrare de test în HKCU\...\Run: dezactivare ca în Task Manager + anulare.
        use winreg::enums::HKEY_CURRENT_USER;
        let run = winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", winreg::enums::KEY_ALL_ACCESS).unwrap();
        let name = format!("PioCareTestStartup{}", std::process::id());
        run.set_value(&name, &"C:\\Windows\\notepad.exe").unwrap();
        let item = list().into_iter().find(|i| i.name == name).expect("intrarea de test lipsește");
        assert!(item.enabled);
        let root = std::env::temp_dir().join(format!("piocare-st-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let plan = crate::engine::Plan { title: "t".into(), rule_id: None, actions: vec![item.disable.clone()] };
        let op = crate::engine::execute(&root, crate::engine::Request::Apply { plan }).unwrap();
        assert!(!list().into_iter().find(|i| i.name == name).unwrap().enabled);
        crate::engine::execute(&root, crate::engine::Request::Undo { id: op.id }).unwrap();
        assert!(list().into_iter().find(|i| i.name == name).unwrap().enabled);
        let _ = run.delete_value(&name);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// cargo test print_startup -- --ignored --nocapture (doar citire)
    #[test]
    #[ignore]
    fn print_startup() {
        for i in list() {
            println!("[{}] {} | {} | pornit={} lipsă={} | {}", i.source, i.name, i.publisher.unwrap_or_default(), i.enabled, i.missing, i.command);
        }
    }
}
