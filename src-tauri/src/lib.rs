//! PioCare — motorul.
//! Citirile sunt libere; orice schimbare trece prin `engine` (copie, jurnal, anulare).

mod actions;
mod engine;
mod journal;
mod rules;
mod sys;
mod system;

use actions::{ActionSpec, RegData, Root};
use engine::{Plan, Request};
use journal::Operation;

#[tauri::command]
fn system_overview() -> system::Overview {
    system::overview()
}

#[tauri::command]
fn rules_info() -> rules::RulesInfo {
    rules::info()
}

#[derive(serde::Serialize)]
struct StepView {
    text: String,
    state: journal::StepState,
    error: Option<String>,
}

#[derive(serde::Serialize)]
struct OpView {
    #[serde(flatten)]
    op: Operation,
    step_texts: Vec<StepView>,
    folder: String,
}

fn view(root: &std::path::Path, op: Operation) -> OpView {
    let step_texts = op
        .steps
        .iter()
        .map(|s| StepView { text: s.action.describe(), state: s.state.clone(), error: s.error.clone() })
        .collect();
    let folder = op.dir(root).to_string_lossy().into_owned();
    OpView { op, step_texts, folder }
}

#[tauri::command]
fn journal_list() -> Result<Vec<OpView>, String> {
    let root = journal::default_root()?;
    Ok(journal::list(&root).into_iter().map(|o| view(&root, o)).collect())
}

#[tauri::command]
fn is_admin() -> bool {
    sys::is_elevated()
}

#[tauri::command(async)]
fn undo_operation(id: String) -> Result<OpView, String> {
    let root = journal::default_root()?;
    engine::run(&root, Request::Undo { id }).map(|o| view(&root, o))
}

#[tauri::command(async)]
fn full_backup() -> Result<OpView, String> {
    let root = journal::default_root()?;
    engine::run(&root, Request::FullBackup).map(|o| view(&root, o))
}

/// Operație de probă, fără risc: scrie doar în HKCU\Software\PioCare\Proba.
#[tauri::command(async)]
fn safe_test_operation() -> Result<OpView, String> {
    let root = journal::default_root()?;
    let key = "Software\\PioCare\\Proba".to_string();
    let plan = Plan {
        title: "Probă sigură (registrul utilizatorului)".into(),
        rule_id: Some("test.proba".into()),
        actions: vec![
            ActionSpec::RegSetValue { root: Root::HKCU, path: key.clone(), name: "Activ".into(), value: RegData::dword(1) },
            ActionSpec::RegSetValue {
                root: Root::HKCU,
                path: key,
                name: "Mesaj".into(),
                value: RegData::sz("Scris de PioCare; se șterge la anulare."),
            },
        ],
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let root = journal::default_root()?;
    let p = std::path::PathBuf::from(&path);
    if !p.starts_with(&root) || !p.is_dir() {
        return Err("folder necunoscut".into());
    }
    std::process::Command::new("explorer.exe").arg(p).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// PioCare.exe --elevated <cerere> <rezultat>: pasul de administrator, fără fereastră.
pub fn elevated_main(req: &str, res: &str) -> i32 {
    engine::elevated_main(req, res)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            system_overview,
            rules_info,
            journal_list,
            is_admin,
            undo_operation,
            full_backup,
            safe_test_operation,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("PioCare nu a putut porni");
}
