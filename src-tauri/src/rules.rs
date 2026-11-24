//! Pachetele de reguli: „cunoștințele” PioCare, ținute separat de program.
//! Fiecare pachet are o versiune inclusă în aplicație (rezervă). Un pachet mai nou poate fi pus în
//! %ProgramData%\PioCare\rules\<nume>.json împreună cu <nume>.json.sig (semnătură ECDSA P-256).
//! Pachetul nou e folosit DOAR dacă: semnătura e validă (cheia publică PioCare), formatul e înțeles
//! de această versiune a aplicației și versiunea e mai nouă decât cea inclusă.

use serde::Serialize;
use std::path::PathBuf;

/// Cea mai mare versiune de format pe care o înțelege acest motor.
pub const SUPPORTED_SCHEMA: u32 = 1;

/// Cheia publică PioCare pentru reguli (SEC1, necomprimată). Cheia privată nu e în depozit.
const PUBLIC_KEY_HEX: &str = "0474f649f52b786f82c73ac1428edff86a84c7523e853033baa6399186bdffb154ad87dd7f7f145d21851d9ab10e67f60bf770f87f3b58728464248b8b505f9a37";

const BUILTIN: &[(&str, &str)] = &[
    ("services", include_str!("../../rules/services.json")),
    ("registry", include_str!("../../rules/registry.json")),
    ("firewall", include_str!("../../rules/firewall.json")),
    ("maintenance", include_str!("../../rules/maintenance.json")),
    ("explain", include_str!("../../rules/explain.json")),
];

#[derive(Serialize, Clone, Debug)]
pub struct PackStatus {
    pub name: String,
    pub builtin_version: String,
    pub active_version: String,
    /// builtin / updated
    pub source: String,
    /// none / valid / invalid / older / schema_too_new / broken
    pub update_state: String,
    pub update_version: Option<String>,
}

fn version_of(text: &str) -> (u32, String) {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    (
        v.get("schema_version").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
        v.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    )
}

/// Compară versiuni de tip „2026.11.24.3” pe bucăți numerice.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |s: &str| s.split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a) > parts(b)
}

pub fn updates_dir() -> PathBuf {
    std::env::var("ProgramData").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("C:\\ProgramData")).join("PioCare").join("rules")
}

pub fn verify(data: &[u8], sig_hex: &str) -> bool {
    use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
    let (Ok(key_bytes), Ok(sig_bytes)) = (crate::actions::from_hex(PUBLIC_KEY_HEX), crate::actions::from_hex(sig_hex.trim())) else {
        return false;
    };
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_sec1_bytes(&key_bytes), Signature::from_slice(&sig_bytes)) else {
        return false;
    };
    key.verify(data, &sig).is_ok()
}

fn load(name: &str, dir: &std::path::Path) -> (String, PackStatus) {
    let builtin = BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).unwrap_or("{}");
    let (_, bver) = version_of(builtin);
    let mut st = PackStatus {
        name: name.into(),
        builtin_version: bver.clone(),
        active_version: bver.clone(),
        source: "builtin".into(),
        update_state: "none".into(),
        update_version: None,
    };
    let file = dir.join(format!("{name}.json"));
    let Ok(data) = std::fs::read(&file) else { return (builtin.to_string(), st) };
    let sig = std::fs::read_to_string(dir.join(format!("{name}.json.sig"))).unwrap_or_default();
    let text = String::from_utf8_lossy(&data).into_owned();
    let (schema, uver) = version_of(&text);
    st.update_version = Some(uver.clone());
    if !verify(&data, &sig) {
        st.update_state = "invalid".into();
    } else if serde_json::from_str::<serde_json::Value>(&text).is_err() {
        st.update_state = "broken".into();
    } else if schema > SUPPORTED_SCHEMA {
        st.update_state = "schema_too_new".into();
    } else if !newer(&uver, &bver) {
        st.update_state = "older".into();
    } else {
        st.update_state = "valid".into();
        st.source = "updated".into();
        st.active_version = uver;
        return (text, st);
    }
    (builtin.to_string(), st)
}

