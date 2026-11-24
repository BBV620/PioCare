//! Modulul „Sarcini programate”: tot ce rulează din Task Scheduler, nu doar la pornire.
//! Sarcinile Microsoft sunt doar afișate. Sarcinile create de utilizator sunt protejate (doar dezactivare).
//! Ștergerea salvează întâi sarcina ca XML (Export-ScheduledTask), deci anularea o readuce exact.
//! Numele sarcinilor ajung la PowerShell prin variabile de mediu, niciodată în linia de comandă.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Row {
    task_path: String,
    task_name: String,
    state: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    exec: Option<String>,
    #[serde(default)]
    args: Option<String>,
    #[serde(default)]
    last_run: Option<String>,
    #[serde(default)]
    next_run: Option<String>,
    #[serde(default)]
    triggers: Option<String>,
    #[serde(default)]
    logon_type: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TaskItem {
    pub id: String,
    pub path: String,
    pub name: String,
    pub enabled: bool,
    pub microsoft: bool,
    pub user_created: bool,
    pub author: String,
    pub description: String,
    pub command: String,
    pub file: Option<String>,
    pub file_name: Option<String>,
    pub publisher: Option<String>,
    pub missing: bool,
    pub looks_like_ad: bool,
    pub can_delete: bool,
    pub last_run: String,
    pub next_run: String,
    pub triggers: String,
}

#[cfg(windows)]
fn rows() -> Vec<Row> {
    const PS: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$r = @(Get-ScheduledTask | ForEach-Object {
  $t = $_; $i = $t | Get-ScheduledTaskInfo
  $a = $t.Actions | Select-Object -First 1
  $fmt = { param($d) if ($d -and $d.Year -gt 2000) { $d.ToString('yyyy-MM-dd HH:mm') } else { '' } }
  [pscustomobject]@{
    TaskPath = $t.TaskPath; TaskName = $t.TaskName; State = [string]$t.State
    Author = [string]$t.Author; Description = [string]$t.Description
    Exec = [string]$a.Execute; Args = [string]$a.Arguments
    LastRun = (& $fmt $i.LastRunTime); NextRun = (& $fmt $i.NextRunTime)
    Triggers = (($t.Triggers | ForEach-Object { $_.CimClass.CimClassName -replace 'MSFT_Task','' -replace 'Trigger','' }) -join ',')
    LogonType = [string]$t.Principal.LogonType
  }
})
[IO.File]::WriteAllText($env:PC_OUT, (ConvertTo-Json -InputObject $r -Compress), (New-Object Text.UTF8Encoding $false))
exit 0
"#;
    // JSON-ul se scrie într-un fișier UTF-8: ieșirea consolei folosește pagina de cod OEM și strică textele.
    let tmp = std::env::temp_dir().join(format!("piocare-tasks-{}.json", std::process::id()));
    let tmp_s = tmp.to_string_lossy().into_owned();
    let res = crate::sys::run_hidden_env(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PS],
        &[("PC_OUT", &tmp_s)],
    )
    .and_then(|_| std::fs::read_to_string(&tmp).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&tmp);
    match res.and_then(|s| serde_json::from_str::<Vec<Row>>(s.trim()).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => {
            if cfg!(test) {
                eprintln!("tasks: {e}");
            }
            vec![]
        }
    }
}

const AD_WORDS: &[&str] = &["sale", "offer", "promo", "bf20", "blackfriday", "black friday", "discount", "deal", "survey", "toolbar", "rpop"];

