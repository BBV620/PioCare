//! Pachetele de reguli. În 0.1 se folosește doar pachetul de bază inclus în aplicație.
//! Mai târziu: pachet mai nou din %ProgramData%\PioCare\rules, cu semnătură verificată.

use serde::{Deserialize, Serialize};

/// Cea mai mare versiune de format pe care o înțelege acest motor.
pub const SUPPORTED_SCHEMA: u32 = 1;

const BUILTIN_SERVICES: &str = include_str!("../../rules/services.json");

#[derive(Deserialize)]
struct Pack {
    schema_version: u32,
    version: String,
    #[serde(default)]
    rules: Vec<serde_json::Value>,
}

#[derive(Serialize)]
pub struct RulesInfo {
    pub source: String,
    pub version: String,
    pub schema_version: u32,
    pub supported_schema: u32,
    pub rule_count: usize,
}

pub fn info() -> RulesInfo {
    match serde_json::from_str::<Pack>(BUILTIN_SERVICES) {
        Ok(p) => RulesInfo {
            source: crate::i18n::t("inclus în aplicație", "built into the app"),
            version: p.version,
            schema_version: p.schema_version,
            supported_schema: SUPPORTED_SCHEMA,
            rule_count: p.rules.len(),
        },
        Err(e) => RulesInfo {
            source: format!("eroare: {e}"),
            version: "-".into(),
            schema_version: 0,
            supported_schema: SUPPORTED_SCHEMA,
            rule_count: 0,
        },
    }
}
