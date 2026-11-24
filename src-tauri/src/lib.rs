//! PioCare — motorul.
//! Citirile sunt libere; orice schimbare trece prin `engine` (copie, jurnal, anulare).

mod actions;
mod cleaner;
mod engine;
mod explain;
mod firewall;
mod i18n;
mod journal;
mod maintenance;
mod rules;
mod services;
mod sys;
mod system;
mod uninstall;

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
        title: crate::i18n::t("Probă sigură (registrul utilizatorului)", "Safe test (user registry)"),
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

/// Probă de test pentru servicii (temporară, pentru verificarea în mașina virtuală):
/// Windows Search (WSearch) trecut pe pornire manuală. Anularea readuce tipul inițial.
#[tauri::command(async)]
fn service_test_operation() -> Result<OpView, String> {
    let root = journal::default_root()?;
    let plan = Plan {
        title: crate::i18n::t("Probă serviciu: Windows Search pe pornire manuală", "Service test: Windows Search set to manual"),
        rule_id: Some("test.serviciu".into()),
        actions: vec![ActionSpec::ServiceStartType { name: "WSearch".into(), start: actions::StartType::Manual }],
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Registru curat: scanare (doar citire).
#[tauri::command(async)]
fn registry_scan() -> Result<cleaner::ScanResult, String> {
    cleaner::scan()
}

/// Registru curat: șterge intrările bifate. Rescanează și folosește doar ce e încă valabil,
/// apoi trece totul prin motor (copie, jurnal, anulare).
#[tauri::command(async)]
fn registry_clean(ids: Vec<String>) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let scan = cleaner::scan()?;
    let chosen: Vec<_> = scan.findings.into_iter().filter(|f| ids.contains(&f.id)).collect();
    if chosen.is_empty() {
        return Err(crate::i18n::t("nicio intrare selectată nu mai este valabilă; rescanează", "none of the selected entries is still valid; scan again"));
    }
    let plan = Plan {
        title: crate::i18n::t(&format!("Registru curat: {} intrări", chosen.len()), &format!("Registry cleanup: {} entries", chosen.len())),
        rule_id: Some(format!("registry.{}", cleaner::RULES_ID)),
        actions: chosen.into_iter().map(|f| f.action).collect(),
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Harta serviciilor (doar citire).
#[tauri::command(async)]
fn services_list() -> Result<Vec<services::Svc>, String> {
    services::list()
}

/// Propuneri pentru profilul și funcțiile alese (doar citire).
#[tauri::command(async)]
fn services_analyze(profile: String, uses: Vec<String>) -> Result<services::Analysis, String> {
    services::analyze(&profile, &uses)
}

/// Aplică propunerile bifate. Reanalizează, ca să folosească doar propuneri încă valabile.
#[tauri::command(async)]
fn services_apply(profile: String, uses: Vec<String>, names: Vec<String>) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let a = services::analyze(&profile, &uses)?;
    let chosen: Vec<_> = a.proposals.into_iter().filter(|p| names.contains(&p.service)).collect();
    if chosen.is_empty() {
        return Err(crate::i18n::t("nicio propunere selectată nu mai este valabilă; analizează din nou", "none of the selected proposals is still valid; analyze again"));
    }
    let plan = Plan {
        title: crate::i18n::t(&format!("Servicii, profil {profile}: {} schimbări", chosen.len()), &format!("Services, {profile} profile: {} changes", chosen.len())),
        rule_id: Some(format!("services.{profile}")),
        actions: services::to_actions(&chosen),
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Firewall: stare + propuneri (doar citire).
#[tauri::command(async)]
fn firewall_status(uses: Vec<String>) -> Result<firewall::FwStatus, String> {
    firewall::status(&uses)
}

/// Firewall: aplică propunerile bifate (reanalizează întâi), prin motor.
#[tauri::command(async)]
fn firewall_apply(uses: Vec<String>, ids: Vec<String>) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let st = firewall::status(&uses)?;
    let chosen: Vec<_> = st.proposals.into_iter().filter(|p| ids.contains(&p.id)).collect();
    if chosen.is_empty() {
        return Err(crate::i18n::t("nicio propunere selectată nu mai este valabilă; reîmprospătează", "none of the selected proposals is still valid; refresh"));
    }
    let plan = Plan {
        title: crate::i18n::t(&format!("Firewall: {} reguli de siguranță", chosen.len()), &format!("Firewall: {} safety rules", chosen.len())),
        rule_id: Some("firewall.policy".into()),
        actions: firewall::proposals_to_actions(&chosen),
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Programele care au acum conexiuni spre internet (doar citire).
#[tauri::command(async)]
fn firewall_activity() -> Result<firewall::NetActivity, String> {
    firewall::activity()
}

/// Blochează ieșirea pe internet pentru un program (prin motor, se poate anula).
#[tauri::command(async)]
fn firewall_block_program(path: String) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let rule = firewall::block_program_rule(&path)?;
    let plan = Plan {
        title: format!("Firewall: {}", rule.display),
        rule_id: Some("firewall.block.program".into()),
        actions: vec![ActionSpec::FirewallBlock(rule)],
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Dezinstalator: lista programelor (doar citire).
#[tauri::command(async)]
fn programs_list() -> Vec<uninstall::Program> {
    uninstall::list()
}

#[tauri::command]
fn program_exists(id: String) -> bool {
    uninstall::exists(&id)
}

/// Rulează dezinstalatorul oficial și așteaptă să se închidă (întoarce codul de ieșire).
#[tauri::command(async)]
fn program_uninstall(id: String) -> Result<u32, String> {
    uninstall::run(&id)
}

#[derive(serde::Serialize)]
struct LeftoverScan {
    leftovers: Vec<uninstall::Leftover>,
    still_installed: bool,
}

/// Resturile unui program (după dezinstalare). Doar citire.
#[tauri::command(async)]
fn program_leftovers(program: uninstall::Program) -> LeftoverScan {
    let all = uninstall::list();
    LeftoverScan { still_installed: uninstall::exists(&program.id), leftovers: uninstall::leftovers(&program, &all) }
}

/// Curăță resturile bifate: rescanează și trece doar ce e încă valabil prin motor (copie, carantină, anulare).
#[tauri::command(async)]
fn program_clean(program: uninstall::Program, ids: Vec<String>) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let all = uninstall::list();
    let chosen: Vec<_> = uninstall::leftovers(&program, &all).into_iter().filter(|l| ids.contains(&l.id)).collect();
    if chosen.is_empty() {
        return Err(i18n::t("nicio intrare selectată nu mai este valabilă; rescanează", "none of the selected entries is still valid; scan again"));
    }
    let plan = Plan {
        title: i18n::t(
            &format!("Resturi {}: {} elemente", program.name, chosen.len()),
            &format!("{} leftovers: {} items", program.name, chosen.len()),
        ),
        rule_id: Some("uninstall.leftovers".into()),
        actions: chosen.into_iter().map(|l| l.action).collect(),
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// „Ce este?” pentru un serviciu sau program (doar citire).
#[tauri::command(async)]
fn explain(kind: String, name: String, path: Option<String>) -> Result<explain::Explanation, String> {
    match kind.as_str() {
        "service" => explain::service(&name),
        _ => explain::program(&name, path),
    }
}

#[derive(serde::Serialize)]
struct LangInfo {
    /// null = nealeasă încă (se întreabă la prima pornire)
    lang: Option<String>,
    suggestion: &'static str,
}

#[tauri::command]
fn get_language() -> LangInfo {
    LangInfo { lang: i18n::load().lang, suggestion: i18n::system_suggestion() }
}

#[tauri::command]
fn set_language(lang: String) -> Result<(), String> {
    i18n::save_lang(&lang)
}

/// Întreținere: cât se poate elibera (doar citire).
#[tauri::command(async)]
fn maintenance_scan() -> Result<maintenance::Scan, String> {
    maintenance::scan()
}

/// Întreținere: curăță categoriile bifate (cu administrator doar dacă e nevoie).
#[tauri::command(async)]
fn maintenance_clean(ids: Vec<String>) -> Result<OpView, String> {
    let root = journal::default_root()?;
    engine::run(&root, Request::Cleanup { ids }).map(|o| view(&root, o))
}

/// Starea Protecției sistemului pe C: (null = necunoscută fără administrator).
#[tauri::command]
fn protection_status() -> Option<bool> {
    sys::protection_enabled_c()
}

/// Pornește Protecția sistemului pe C:, prin jurnal (se poate anula).
#[tauri::command(async)]
fn enable_protection() -> Result<OpView, String> {
    let root = journal::default_root()?;
    let plan = Plan {
        title: crate::i18n::t("Pornire Protecția sistemului (puncte de restaurare) pe C:", "Turn on System Protection (restore points) on C:"),
        rule_id: Some("sys.restore.enable".into()),
        actions: vec![ActionSpec::SystemProtection { enabled: true }],
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
    i18n::load();
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(windows)]
            {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    if let Ok(h) = w.hwnd() {
                        sys::set_main_hwnd(h.0 as isize);
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system_overview,
            rules_info,
            journal_list,
            is_admin,
            undo_operation,
            full_backup,
            safe_test_operation,
            protection_status,
            service_test_operation,
            registry_scan,
            registry_clean,
            services_list,
            services_analyze,
            services_apply,
            explain,
            programs_list,
            program_exists,
            program_uninstall,
            program_leftovers,
            program_clean,
            get_language,
            set_language,
            maintenance_scan,
            maintenance_clean,
            firewall_status,
            firewall_apply,
            firewall_activity,
            firewall_block_program,
            enable_protection,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("PioCare nu a putut porni");
}