#[cfg(windows)]
pub fn list() -> Vec<TaskItem> {
    let me = std::env::var("USERNAME").unwrap_or_default().to_lowercase();
    let mut out: Vec<TaskItem> = rows()
        .into_iter()
        .map(|r| {
            let microsoft = r.task_path.to_lowercase().starts_with("\\microsoft\\");
            let exec = r.exec.clone().unwrap_or_default();
            let file = if exec.trim().is_empty() {
                None
            } else {
                let s = exec.trim().trim_matches('"').to_string();
                let expanded = s
                    .replace("%windir%", &std::env::var("windir").unwrap_or_default())
                    .replace("%SystemRoot%", &std::env::var("SystemRoot").unwrap_or_default())
                    .replace("%ProgramFiles%", &std::env::var("ProgramFiles").unwrap_or_default())
                    .replace("%LOCALAPPDATA%", &std::env::var("LOCALAPPDATA").unwrap_or_default());
                Some(expanded)
            };
            let abs = file.clone().filter(|p| p.len() > 3 && p.as_bytes()[1] == b':');
            let missing = abs.as_ref().map_or(false, |p| crate::cleaner::file_state(p) == crate::cleaner::FileState::Missing);
            let (publisher, file_name) = match &abs {
                Some(p) if !missing && !microsoft => {
                    let fname = PathBuf::from(p).file_name().map(|f| f.to_string_lossy().into_owned());
                    let e = crate::explain::program(fname.as_deref().unwrap_or(""), Some(p.clone())).ok();
                    (e.and_then(|e| e.company), fname)
                }
                Some(p) => (None, PathBuf::from(p).file_name().map(|f| f.to_string_lossy().into_owned())),
                None => (None, None),
            };
            let author = r.author.clone().unwrap_or_default();
            let text = format!("{} {} {}", r.task_name, exec, r.args.clone().unwrap_or_default()).to_lowercase();
            let looks_like_ad = !microsoft && AD_WORDS.iter().any(|w| text.contains(w));
            // „Creată de tine”: o sarcină care rulează o unealtă/un script Windows (powershell, cmd, pnputil…)
            // sau un program fără producător — tipic pentru sarcinile făcute de mână. Nu se propune la ștergere.
            let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()).to_lowercase();
            let runs_windows_tool = abs.as_ref().map_or(false, |p| p.to_lowercase().starts_with(&windir));
            let no_publisher = publisher.as_deref().map_or(true, |p| p.trim().is_empty());
            let user_created = !microsoft && !missing && !looks_like_ad && (runs_windows_tool || (abs.is_some() && no_publisher));
            let _ = &me;
            let password = r.logon_type.as_deref().unwrap_or("").eq_ignore_ascii_case("Password");
            TaskItem {
                id: format!("{}{}", r.task_path, r.task_name),
                enabled: r.state != "Disabled",
                command: format!("{} {}", exec, r.args.clone().unwrap_or_default()).trim().to_string(),
                description: r.description.clone().unwrap_or_default(),
                last_run: r.last_run.clone().unwrap_or_default(),
                next_run: r.next_run.clone().unwrap_or_default(),
                triggers: r.triggers.clone().unwrap_or_default(),
                // Sarcinile unor programe Microsoft (ex. actualizarea Edge) se pot doar dezactiva, nu șterge.
                can_delete: !microsoft && !user_created && !password && !publisher.as_deref().unwrap_or("").to_lowercase().contains("microsoft"),
                path: r.task_path,
                name: r.task_name,
                microsoft,
                user_created,
                author,
                file: abs,
                file_name,
                publisher,
                missing,
                looks_like_ad,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.microsoft
            .cmp(&b.microsoft)
            .then((b.missing || b.looks_like_ad).cmp(&(a.missing || a.looks_like_ad)))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

// ---------------- pentru motor: ștergere cu copie XML ----------------

fn ps(script: &str, env: &[(&str, &str)]) -> Result<String, String> {
    crate::sys::run_hidden_env(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &format!("$ErrorActionPreference='Stop'; {script}")],
        env,
    )
}

/// Salvează sarcina în XML; refuză sarcinile Microsoft și pe cele care țin o parolă (nu s-ar putea readuce).
pub fn export(path: &str, name: &str, dest: &Path) -> Result<(), String> {
    crate::startup::check_task(path, name)?;
    let dest_s = dest.to_string_lossy().into_owned();
    ps(
        "Export-ScheduledTask -TaskPath $env:PC_TPATH -TaskName $env:PC_TNAME | Out-File -LiteralPath $env:PC_DEST -Encoding Unicode",
        &[("PC_TPATH", path), ("PC_TNAME", name), ("PC_DEST", &dest_s)],
    )?;
    let xml = std::fs::read(dest).map_err(|e| e.to_string())?;
    let text = String::from_utf16_lossy(&xml.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>());
    if text.contains("<LogonType>Password</LogonType>") {
        let _ = std::fs::remove_file(dest);
        return Err(crate::i18n::t(
            "sarcina folosește o parolă salvată și n-ar mai putea fi readusă; o poți doar dezactiva",
            "the task uses a stored password and could not be restored; you can only disable it",
        ));
    }
    if !text.contains("<Task") {
        return Err(crate::i18n::t("exportul sarcinii a eșuat", "the task export failed"));
    }
    Ok(())
}

pub fn delete(path: &str, name: &str) -> Result<(), String> {
    crate::startup::check_task(path, name)?;
    ps("Unregister-ScheduledTask -TaskPath $env:PC_TPATH -TaskName $env:PC_TNAME -Confirm:$false", &[("PC_TPATH", path), ("PC_TNAME", name)]).map(|_| ())
}

pub fn restore(path: &str, name: &str, xml_file: &Path) -> Result<(), String> {
    crate::startup::check_task(path, name)?;
    let f = xml_file.to_string_lossy().into_owned();
    ps(
        "Register-ScheduledTask -TaskPath $env:PC_TPATH -TaskName $env:PC_TNAME -Xml ([IO.File]::ReadAllText($env:PC_XML)) | Out-Null",
        &[("PC_TPATH", path), ("PC_TNAME", name), ("PC_XML", &f)],
    )
    .map(|_| ())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn lists_tasks_and_flags_microsoft() {
        let all = list();
        assert!(all.len() > 20, "prea puține sarcini citite: {}", all.len());
        assert!(all.iter().any(|t| t.microsoft));
        assert!(all.iter().filter(|t| t.microsoft).all(|t| !t.can_delete));
    }

    /// cargo test print_tasks -- --ignored --nocapture (doar citire)
    #[test]
    #[ignore]
    fn print_tasks() {
        let all = list();
        println!("{} sarcini, {} Microsoft", all.len(), all.iter().filter(|t| t.microsoft).count());
        for t in all.iter().filter(|t| !t.microsoft) {
            println!("{}{} | {} | activ={} lipsă={} reclamă={} a_ta={} șterge={} | {} | {}", t.path, t.name, t.publisher.clone().unwrap_or_default(), t.enabled, t.missing, t.looks_like_ad, t.user_created, t.can_delete, t.triggers, t.next_run);
        }
    }

    /// Ștergere + anulare pe o sarcină de probă creată de test (doar cu drepturi de administrator).
    /// cargo test task_delete_roundtrip -- --ignored --nocapture  (într-un terminal ridicat)
    #[test]
    #[ignore]
    fn task_delete_roundtrip() {
        let name = format!("PioCareTestTask{}", std::process::id());
        ps(
            "Register-ScheduledTask -TaskPath '\\' -TaskName $env:PC_TNAME -Action (New-ScheduledTaskAction -Execute 'C:\\Windows\\System32\\cmd.exe' -Argument '/c exit') -Trigger (New-ScheduledTaskTrigger -Once -At (Get-Date).AddYears(5)) | Out-Null",
            &[("PC_TNAME", &name)],
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!("piocare-task-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let plan = crate::engine::Plan {
            title: "t".into(),
            rule_id: None,
            actions: vec![crate::actions::ActionSpec::DeleteTask { path: "\\".into(), name: name.clone() }],
        };
        let op = crate::engine::execute(&root, crate::engine::Request::Apply { plan }).unwrap();
        assert_eq!(op.status, crate::journal::OpStatus::Applied, "{:?}", op);
        assert!(!list().iter().any(|t| t.name == name));
        crate::engine::execute(&root, crate::engine::Request::Undo { id: op.id }).unwrap();
        assert!(list().iter().any(|t| t.name == name), "sarcina trebuia readusă");
        let _ = delete("\\", &name);
        let _ = std::fs::remove_dir_all(&root);
        println!("ștergere + anulare: OK");
    }
}