/// Textul pachetului activ (actualizat și verificat, sau cel inclus în aplicație).
pub fn text(name: &str) -> String {
    load(name, &updates_dir()).0
}

pub fn status() -> Vec<PackStatus> {
    let dir = updates_dir();
    BUILTIN.iter().map(|(n, _)| load(n, &dir).1).collect()
}

#[derive(Serialize)]
pub struct RulesInfo {
    pub source: String,
    pub version: String,
    pub schema_version: u32,
    pub supported_schema: u32,
    pub rule_count: usize,
    pub packs: Vec<PackStatus>,
    pub folder: String,
}

/// Rezumat pentru Panou: cel mai nou pachet activ + numărul total de reguli.
pub fn info() -> RulesInfo {
    let packs = status();
    let mut newest = String::new();
    for p in &packs {
        if newer(&p.active_version, &newest) {
            newest = p.active_version.clone();
        }
    }
    let count: usize = BUILTIN
        .iter()
        .map(|(n, _)| {
            let v: serde_json::Value = serde_json::from_str(&text(n)).unwrap_or_default();
            ["rules", "items", "categories", "services", "programs"]
                .iter()
                .map(|k| v.get(*k).map_or(0, |x| x.as_array().map(|a| a.len()).or_else(|| x.as_object().map(|o| o.len())).unwrap_or(0)))
                .sum::<usize>()
        })
        .sum();
    let updated = packs.iter().filter(|p| p.source == "updated").count();
    RulesInfo {
        source: if updated > 0 {
            crate::i18n::t(&format!("{updated} pachete actualizate"), &format!("{updated} packs updated"))
        } else {
            crate::i18n::t("incluse în aplicație", "built into the app")
        },
        version: newest,
        schema_version: SUPPORTED_SCHEMA,
        supported_schema: SUPPORTED_SCHEMA,
        rule_count: count,
        packs,
        folder: updates_dir().to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(newer("2026.11.24.10", "2026.11.24.9"));
        assert!(!newer("2026.11.24", "2026.11.24"));
        assert!(newer("2027.1.1", "2026.12.31.5"));
    }

    #[test]
    fn unsigned_or_forged_update_is_ignored() {
        let dir = std::env::temp_dir().join(format!("piocare-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (_, bver) = version_of(BUILTIN[0].1);
        // Pachet „mai nou”, dar cu semnătură falsă: trebuie ignorat.
        std::fs::write(dir.join("services.json"), r#"{"schema_version":1,"version":"2999.1.1","rules":[]}"#).unwrap();
        std::fs::write(dir.join("services.json.sig"), "00".repeat(64)).unwrap();
        let (text, st) = load("services", &dir);
        assert_eq!(st.update_state, "invalid");
        assert_eq!(st.active_version, bver);
        assert!(text.contains("DiagTrack"), "trebuie să rămână pachetul inclus");
        let _ = std::fs::remove_file(dir.join("services.json"));
        assert_eq!(load("services", &dir).1.update_state, "none");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtin_packs_parse() {
        for (n, t) in BUILTIN {
            let (schema, v) = version_of(t);
            assert!(schema >= 1 && schema <= SUPPORTED_SCHEMA && !v.is_empty(), "{n}");
        }
    }

    /// Cu pachete semnate real (node tools/semneaza-reguli.cjs <cheie> dist-reguli):
    /// cargo test signed_pack -- --ignored --nocapture
    #[test]
    #[ignore]
    fn signed_pack() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("dist-reguli");
        for (n, _) in BUILTIN {
            let data = std::fs::read(dir.join(format!("{n}.json"))).unwrap();
            let sig = std::fs::read_to_string(dir.join(format!("{n}.json.sig"))).unwrap();
            assert!(verify(&data, &sig), "{n}: semnătură invalidă");
            let mut bad = data.clone();
            bad[10] ^= 1;
            assert!(!verify(&bad, &sig), "{n}: o modificare trebuia detectată");
        }
        println!("toate pachetele: semnătură validă, modificările sunt detectate");
    }
}
