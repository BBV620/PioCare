//! Motorul de schimbări: orice modificare trece pe aici.
//! Reguli: (1) se salvează starea de dinainte ÎNAINTE de orice schimbare;
//! (2) dacă un pas eșuează, pașii deja aplicați se readuc automat;
//! (3) totul se scrie în jurnal și se poate anula ulterior.

use crate::actions::{self, ActionSpec};
use crate::journal::{self, OpKind, OpStatus, Operation, Step, StepState};
use crate::sys;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Plan {
    pub title: String,
    pub rule_id: Option<String>,
    pub actions: Vec<ActionSpec>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Apply { plan: Plan },
    Undo { id: String },
    FullBackup,
    Cleanup { ids: Vec<String> },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Operation),
    Err(String),
}

/// Ce se trimite pasului ridicat: cererea și folderul jurnalului utilizatorului.
#[derive(Serialize, Deserialize)]
struct Envelope {
    journal: PathBuf,
    request: Request,
    #[serde(default)]
    lang: Option<String>,
}

pub fn needs_admin(req: &Request, root: &Path) -> Result<bool, String> {
    Ok(match req {
        Request::Apply { plan } => plan.actions.iter().any(|a| a.needs_admin()),
        Request::Undo { id } => journal::load(root, id)?.needs_admin,
        Request::FullBackup => true,
        Request::Cleanup { ids } => crate::maintenance::needs_admin(ids)?,
    })
}

/// Punctul de intrare din interfață: cere UAC doar când e nevoie.
pub fn run(root: &Path, req: Request) -> Result<Operation, String> {
    if needs_admin(&req, root)? && !sys::is_elevated() {
        run_elevated(root, req)
    } else {
        execute(root, req)
    }
}

fn run_elevated(root: &Path, req: Request) -> Result<Operation, String> {
    let dir = root.join(".requests");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = journal::now().to_string() + &std::process::id().to_string();
    let (rq, rs) = (dir.join(format!("{stamp}.req.json")), dir.join(format!("{stamp}.res.json")));
    let env = Envelope { journal: root.to_path_buf(), request: req, lang: Some(crate::i18n::current().into()) };
    std::fs::write(&rq, serde_json::to_string(&env).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let res = sys::run_self_elevated(&[
        "--elevated".into(),
        rq.to_string_lossy().into_owned(),
        rs.to_string_lossy().into_owned(),
    ]);
    let _ = std::fs::remove_file(&rq);
    res?;
    let text = std::fs::read_to_string(&rs).map_err(|_| crate::i18n::t("pasul de administrator nu a întors un rezultat", "the administrator step returned no result"))?;
    let _ = std::fs::remove_file(&rs);
    match serde_json::from_str::<Outcome>(&text).map_err(|e| e.to_string())? {
        Outcome::Ok(op) => Ok(op),
        Outcome::Err(e) => Err(e),
    }
}

/// Rulat în procesul ridicat (PioCare.exe --elevated <cerere> <rezultat>).
pub fn elevated_main(req_file: &str, res_file: &str) -> i32 {
    let out = match std::fs::read_to_string(req_file)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<Envelope>(&s).map_err(|e| e.to_string()))
    {
        Ok(env) => match {
            crate::i18n::set(env.lang.as_deref().unwrap_or("ro"));
            execute(&env.journal, env.request)
        } {
            Ok(op) => Outcome::Ok(op),
            Err(e) => Outcome::Err(e),
        },
        Err(e) => Outcome::Err(format!("cerere invalidă: {e}")),
    };
    let ok = matches!(out, Outcome::Ok(_));
    let _ = std::fs::write(res_file, serde_json::to_string(&out).unwrap_or_default());
    if ok { 0 } else { 2 }
}

pub fn execute(root: &Path, req: Request) -> Result<Operation, String> {
    match req {
        Request::Apply { plan } => apply(root, plan),
        Request::Undo { id } => undo(root, &id),
        Request::FullBackup => full_backup(root),
        Request::Cleanup { ids } => cleanup(root, &ids),
    }
}

