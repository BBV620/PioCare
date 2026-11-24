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
        Ok(s) if s.starts_with("CREAT") => "punct de restaurare creat".into(),
        Ok(s) => format!("punct de restaurare necreat ({})", s.lines().last().unwrap_or("").trim()),
        Err(e) => format!("punct de restaurare necreat ({e})"),
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
        Some(_) => Err("Windows nu a schimbat starea Protecției sistemului".into()),
        None => Ok(()),
    }
}

/// Export complet al politicii Windows Firewall (cere administrator).
pub fn export_firewall(file: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(file);
    run_hidden("netsh.exe", &["advfirewall", "export", &file.to_string_lossy()])?;
    if file.exists() { Ok(()) } else { Err("netsh nu a produs fișierul".into()) }
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
        info.hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if ShellExecuteExW(&mut info) == 0 {
            return Err(if GetLastError() == 1223 {
                "ai refuzat cererea de administrator; nu s-a schimbat nimic".into()
            } else {
                format!("nu pot porni pasul de administrator: {}", std::io::Error::last_os_error())
            });
        }
        if info.hProcess.is_null() {
            return Err("pasul de administrator nu a pornit".into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}
