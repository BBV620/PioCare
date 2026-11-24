//! Utilitare Windows: comenzi fără fereastră, verificare drepturi de administrator,
//! punct de restaurare, export firewall și relansare ridicată (UAC).

use std::path::Path;

/// Rulează o unealtă Windows fără să apară fereastră de consolă.
pub fn run_hidden(exe: &str, args: &[&str]) -> Result<String, String> {
    run_hidden_env(exe, args, &[])
}

pub fn run_hidden_env(exe: &str, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("{exe}: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        Ok(text.trim().to_string())
    } else {
        Err(format!("{exe} a eșuat: {}", text.trim()))
    }
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut tok = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            return false;
        }
        let mut el = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            tok,
            TokenElevation,
            &mut el as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(tok);
        ok != 0 && el.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Încearcă un punct de restaurare Windows. Întoarce un text pentru jurnal.
/// Nu e fatal dacă nu reușește: Protecția sistemului poate fi oprită, iar Windows
/// permite implicit un singur punct la 24 de ore. Copiile proprii PioCare rămân oricum.
pub fn restore_point(description: &str) -> String {
    const PS: &str = r#"
$ErrorActionPreference = 'Stop'
$d = $env:PIOCARE_RP_DESC
$before = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count
try { Checkpoint-Computer -Description $d -RestorePointType MODIFY_SETTINGS -WarningVariable w -WarningAction SilentlyContinue }
catch { 'EROARE: ' + $_.Exception.Message; exit 0 }
$after = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count
if ($after -gt $before) { 'CREAT' } elseif ($w) { 'SARIT: ' + $w[0] } else { 'SARIT: Windows nu a creat un punct nou (limita de 24 de ore)' }
"#;
    let desc: String = format!("PioCare: {description}").chars().take(200).collect();
    match run_hidden_env(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PS],
        &[("PIOCARE_RP_DESC", &desc)],
    ) {
        Ok(s) if s.starts_with("CREAT") => crate::i18n::t("punct de restaurare creat", "restore point created"),
        // Limita Windows (SystemRestorePointCreationFrequency, implicit 24 h): există deja un punct recent.
        Ok(s) if s.starts_with("SARIT") => {
            crate::i18n::t("punct de restaurare recent deja existent (Windows face cel mult unul la 24 de ore); copiile PioCare sunt făcute", "restore point: a recent one already exists (Windows makes at most one per 24 hours); PioCare backups are done")
        }
        Ok(s) => crate::i18n::t(&format!("punct de restaurare necreat ({})", s.lines().last().unwrap_or("").trim()), &format!("restore point not created ({})", s.lines().last().unwrap_or("").trim())),
        Err(e) => crate::i18n::t(&format!("punct de restaurare necreat ({e})"), &format!("restore point not created ({e})")),
    }
}

/// Protecția sistemului pe C: e pornită? `None` = nu se poate afla fără administrator.
/// Windows ține volumele protejate în SPP\Clients (citibil doar de administrator).
#[cfg(windows)]
pub fn protection_enabled_c() -> Option<bool> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let mount: Vec<u16> = "C:\\\0".encode_utf16().collect();
    let mut buf = [0u16; 64];
    if unsafe { GetVolumeNameForVolumeMountPointW(mount.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } == 0 {
        return None;
    }
    let vol = String::from_utf16_lossy(&buf).trim_end_matches('\0').to_lowercase();
    let guid = vol.split("volume").nth(1)?.trim_matches(|c| c == '\\').to_string();
    let k = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SPP\\Clients")
        .ok()?;
    let v: Vec<String> = k.get_value("{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}").unwrap_or_default();
    Some(v.iter().any(|s| s.to_lowercase().contains(&guid)))
}

#[cfg(not(windows))]
pub fn protection_enabled_c() -> Option<bool> {
    None
}