fn cleanup(root: &Path, ids: &[String]) -> Result<Operation, String> {
    if ids.is_empty() {
        return Err(crate::i18n::t("nicio categorie selectată", "no category selected"));
    }
    let admin = crate::maintenance::needs_admin(ids)?;
    let mut op = Operation::new(root, OpKind::Cleanup, &crate::i18n::t("Întreținere: curățare", "Maintenance: cleanup"), Some("maintenance".into()), admin)?;
    op.save(root)?;
    let results = crate::maintenance::clean(ids)?;
    let (mut files, mut bytes) = (0u64, 0u64);
    for r in &results {
        files += r.files;
        bytes += r.bytes;
        let mut line = crate::i18n::t(&format!("{}: {} fișiere, {:.1} MB", r.title, r.files, r.bytes as f64 / 1_048_576.0), &format!("{}: {} files, {:.1} MB", r.title, r.files, r.bytes as f64 / 1_048_576.0));
        if r.skipped > 0 {
            line.push_str(&crate::i18n::t(&format!(" ({} în uz, sărite)", r.skipped), &format!(" ({} in use, skipped)", r.skipped)));
        }
        if let Some(n) = &r.note {
            line.push_str(&format!(" · {n}"));
        }
        op.notes.push(line);
    }
    op.title = crate::i18n::t(&format!("Întreținere: {:.1} MB eliberați ({} fișiere)", bytes as f64 / 1_048_576.0, files), &format!("Maintenance: {:.1} MB freed ({} files)", bytes as f64 / 1_048_576.0, files));
    op.notes.push(crate::i18n::t("Curățarea nu se poate anula: sunt fișiere temporare și cache-uri, refăcute automat la nevoie.", "Cleanup cannot be undone: these are temporary files and caches, recreated automatically when needed."));
    op.status = OpStatus::CleanupDone;
    op.save(root)?;
    Ok(op)
}

fn apply(root: &Path, plan: Plan) -> Result<Operation, String> {
    if plan.actions.is_empty() {
        return Err(crate::i18n::t("planul nu are nicio acțiune", "the plan has no actions"));
    }
    let admin = plan.actions.iter().any(|a| a.needs_admin());
    let mut op = Operation::new(root, OpKind::Change, &plan.title, plan.rule_id.clone(), admin)?;
    op.steps = plan
        .actions
        .into_iter()
        .map(|action| Step { action, before: None, state: StepState::Pending, error: None })
        .collect();
    op.save(root)?;
    let dir = op.dir(root);

    // 1. Starea de dinainte pentru TOȚI pașii, înainte de prima schimbare.
    for i in 0..op.steps.len() {
        match actions::capture(&op.steps[i].action, &dir, i) {
            Ok(b) => {
                if let actions::Before::KeyExport { file: Some(f) } = &b {
                    op.files.push(f.clone());
                }
                op.steps[i].before = Some(b);
            }
            Err(e) => {
                op.steps[i].state = StepState::Failed;
                op.steps[i].error = Some(e.clone());
                op.status = OpStatus::Failed;
                op.notes.push(crate::i18n::t("Copia de siguranță a eșuat; nu s-a schimbat nimic.", "The backup failed; nothing was changed."));
                op.save(root)?;
                return Ok(op);
            }
        }
    }
    op.save(root)?;

    // 1b. Pentru schimbări de firewall: export complet al politicii, înainte de orice.
    if op.steps.iter().any(|s| matches!(s.action, ActionSpec::FirewallBlock(_) | ActionSpec::FirewallProfileOn { .. })) {
        match sys::export_firewall(&dir.join("firewall-inainte.wfw")) {
            Ok(()) => op.files.push("firewall-inainte.wfw".into()),
            Err(e) => {
                op.status = OpStatus::Failed;
                op.notes.push(crate::i18n::t(&format!("Exportul firewall-ului a eșuat ({e}); nu s-a schimbat nimic."), &format!("The firewall export failed ({e}); nothing was changed.")));
                op.save(root)?;
                return Ok(op);
            }
        }
        op.save(root)?;
    }

    // 2. Punct de restaurare, doar pentru schimbări de sistem (copiile proprii există deja).
    if admin {
        op.restore_point = Some(sys::restore_point(&op.title));
        op.save(root)?;
    }

    // 3. Aplicare, cu revenire automată la primul eșec.
    for i in 0..op.steps.len() {
        match actions::apply(&op.steps[i].action, op.steps[i].before.as_ref()) {
            Ok(()) => op.steps[i].state = StepState::Applied,
            Err(e) => {
                op.steps[i].state = StepState::Failed;
                op.steps[i].error = Some(e);
                op.save(root)?;
                rollback(&mut op, &dir, i);
                op.status = OpStatus::RolledBack;
                op.notes.push(crate::i18n::t("Un pas a eșuat; pașii aplicați au fost readuși automat.", "A step failed; the steps already applied were rolled back automatically."));
                op.save(root)?;
                return Ok(op);
            }
        }
        op.save(root)?;
    }
    op.status = OpStatus::Applied;
    // Dacă tocmai am pornit Protecția sistemului, facem imediat primul punct de restaurare.
    if op.steps.iter().any(|s| matches!(s.action, ActionSpec::SystemProtection { enabled: true })) {
        op.restore_point = Some(sys::restore_point("protectie pornita"));
    }
    op.save(root)?;
    Ok(op)
}

