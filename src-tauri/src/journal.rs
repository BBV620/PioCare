//! Jurnalul operațiilor. Fiecare operație are folderul ei:
//! %LOCALAPPDATA%\PioCare\journal\<id>\operation.json + fișierele de copie (.reg, .wfw).
//! Fișierul se rescrie după fiecare pas, ca o întrerupere să nu piardă starea „înainte”.

use crate::actions::{ActionSpec, Before};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Change,
    Backup,
    /// Curățare de fișiere temporare/cache: fără anulare (fișierele se refac automat).
    Cleanup,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OpStatus {
    InProgress,
    Applied,
    /// O acțiune a eșuat și pașii deja aplicați au fost readuși automat.
    RolledBack,
    Undone,
    PartiallyUndone,
    Failed,
    BackupDone,
    CleanupDone,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Applied,
    Failed,
    Undone,
    UndoFailed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Step {
    pub action: ActionSpec,
    pub before: Option<Before>,
    pub state: StepState,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Operation {
    pub id: String,
    pub created: u64,
    pub title: String,
    pub rule_id: Option<String>,
    pub kind: OpKind,
    pub status: OpStatus,
    pub needs_admin: bool,
    pub restore_point: Option<String>,
    pub files: Vec<String>,
    pub notes: Vec<String>,
    pub steps: Vec<Step>,
    pub undone: Option<u64>,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:013}", d.as_millis())
}

/// Folderul implicit al jurnalului.
pub fn default_root() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA lipsește".to_string())?;
    let p = PathBuf::from(base).join("PioCare").join("journal");
    std::fs::create_dir_all(&p).map_err(|e| format!("nu pot crea {}: {e}", p.display()))?;
    Ok(p)
}

impl Operation {
    pub fn new(root: &Path, kind: OpKind, title: &str, rule_id: Option<String>, needs_admin: bool) -> Result<Self, String> {
        let mut id = new_id();
        while root.join(&id).exists() {
            std::thread::sleep(std::time::Duration::from_millis(2));
            id = new_id();
        }
        std::fs::create_dir_all(root.join(&id)).map_err(|e| e.to_string())?;
        Ok(Operation {
            id,
            created: now(),
            title: title.to_string(),
            rule_id,
            kind,
            status: OpStatus::InProgress,
            needs_admin,
            restore_point: None,
            files: vec![],
            notes: vec![],
            steps: vec![],
            undone: None,
        })
    }

    pub fn dir(&self, root: &Path) -> PathBuf {
        root.join(&self.id)
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        let dir = self.dir(root);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = dir.join("operation.json.tmp");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dir.join("operation.json")).map_err(|e| e.to_string())
    }
}

pub fn load(root: &Path, id: &str) -> Result<Operation, String> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err("id invalid".into());
    }
    let s = std::fs::read_to_string(root.join(id).join("operation.json")).map_err(|e| format!("operația {id}: {e}"))?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// Toate operațiile, cele mai noi primele.
pub fn list(root: &Path) -> Vec<Operation> {
    let mut v: Vec<Operation> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| load(root, &e.file_name().to_string_lossy()).ok())
        .collect();
    v.sort_by(|a, b| b.id.cmp(&a.id));
    v
}