/// Pornește sau oprește Protecția sistemului pe C: (cere administrator).
/// Atenție: oprirea șterge punctele de restaurare existente pe C:.
pub fn set_protection_c(enabled: bool) -> Result<(), String> {
    let cmd = if enabled {
        "Enable-ComputerRestore -Drive 'C:\\'; vssadmin Resize ShadowStorage /For=C: /On=C: /MaxSize=5% | Out-Null"
    } else {
        "Disable-ComputerRestore -Drive 'C:\\'"
    };
    run_hidden(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &format!("$ErrorActionPreference='Stop'; {cmd}")],
    )?;
    match protection_enabled_c() {
        Some(now) if now == enabled => Ok(()),
        Some(_) => Err(crate::i18n::t("Windows nu a schimbat starea Protecției sistemului", "Windows did not change the System Protection state")),
        None => Ok(()),
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct RestoreInfo {
    pub points: Vec<RestorePoint>,
    pub used_bytes: u64,
    pub allocated_bytes: u64,
    /// None = nelimitat
    pub max_bytes: Option<u64>,
    pub disk_bytes: u64,
    pub error: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RestorePoint {
    pub time: String,
    pub description: String,
}

/// Punctele de restaurare și spațiul lor pe C: (cere administrator; rulat în pasul ridicat).
pub fn restore_info() -> RestoreInfo {
    const PS: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$p = @(Get-ComputerRestorePoint | ForEach-Object { [pscustomobject]@{ time = ([Management.ManagementDateTimeConverter]::ToDateTime($_.CreationTime)).ToString('yyyy-MM-dd HH:mm'); description = [string]$_.Description } })
$vol = Get-CimInstance Win32_Volume | Where-Object { $_.DriveLetter -eq 'C:' } | Select-Object -First 1
$s = Get-CimInstance Win32_ShadowStorage | Where-Object { $_.Volume.DeviceID -eq $vol.DeviceID } | Select-Object -First 1
$o = [pscustomobject]@{ points = $p; used = [uint64]$s.UsedSpace; alloc = [uint64]$s.AllocatedSpace; max = [uint64]$s.MaxSpace; disk = [uint64]$vol.Capacity }
[IO.File]::WriteAllText($env:PC_OUT, (ConvertTo-Json -InputObject $o -Depth 3 -Compress), (New-Object Text.UTF8Encoding $false))
exit 0
"#;
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        points: Vec<RestorePoint>,
        #[serde(default)]
        used: u64,
        #[serde(default)]
        alloc: u64,
        #[serde(default)]
        max: u64,
        #[serde(default)]
        disk: u64,
    }
    let tmp = std::env::temp_dir().join(format!("piocare-rp-{}.json", std::process::id()));
    let res = run_hidden_env(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PS],
        &[("PC_OUT", &tmp.to_string_lossy())],
    )
    .and_then(|_| std::fs::read_to_string(&tmp).map_err(|e| e.to_string()))
    .and_then(|s| serde_json::from_str::<Raw>(s.trim()).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&tmp);
    match res {
        Ok(r) => RestoreInfo {
            points: r.points,
            used_bytes: r.used,
            allocated_bytes: r.alloc,
            // MaxSpace = valoarea maximă posibilă înseamnă „nelimitat”.
            max_bytes: if r.max == 0 || r.max >= u64::MAX / 2 { None } else { Some(r.max) },
            disk_bytes: r.disk,
            error: None,
        },
        Err(e) => RestoreInfo { error: Some(e), ..Default::default() },
    }
}

/// Limita spațiului pentru punctele de restaurare pe C: (vssadmin, cere administrator).
/// `max` = None → nelimitat.
pub fn set_restore_limit(max: Option<u64>) -> Result<(), String> {
    let size = match max {
        None => "UNBOUNDED".to_string(),
        Some(b) => format!("{}MB", (b / 1_048_576).max(320)), // minimul acceptat de Windows: 320 MB
    };
    run_hidden("vssadmin.exe", &["Resize", "ShadowStorage", "/For=C:", "/On=C:", &format!("/MaxSize={size}")]).map(|_| ())
}