/// Readuce, în ordine inversă, pașii aplicați dinaintea indexului `upto`.
fn rollback(op: &mut Operation, dir: &Path, upto: usize) -> bool {
    let mut all_ok = true;
    for j in (0..upto).rev() {
        let s = &mut op.steps[j];
        if s.state != StepState::Applied {
            continue;
        }
        let Some(before) = s.before.clone() else { continue };
        match actions::undo(&s.action, &before, dir) {
            Ok(()) => s.state = StepState::Undone,
            Err(e) => {
                s.state = StepState::UndoFailed;
                s.error = Some(e);
                all_ok = false;
            }
        }
    }
    all_ok
}

/// Momentul ultimei scrieri a jurnalului operației (secunde Unix).
fn last_write(root: &Path, op: &Operation) -> u64 {
    std::fs::metadata(op.dir(root).join("operation.json"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn undo(root: &Path, id: &str) -> Result<Operation, String> {
    let mut op = journal::load(root, id)?;
    if op.kind != OpKind::Change {
        return Err(crate::i18n::t("această operație nu se anulează", "this operation cannot be undone"));
    }
    // Operație întreruptă (ex. calculatorul a fost repornit în timpul aplicării): se readuce tot ce
    // avea starea de dinainte salvată. Readucerea e sigură și pentru pașii neaplicați, pentru că
    // fiecare pas își rescrie doar valoarea de dinainte (carantina neaplicată se sare).
    if op.status == OpStatus::InProgress {
        if journal::now().saturating_sub(last_write(root, &op)) < 120 {
            return Err(crate::i18n::t("operația încă rulează; încearcă peste un minut", "the operation is still running; try again in a minute"));
        }
        for s in op.steps.iter_mut() {
            if s.state == StepState::Pending && s.before.is_some() && !matches!(s.action, ActionSpec::Quarantine { .. }) {
                s.state = StepState::Applied;
            }
        }
    } else if !matches!(op.status, OpStatus::Applied | OpStatus::PartiallyUndone) {
        return Err(crate::i18n::t("operația nu este în starea „aplicată”", "the operation is not in the applied state"));
    }
    // Pașii care au eșuat la o anulare anterioară se reîncearcă.
    for s in op.steps.iter_mut() {
        if s.state == StepState::UndoFailed {
            s.state = StepState::Applied;
        }
    }
    let dir = op.dir(root);
    let n = op.steps.len();
    let ok = rollback(&mut op, &dir, n);
    op.status = if ok { OpStatus::Undone } else { OpStatus::PartiallyUndone };
    op.undone = Some(journal::now());
    op.save(root)?;
    Ok(op)
}

fn full_backup(root: &Path) -> Result<Operation, String> {
    let mut op = Operation::new(root, OpKind::Backup, &crate::i18n::t("Copie de siguranță completă", "Full backup"), None, true)?;
    let dir = op.dir(root);
    op.restore_point = Some(sys::restore_point("copie completa"));
    op.save(root)?;
    match sys::export_firewall(&dir.join("firewall.wfw")) {
        Ok(()) => op.files.push("firewall.wfw".into()),
        Err(e) => op.notes.push(format!("Firewall: {e}")),
    }
    match sys::run_hidden(
        "reg.exe",
        &["export", "HKLM\\SYSTEM\\CurrentControlSet\\Services", &dir.join("servicii.reg").to_string_lossy(), "/y"],
    ) {
        Ok(_) => op.files.push("servicii.reg".into()),
        Err(e) => op.notes.push(crate::i18n::t(&format!("Servicii: {e}"), &format!("Services: {e}"))),
    }
    op.status = if op.files.is_empty() { OpStatus::Failed } else { OpStatus::BackupDone };
    op.save(root)?;
    Ok(op)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::actions::{RegData, Root};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const KEY: &str = "Software\\PioCare\\UnitTest";

    fn tmp_root(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("piocare-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    #[test]
    fn apply_and_undo_restore_exact_state() {
        let root = tmp_root("roundtrip");
        let _ = hkcu().delete_subkey_all(KEY);
        // Stare inițială: cheia există, cu o valoare care va fi schimbată și una care va fi ștearsă.
        let (k, _) = hkcu().create_subkey(KEY).unwrap();
        k.set_value("Schimbata", &7u32).unwrap();
        k.set_value("Stearsa", &"original").unwrap();

        let plan = Plan {
            title: "test".into(),
            rule_id: None,
            actions: vec![
                ActionSpec::RegSetValue { root: Root::HKCU, path: KEY.into(), name: "Schimbata".into(), value: RegData::dword(99) },
                ActionSpec::RegSetValue { root: Root::HKCU, path: KEY.into(), name: "Noua".into(), value: RegData::sz("nou") },
                ActionSpec::RegDeleteValue { root: Root::HKCU, path: KEY.into(), name: "Stearsa".into() },
                ActionSpec::RegDeleteKey { root: Root::HKCU, path: format!("{KEY}\\Sub") },
            ],
        };
        let (sub, _) = hkcu().create_subkey(format!("{KEY}\\Sub")).unwrap();
        sub.set_value("x", &1u32).unwrap();

        let op = execute(&root, Request::Apply { plan }).unwrap();
        assert_eq!(op.status, OpStatus::Applied, "{:?}", op);
        let k = hkcu().open_subkey(KEY).unwrap();
        assert_eq!(k.get_value::<u32, _>("Schimbata").unwrap(), 99);
        assert_eq!(k.get_value::<String, _>("Noua").unwrap(), "nou");
        assert!(k.get_value::<String, _>("Stearsa").is_err());
        assert!(hkcu().open_subkey(format!("{KEY}\\Sub")).is_err());

        let op = execute(&root, Request::Undo { id: op.id }).unwrap();
        assert_eq!(op.status, OpStatus::Undone);
        let k = hkcu().open_subkey(KEY).unwrap();
        assert_eq!(k.get_value::<u32, _>("Schimbata").unwrap(), 7);
        assert!(k.get_value::<String, _>("Noua").is_err());
        assert_eq!(k.get_value::<String, _>("Stearsa").unwrap(), "original");
        let sub = hkcu().open_subkey(format!("{KEY}\\Sub")).unwrap();
        assert_eq!(sub.get_value::<u32, _>("x").unwrap(), 1);

        // A doua anulare e refuzată.
        assert!(execute(&root, Request::Undo { id: op.id.clone() }).is_err());
        assert_eq!(journal::list(&root).len(), 1);
        let _ = hkcu().delete_subkey_all(KEY);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn interrupted_operation_can_be_reverted() {
        let root = tmp_root("interrupted");
        let key = "Software\\PioCare\\UnitTest3";
        let _ = hkcu().delete_subkey_all(key);
        let plan = Plan {
            title: "t".into(),
            rule_id: None,
            actions: vec![
                ActionSpec::RegSetValue { root: Root::HKCU, path: key.into(), name: "a".into(), value: RegData::dword(1) },
                ActionSpec::RegSetValue { root: Root::HKCU, path: key.into(), name: "b".into(), value: RegData::dword(2) },
            ],
        };
        let mut op = execute(&root, Request::Apply { plan }).unwrap();
        // Simulăm o repornire după primul pas: al doilea „neaplicat”, operația rămasă „în curs”, scrisă acum 10 minute.
        op.status = OpStatus::InProgress;
        op.steps[1].state = StepState::Pending;
        op.save(&root).unwrap();
        let f = std::fs::File::options().write(true).open(op.dir(&root).join("operation.json")).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(600)).unwrap();
        drop(f);
        let op = execute(&root, Request::Undo { id: op.id }).unwrap();
        assert_eq!(op.status, OpStatus::Undone, "{:?}", op);
        assert!(hkcu().open_subkey(key).is_err(), "cheia creată trebuia să dispară");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_capture_changes_nothing() {
        let root = tmp_root("capture");
        let plan = Plan {
            title: "test".into(),
            rule_id: None,
            actions: vec![
                ActionSpec::RegSetValue { root: Root::HKCU, path: "Software\\PioCare\\UnitTest2".into(), name: "a".into(), value: RegData::dword(1) },
                // Cale prea generală: refuzată la copia de siguranță.
                ActionSpec::RegDeleteKey { root: Root::HKCU, path: "Software".into() },
            ],
        };
        let op = execute(&root, Request::Apply { plan }).unwrap();
        assert_eq!(op.status, OpStatus::Failed);
        assert!(hkcu().open_subkey("Software\\PioCare\\UnitTest2").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn drivers_are_refused() {
        let root = tmp_root("driver");
        let plan = Plan {
            title: "test".into(),
            rule_id: None,
            actions: vec![ActionSpec::ServiceStartType { name: "disk".into(), start: crate::actions::StartType::Disabled }],
        };
        // Captura citește doar; „disk” e driver de boot, deci operația trebuie să eșueze fără schimbări.
        let op = apply(&root, plan).unwrap();
        assert_eq!(op.status, OpStatus::Failed);
        let _ = std::fs::remove_dir_all(&root);
    }
}
