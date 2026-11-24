//! Limba aplicației (ro / en). Setarea se păstrează în %LOCALAPPDATA%\PioCare\settings.json.
//! Textele din pachetele de reguli pot fi simple („text”) sau bilingve ({"ro": "...", "en": "..."}).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0); // 0 = ro, 1 = en

pub fn set(lang: &str) {
    LANG.store(if lang == "en" { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn current() -> &'static str {
    if LANG.load(Ordering::Relaxed) == 1 { "en" } else { "ro" }
}

pub fn en() -> bool {
    LANG.load(Ordering::Relaxed) == 1
}

/// Alege textul în limba curentă.
pub fn t(ro: &str, en_text: &str) -> String {
    if en() { en_text.to_string() } else { ro.to_string() }
}

/// Text dintr-un pachet de reguli: simplu sau bilingv.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Text {
    Plain(String),
    Multi { ro: String, #[serde(default)] en: Option<String> },
}

impl Text {
    pub fn get(&self) -> String {
        match self {
            Text::Plain(s) => s.clone(),
            Text::Multi { ro, en: e } => {
                if en() {
                    e.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| ro.clone())
                } else {
                    ro.clone()
                }
            }
        }
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Text::Plain(s.to_string())
    }
}

impl Default for Text {
    fn default() -> Self {
        Text::Plain(String::new())
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Settings {
    /// None = încă nealeasă (se întreabă la prima pornire).
    pub lang: Option<String>,
}

fn file() -> Option<std::path::PathBuf> {
    std::env::var("LOCALAPPDATA").ok().map(|b| std::path::PathBuf::from(b).join("PioCare").join("settings.json"))
}

pub fn load() -> Settings {
    let s: Settings = file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if let Some(l) = &s.lang {
        set(l);
    }
    s
}

pub fn save_lang(lang: &str) -> Result<(), String> {
    if lang != "ro" && lang != "en" {
        return Err("unsupported language".into());
    }
    set(lang);
    let mut s = load();
    s.lang = Some(lang.to_string());
    set(lang);
    let f = file().ok_or("LOCALAPPDATA")?;
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(f, serde_json::to_string_pretty(&s).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Limba Windows, ca sugestie la prima pornire.
pub fn system_suggestion() -> &'static str {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
        let id = unsafe { GetUserDefaultUILanguage() };
        if id & 0x3ff == 0x18 { "ro" } else { "en" } // 0x18 = LANG_ROMANIAN
    }
    #[cfg(not(windows))]
    {
        "en"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_variants() {
        let a: Text = serde_json::from_str("\"simplu\"").unwrap();
        let b: Text = serde_json::from_str(r#"{"ro":"unu","en":"one"}"#).unwrap();
        set("ro");
        assert_eq!(a.get(), "simplu");
        assert_eq!(b.get(), "unu");
        set("en");
        assert_eq!(b.get(), "one");
        assert_eq!(a.get(), "simplu");
        set("ro");
    }
}