/// Export complet al politicii Windows Firewall (cere administrator).
pub fn export_firewall(file: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(file);
    run_hidden("netsh.exe", &["advfirewall", "export", &file.to_string_lossy()])?;
    if file.exists() { Ok(()) } else { Err("netsh nu a produs fișierul".into()) }
}

/// Rulează un program extern (ex. dezinstalatorul oficial) cu fereastra lui vizibilă și așteaptă să se închidă.
/// `admin` = cere direct drepturi de administrator (UAC).
#[cfg(windows)]
pub fn shell_run_wait(file: &str, params: &str, admin: bool) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, f, p) = (wide(if admin { "runas" } else { "open" }), wide(file), wide(params));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = f.as_ptr();
        info.lpParameters = p.as_ptr();
        info.nShow = 1; // SW_SHOWNORMAL: dezinstalatorul trebuie să se vadă
        let main = MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed);
        info.hwnd = main as *mut core::ffi::c_void;
        if ShellExecuteExW(&mut info) == 0 {
            return Err(if GetLastError() == 1223 {
                crate::i18n::t("ai refuzat cererea de administrator; nu s-a schimbat nimic", "you declined the administrator request; nothing was changed")
            } else {
                crate::i18n::t(
                    &format!("nu pot porni dezinstalatorul: {}", std::io::Error::last_os_error()),
                    &format!("cannot start the uninstaller: {}", std::io::Error::last_os_error()),
                )
            });
        }
        if info.hProcess.is_null() {
            return Ok(0);
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}

/// Deschide în browserul implicit o căutare pe internet (doar la cererea utilizatorului).
/// Se trimite doar textul căutat; motorul folosit este DuckDuckGo (fără profilare).
#[cfg(windows)]
pub fn open_search(query: &str) -> Result<(), String> {
    let q: String = query
        .chars()
        .take(200)
        .collect::<String>()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    open_url(&format!("https://duckduckgo.com/?q={q}"))
}

/// Deschide o adresă https în browserul implicit.
#[cfg(windows)]
pub fn open_url(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    if !url.starts_with("https://") || url.contains(['"', ' ', '\n']) {
        return Err("url invalid".into());
    }
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url));
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
    if r as isize > 32 { Ok(()) } else { Err(crate::i18n::t("nu pot deschide browserul", "cannot open the browser")) }
}

/// Fereastra principală PioCare, ca fereastra UAC să i se atașeze și să apară în față.
static MAIN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub fn set_main_hwnd(h: isize) {
    MAIN_HWND.store(h, std::sync::atomic::Ordering::Relaxed);
}

/// Pornește același executabil cu drepturi de administrator (fereastra UAC) și așteaptă să termine.
#[cfg(windows)]
pub fn run_self_elevated(args: &[String]) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let params = args.iter().map(|a| format!("\"{}\"", a.replace('"', ""))).collect::<Vec<_>>().join(" ");
    let (verb, file, par) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&params));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = par.as_ptr();
        info.nShow = 0; // SW_HIDE
        // Fereastra UAC se leagă de fereastra PioCare, ca să apară în față, nu doar în bara de jos.
        let main = MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed);
        info.hwnd = if main != 0 {
            main as *mut core::ffi::c_void
        } else {
            windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
        };
        if ShellExecuteExW(&mut info) == 0 {
            return Err(if GetLastError() == 1223 {
                crate::i18n::t("ai refuzat cererea de administrator; nu s-a schimbat nimic", "you declined the administrator request; nothing was changed")
            } else {
                crate::i18n::t(&format!("nu pot porni pasul de administrator: {}", std::io::Error::last_os_error()), &format!("cannot start the administrator step: {}", std::io::Error::last_os_error()))
            });
        }
        if info.hProcess.is_null() {
            return Err(crate::i18n::t("pasul de administrator nu a pornit", "the administrator step did not start"));
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}
