//! Modulul „Firewall pentru siguranță”, peste Windows Firewall (nu îl înlocuiește).
//! PioCare adaugă DOAR reguli de blocare, toate cu nume „PioCare-…” și grupul „PioCare”,
//! ca să poată fi recunoscute și scoase oricând. Citirea se face din registru (fără administrator),
//! scrierea prin cmdlet-urile oficiale NetSecurity (cu administrator, prin motor).

use crate::actions::ActionSpec;
use crate::sys::run_hidden_env;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

const RULES: &str = include_str!("../../rules/firewall.json");
const POLICY: &str = "SYSTEM\\CurrentControlSet\\Services\\SharedAccess\\Parameters\\FirewallPolicy";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FwRule {
    /// Numele intern, mereu „PioCare-…”.
    pub name: String,
    pub display: String,
    /// "in" sau "out"
    pub direction: String,
    pub protocol: Option<String>,
    pub local_ports: Option<String>,
    pub program: Option<String>,
    /// "Any" sau listă: "Public", "Private", "Public,Private"
    pub profiles: String,
}

impl FwRule {
    pub fn validate(&self) -> Result<(), String> {
        let ok_name = self.name.starts_with("PioCare-")
            && self.name.len() < 120
            && self.name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
        if !ok_name {
            return Err(format!("nume de regulă invalid: {}", self.name));
        }
        if self.direction != "in" && self.direction != "out" {
            return Err("direcție invalidă".into());
        }
        if let Some(p) = &self.protocol {
            if p != "TCP" && p != "UDP" {
                return Err("protocol invalid".into());
            }
        }
        if let Some(lp) = &self.local_ports {
            if lp.is_empty() || !lp.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '-') || self.protocol.is_none() {
                return Err("porturi invalide".into());
            }
        }
        if let Some(prog) = &self.program {
            let b = prog.as_bytes();
            if !(b.len() > 3 && b[1] == b':' && b[2] == b'\\') || prog.contains('"') {
                return Err(format!("cale de program invalidă: {prog}"));
            }
        }
        if self.program.is_none() && self.local_ports.is_none() {
            // O regulă fără program și fără porturi ar bloca TOT traficul într-o direcție.
            return Err(crate::i18n::t("regula ar bloca tot traficul; refuz", "the rule would block all traffic; refused"));
        }
        if self.display.contains('"') || self.profiles.contains('"') {
            return Err("text invalid".into());
        }
        for p in self.profiles.split(',') {
            if !["Any", "Public", "Private", "Domain"].contains(&p.trim()) {
                return Err(format!("profil invalid: {p}"));
            }
        }
        Ok(())
    }
}

fn ps(script: &str, env: &[(&str, &str)]) -> Result<String, String> {
    let full = format!("$ErrorActionPreference='Stop'; Import-Module NetSecurity; {script}");
    run_hidden_env("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full], env)
}

/// Valorile textului sunt transmise prin variabile de mediu, nu lipite în script (fără injecții).
pub fn add_rule(r: &FwRule) -> Result<(), String> {
    r.validate()?;
    if rule_exists(&r.name) {
        return Ok(());
    }
    let mut script = String::from(
        "$p=@{Name=$env:PC_NAME; DisplayName=('PioCare: '+$env:PC_DISPLAY); Group='PioCare'; Direction=$env:PC_DIR; Action='Block'; Enabled='True'; Profile=($env:PC_PROF -split ',')}; ",
    );
    if r.protocol.is_some() {
        script.push_str("$p.Protocol=$env:PC_PROTO; ");
    }
    if r.local_ports.is_some() {
        script.push_str("$p.LocalPort=($env:PC_PORTS -split ','); ");
    }
    if r.program.is_some() {
        script.push_str("$p.Program=$env:PC_PROG; ");
    }
    script.push_str("New-NetFirewallRule @p | Out-Null");
    let dir = if r.direction == "in" { "Inbound" } else { "Outbound" };
    ps(
        &script,
        &[
            ("PC_NAME", &r.name),
            ("PC_DISPLAY", &r.display),
            ("PC_DIR", dir),
            ("PC_PROF", &r.profiles),
            ("PC_PROTO", r.protocol.as_deref().unwrap_or("")),
            ("PC_PORTS", r.local_ports.as_deref().unwrap_or("")),
            ("PC_PROG", r.program.as_deref().unwrap_or("")),
        ],
    )?;
    if rule_exists(&r.name) { Ok(()) } else { Err(crate::i18n::t(&format!("regula {} nu a apărut în firewall", r.name), &format!("rule {} did not appear in the firewall", r.name))) }
}

pub fn remove_rule(name: &str) -> Result<(), String> {
    if !name.starts_with("PioCare-") {
        return Err(crate::i18n::t("PioCare scoate doar regulile create de el", "PioCare only removes its own rules"));
    }
    if !rule_exists(name) {
        return Ok(());
    }
    ps("Remove-NetFirewallRule -Name $env:PC_NAME", &[("PC_NAME", name)])?;
    Ok(())
}

fn profile_key(profile: &str) -> Result<&'static str, String> {
    Ok(match profile {
        "Domain" => "DomainProfile",
        "Private" => "StandardProfile",
        "Public" => "PublicProfile",
        _ => return Err(format!("profil necunoscut: {profile}")),
    })
}

