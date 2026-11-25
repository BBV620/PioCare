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
mod monitor;
mod rules;
mod services;
mod sys;
mod startup;
mod system;
mod tasks;
mod uninstall;

use actions::ActionSpec;
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

#[derive(serde::Serialize)]
struct Progress {
    title: String,
    done: usize,
    total: usize,
}

/// Progresul operației în curs (citit din jurnal, pe care motorul îl rescrie după fiecare pas).
#[tauri::command]
fn op_progress() -> Option<Progress> {
    let root = journal::default_root().ok()?;
    let newest = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .max_by_key(|e| e.file_name())?;
    let op = journal::load(&root, &newest.file_name().to_string_lossy()).ok()?;
    if op.status != journal::OpStatus::InProgress || journal::now().saturating_sub(op.created) > 900 {
        return None;
    }
    let done = op.steps.iter().filter(|s| s.state != journal::StepState::Pending).count();
    Some(Progress { title: op.title, done, total: op.steps.len() })
}

/// Punctele de restaurare și spațiul lor (cere administrator: o fereastră UAC, doar citire).
#[tauri::command(async)]
fn restore_points() -> Result<sys::RestoreInfo, String> {
    if sys::is_elevated() {
        return Ok(sys::restore_info());
    }
    let root = journal::default_root()?;
    let dir = root.join(".requests");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let res = dir.join(format!("rp-{}.json", journal::now()));
    sys::run_self_elevated(&["--restore-info".into(), res.to_string_lossy().into_owned()])?;
    let text = std::fs::read_to_string(&res).map_err(|_| i18n::t("pasul de administrator nu a întors un rezultat", "the administrator step returned no result"))?;
    let _ = std::fs::remove_file(&res);
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Schimbă limita de spațiu pentru punctele de restaurare (prin motor, cu anulare). `gb` = 0 → nelimitat.
#[tauri::command(async)]
fn restore_limit(gb: u64) -> Result<OpView, String> {
    let root = journal::default_root()?;
    if gb > 0 && !(2..=100).contains(&gb) {
        return Err(i18n::t("limită nepermisă", "limit not allowed"));
    }
    let plan = Plan {
        title: i18n::t("Limita punctelor de restaurare", "Restore point limit"),
        rule_id: Some("restore.limit".into()),
        actions: vec![ActionSpec::RestoreLimit { max_bytes: if gb == 0 { None } else { Some(gb * 1_073_741_824) } }],
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// PioCare.exe --restore-info <rezultat>: citire ridicată a punctelor de restaurare.
pub fn elevated_restore_info(res: &str) -> i32 {
    let info = sys::restore_info();
    let ok = info.error.is_none();
    let _ = std::fs::write(res, serde_json::to_string(&info).unwrap_or_default());
    if ok { 0 } else { 2 }
}

/// Monitor: o măsurătoare (procesor, RAM, procese, placa video).
#[tauri::command(async)]
fn monitor_sample(by: String) -> monitor::Sample {
    monitor::sample(&by, 30)
}

/// Monitor: închide un proces al utilizatorului (confirmat în interfață).
#[tauri::command(async)]
fn monitor_kill(pid: u32, name: String) -> Result<(), String> {
    monitor::kill(pid, &name)
}

/// Detalii despre o adresă IP (local: proprietar cunoscut + nume DNS invers).
#[tauri::command(async)]
fn ip_info(ip: String) -> Result<explain::IpInfo, String> {
    explain::ip_info(&ip)
}

/// Deschide în browser o pagină publică de detalii pentru o adresă IP (doar la clic).
#[tauri::command]
fn ip_web(ip: String) -> Result<(), String> {
    let ip: std::net::IpAddr = ip.parse().map_err(|_| i18n::t("adresă IP invalidă", "invalid IP address"))?;
    sys::open_url(&format!("https://ipinfo.io/{ip}"))
}

/// „Caută pe internet” din fereastra „Ce este?”.
#[tauri::command]
fn search_web(query: String) -> Result<(), String> {
    sys::open_search(&query)
}

/// Sarcini programate (doar citire).
#[tauri::command(async)]
fn tasks_list() -> Vec<tasks::TaskItem> {
    tasks::list()
}

/// Sarcini: activare/dezactivare sau ștergere (cu copie XML), prin motor, cu anulare.
/// `mode` = "disable" / "enable" / "delete". Sarcinile Microsoft nu se ating; cele create de tine nu se șterg.
#[tauri::command(async)]
fn tasks_apply(ids: Vec<String>, mode: String) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let chosen: Vec<_> = tasks::list()
        .into_iter()
        .filter(|t| ids.contains(&t.id) && !t.microsoft)
        .filter(|t| match mode.as_str() {
            "disable" => t.enabled,
            "enable" => !t.enabled,
            "delete" => t.can_delete,
            _ => false,
        })
        .collect();
    if chosen.is_empty() {
        return Err(i18n::t("nimic de schimbat", "nothing to change"));
    }
    let names = chosen.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", ");
    let (ro, en) = match mode.as_str() {
        "delete" => ("Sarcini șterse", "Tasks deleted"),
        "enable" => ("Sarcini activate", "Tasks enabled"),
        _ => ("Sarcini dezactivate", "Tasks disabled"),
    };
    let plan = Plan {
        title: i18n::t(&format!("{ro}: {names}"), &format!("{en}: {names}")),
        rule_id: Some(format!("tasks.{mode}")),
        actions: chosen
            .into_iter()
            .map(|t| match mode.as_str() {
                "delete" => ActionSpec::DeleteTask { path: t.path, name: t.name },
                m => ActionSpec::ScheduledTask { path: t.path, name: t.name, enabled: m == "enable" },
            })
            .collect(),
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Programe la pornire (doar citire).
#[tauri::command(async)]
fn startup_list() -> Vec<startup::StartupItem> {
    startup::list()
}

/// Pornește/oprește elementele bifate (ca Task Manager), prin motor, cu anulare.
#[tauri::command(async)]
fn startup_set(ids: Vec<String>, enable: bool) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let chosen: Vec<_> = startup::list().into_iter().filter(|i| ids.contains(&i.id) && i.enabled != enable && (enable || !i.protected)).collect();
    if chosen.is_empty() {
        return Err(i18n::t("nimic de schimbat", "nothing to change"));
    }
    let names: Vec<String> = chosen.iter().map(|i| i.name.clone()).collect();
    let plan = Plan {
        title: if enable {
            i18n::t(&format!("Pornire: activat {}", names.join(", ")), &format!("Startup: enabled {}", names.join(", ")))
        } else {
            i18n::t(&format!("Pornire: dezactivat {}", names.join(", ")), &format!("Startup: disabled {}", names.join(", ")))
        },
        rule_id: Some("startup".into()),
        actions: chosen.into_iter().map(|i| if enable { i.enable } else { i.disable }).collect(),
    };
    engine::run(&root, Request::Apply { plan }).map(|o| view(&root, o))
}

/// Foldere rămase de la programe dezinstalate (doar citire).
#[tauri::command(async)]
fn orphans_scan() -> Vec<uninstall::Orphan> {
    uninstall::orphans()
}

/// Mută în carantină folderele orfane alese (rescanează întâi), prin motor, cu anulare.
#[tauri::command(async)]
fn orphans_clean(ids: Vec<String>) -> Result<OpView, String> {
    let root = journal::default_root()?;
    let chosen: Vec<_> = uninstall::orphans().into_iter().filter(|o| ids.contains(&o.id)).collect();
    if chosen.is_empty() {
        return Err(i18n::t("nicio intrare selectată nu mai este valabilă; rescanează", "none of the selected entries is still valid; scan again"));
    }
    let plan = Plan {
        title: i18n::t(&format!("Foldere rămase: {} mutate în carantină", chosen.len()), &format!("Leftover folders: {} moved to quarantine", chosen.len())),
        rule_id: Some("uninstall.orphans".into()),
        actions: chosen.into_iter().map(|o| ActionSpec::Quarantine { path: o.path }).collect(),
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

/// Deschide folderul în care se pun pachetele de reguli actualizate (îl creează dacă lipsește).
#[tauri::command]
fn open_rules_folder() -> Result<(), String> {
    let d = rules::updates_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe").arg(d).spawn().map(|_| ()).map_err(|e| e.to_string())
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
            protection_status,
            registry_scan,
            registry_clean,
            services_list,
            services_analyze,
            services_apply,
            explain,
            open_rules_folder,
            search_web,
            ip_info,
            ip_web,
            op_progress,
            restore_points,
            restore_limit,
            monitor_sample,
            monitor_kill,
            tasks_list,
            tasks_apply,
            startup_list,
            startup_set,
            orphans_scan,
            orphans_clean,
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