pub fn set_profile(profile: &str, enabled: bool, inbound: &str) -> Result<(), String> {
    profile_key(profile)?;
    if !["Block", "Allow", "NotConfigured"].contains(&inbound) {
        return Err("acțiune implicită invalidă".into());
    }
    ps(
        "Set-NetFirewallProfile -Profile $env:PC_PROF -Enabled ([bool]::Parse($env:PC_EN)) -DefaultInboundAction $env:PC_IN",
        &[("PC_PROF", profile), ("PC_EN", if enabled { "true" } else { "false" }), ("PC_IN", inbound)],
    )?;
    Ok(())
}

#[derive(Serialize, Clone, Debug)]
pub struct ProfileState {
    pub profile: String,
    pub enabled: bool,
    /// Block / Allow / NotConfigured (= Block, implicit Windows)
    pub inbound: String,
    pub policy_managed: bool,
}

#[cfg(windows)]
pub fn read_profile(profile: &str) -> Result<ProfileState, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let key = profile_key(profile)?;
    let hk = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);
    let k = hk.open_subkey(format!("{POLICY}\\{key}")).map_err(|e| format!("firewall {profile}: {e}"))?;
    let en: u32 = k.get_value("EnableFirewall").unwrap_or(1);
    let inbound = match k.get_value::<u32, _>("DefaultInboundAction") {
        Ok(1) => "Block",
        Ok(0) => "Allow",
        _ => "NotConfigured",
    };
    let gp = hk.open_subkey(format!("SOFTWARE\\Policies\\Microsoft\\WindowsFirewall\\{key}")).is_ok();
    Ok(ProfileState { profile: profile.into(), enabled: en != 0, inbound: inbound.into(), policy_managed: gp })
}

#[cfg(windows)]
fn rule_names() -> HashSet<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(format!("{POLICY}\\FirewallRules"))
        .map(|k| k.enum_values().flatten().map(|(n, _)| n).collect())
        .unwrap_or_default()
}

#[cfg(windows)]
pub fn rule_exists(name: &str) -> bool {
    rule_names().contains(name)
}

// ---------------- politica propusă ----------------

#[derive(Deserialize)]
struct RuleDef {
    id: String,
    display: crate::i18n::Text,
    direction: String,
    protocol: Option<String>,
    local_ports: Option<String>,
    program: Option<String>,
    profiles: String,
    #[serde(default)]
    needed_for: Vec<String>,
    risk: String,
    why: crate::i18n::Text,
}

#[derive(Deserialize)]
struct ProfileDef {
    id: String,
    profile: String,
    why: crate::i18n::Text,
}

#[derive(Deserialize)]
struct Rules {
    schema_version: u32,
    version: String,
    features: BTreeMap<String, crate::i18n::Text>,
    profiles: Vec<ProfileDef>,
    rules: Vec<RuleDef>,
}

fn rules() -> Result<Rules, String> {
    let r: Rules = serde_json::from_str(RULES).map_err(|e| format!("rules/firewall.json: {e}"))?;
    if r.schema_version > crate::rules::SUPPORTED_SCHEMA {
        return Err("pachetul de reguli pentru firewall cere o versiune mai nouă a aplicației".into());
    }
    Ok(r)
}

fn expand(s: &str) -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    s.replace("%SystemRoot%", &root).replace("%windir%", &root)
}

#[derive(Serialize, Clone, Debug)]
pub struct FwProposal {
    pub id: String,
    pub title: String,
    pub why: String,
    pub risk: String,
    pub action: ActionSpec,
}

#[derive(Serialize)]
pub struct FwStatus {
    pub rules_version: String,
    pub features: BTreeMap<String, String>,
    pub profiles: Vec<ProfileState>,
    pub total_rules: usize,
    pub piocare_rules: Vec<String>,
    pub proposals: Vec<FwProposal>,
}

#[cfg(windows)]
pub fn status(uses: &[String]) -> Result<FwStatus, String> {
    let r = rules()?;
    let names = rule_names();
    let mut proposals = vec![];
    let mut profiles = vec![];
    for p in &r.profiles {
        let st = read_profile(&p.profile)?;
        if !st.enabled || st.inbound == "Allow" {
            proposals.push(FwProposal {
                id: p.id.clone(),
                title: match p.profile.as_str() {
                    "Public" => crate::i18n::t("Pornește firewall-ul pentru rețele publice", "Turn on the firewall for public networks"),
                    "Private" => crate::i18n::t("Pornește firewall-ul pentru rețeaua de acasă", "Turn on the firewall for the home network"),
                    _ => crate::i18n::t("Pornește firewall-ul pentru rețeaua de firmă", "Turn on the firewall for the work network"),
                },
                why: p.why.get(),
                risk: "low".into(),
                action: ActionSpec::FirewallProfileOn { profile: p.profile.clone() },
            });
        }
        profiles.push(st);
    }
    for d in &r.rules {
        if d.needed_for.iter().any(|f| uses.contains(f)) {
            continue;
        }
        let name = format!("PioCare-{}", d.id);
        if names.contains(&name) {
            continue;
        }
        let program = d.program.as_ref().map(|p| expand(p));
        if let Some(p) = &program {
            if !PathBuf::from(p).exists() {
                continue; // de ex. varianta pe 32 de biți lipsește
            }
        }
        let rule = FwRule {
            name,
            display: d.display.get(),
            direction: d.direction.clone(),
            protocol: d.protocol.clone(),
            local_ports: d.local_ports.clone(),
            program,
            profiles: d.profiles.clone(),
        };
        if rule.validate().is_err() {
            continue;
        }
        proposals.push(FwProposal {
            id: d.id.clone(),
            title: d.display.get(),
            why: d.why.get(),
            risk: d.risk.clone(),
            action: ActionSpec::FirewallBlock(rule),
        });
    }
    let mut piocare_rules: Vec<String> = names.iter().filter(|n| n.starts_with("PioCare-")).cloned().collect();
    piocare_rules.sort();
    Ok(FwStatus {
        rules_version: r.version,
        features: r.features.iter().map(|(k, v)| (k.clone(), v.get())).collect(),
        profiles,
        total_rules: names.len(),
        piocare_rules,
        proposals,
    })
}

// ---------------- programe care ies pe internet ----------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetProgram {
    pub name: String,
    pub path: Option<String>,
    pub connections: usize,
    pub remotes: Vec<String>,
    #[serde(default)]
    pub new: bool,
    #[serde(default)]
    pub blocked: bool,
}

#[derive(Serialize)]
pub struct NetActivity {
    pub programs: Vec<NetProgram>,
    pub first_run: bool,
    pub known_count: usize,
}

fn known_file() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA lipsește".to_string())?;
    Ok(PathBuf::from(base).join("PioCare").join("programe-cunoscute.json"))
}

pub fn block_name_for(path: &str) -> String {
    // Nume stabil, derivat din cale (FNV-1a), ca blocarea aceluiași program să nu se dubleze.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("PioCare-out-app-{h:016x}")
}

#[cfg(windows)]
pub fn activity() -> Result<NetActivity, String> {
    const SCRIPT: &str = r#"
$c = Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | Where-Object { $_.RemoteAddress -notmatch '^(127\.|::1$|0\.0\.0\.0|::$)' }
$out = @($c | Group-Object OwningProcess | ForEach-Object {
  $p = Get-Process -Id ([int]$_.Name) -ErrorAction SilentlyContinue
  [pscustomobject]@{ name = [string]$p.ProcessName; path = $p.Path; connections = $_.Count;
    remotes = @($_.Group | Select-Object -First 4 | ForEach-Object { "$($_.RemoteAddress):$($_.RemotePort)" }) }
})
ConvertTo-Json -InputObject $out -Depth 3 -Compress
"#;
    let text = crate::sys::run_hidden("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])?;
    let mut list: Vec<NetProgram> = serde_json::from_str(text.trim()).map_err(|e| format!("conexiuni: {e}"))?;

    // Același program poate avea mai multe procese: îl adunăm după cale (sau nume).
    let mut merged: HashMap<String, NetProgram> = HashMap::new();
    for p in list.drain(..) {
        let key = p.path.clone().unwrap_or_else(|| p.name.clone()).to_lowercase();
        merged
            .entry(key)
            .and_modify(|m| {
                m.connections += p.connections;
                for r in &p.remotes {
                    if m.remotes.len() < 6 && !m.remotes.contains(r) {
                        m.remotes.push(r.clone());
                    }
                }
            })
            .or_insert(p);
    }

    let file = known_file()?;
    let mut known: BTreeMap<String, u64> =
        std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let first_run = known.is_empty();
    let names = rule_names();
    let now = crate::journal::now();
    let mut programs: Vec<NetProgram> = merged
        .into_iter()
        .map(|(key, mut p)| {
            p.new = !first_run && !known.contains_key(&key);
            p.blocked = p.path.as_ref().map_or(false, |path| names.contains(&block_name_for(path)));
            known.entry(key).or_insert(now);
            p
        })
        .collect();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&file, serde_json::to_string_pretty(&known).unwrap_or_default());
    programs.sort_by(|a, b| b.new.cmp(&a.new).then(b.connections.cmp(&a.connections)));
    Ok(NetActivity { programs, first_run, known_count: known.len() })
}

/// Regula de blocare a ieșirii pentru un program ales de utilizator.
pub fn block_program_rule(path: &str) -> Result<FwRule, String> {
    let p = PathBuf::from(path);
    if !p.is_file() {
        return Err(crate::i18n::t("programul nu există pe disc", "the program does not exist on disk"));
    }
    let lower = path.to_lowercase();
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()).to_lowercase();
    // Componentele Windows de bază nu se blochează din acest buton (ar putea tăia actualizările sau rețeaua).
    if lower.starts_with(&format!("{windir}\\system32\\svchost.exe")) || lower.ends_with("\\lsass.exe") || lower.ends_with("\\system") {
        return Err(crate::i18n::t("acest proces Windows nu poate fi blocat din PioCare", "this Windows process cannot be blocked from PioCare"));
    }
    let file = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let rule = FwRule {
        name: block_name_for(path),
        display: crate::i18n::t(&format!("Ieșire pe internet: {file}"), &format!("Internet access: {file}")),
        direction: "out".into(),
        protocol: None,
        local_ports: None,
        program: Some(path.to_string()),
        profiles: "Any".into(),
    };
    rule.validate()?;
    Ok(rule)
}

pub fn proposals_to_actions(props: &[FwProposal]) -> Vec<ActionSpec> {
    props.iter().map(|p| p.action.clone()).collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_and_validate() {
        let r = rules().unwrap();
        assert!(r.rules.len() >= 15);
        for d in &r.rules {
            for f in &d.needed_for {
                assert!(r.features.contains_key(f), "{}: funcție necunoscută {f}", d.id);
            }
            let rule = FwRule {
                name: format!("PioCare-{}", d.id),
                display: d.display.get(),
                direction: d.direction.clone(),
                protocol: d.protocol.clone(),
                local_ports: d.local_ports.clone(),
                program: d.program.as_ref().map(|p| expand(p)),
                profiles: d.profiles.clone(),
            };
            rule.validate().unwrap_or_else(|e| panic!("{}: {e}", d.id));
        }
    }

    #[test]
    fn refuses_dangerous_rules() {
        let mut r = FwRule {
            name: "PioCare-x".into(),
            display: "x".into(),
            direction: "out".into(),
            protocol: None,
            local_ports: None,
            program: None,
            profiles: "Any".into(),
        };
        assert!(r.validate().is_err(), "fără program și porturi ar bloca tot");
        r.program = Some("C:\\a.exe".into());
        assert!(r.validate().is_ok());
        r.name = "AltNume".into();
        assert!(r.validate().is_err());
        r.name = "PioCare-x;rm".into();
        assert!(r.validate().is_err());
    }

    #[test]
    fn reads_profiles_without_admin() {
        for p in ["Domain", "Private", "Public"] {
            read_profile(p).unwrap();
        }
        assert!(rule_names().len() > 50);
    }

    /// cargo test print_firewall -- --ignored --nocapture
    #[test]
    #[ignore]
    fn print_firewall() {
        let s = status(&[]).unwrap();
        for p in &s.profiles {
            println!("{:?}", p);
        }
        println!("reguli: {}, PioCare: {:?}", s.total_rules, s.piocare_rules);
        for p in &s.proposals {
            println!("  [{}] {}", p.risk, p.title);
        }
    }
}
