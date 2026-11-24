//! Citiri despre sistem (doar citire).

use serde::Serialize;

#[derive(Serialize, Default)]
pub struct Overview {
    pub windows_name: String,
    pub windows_version: String,
    pub build: String,
    pub computer_name: String,
    pub uptime_hours: f64,
    pub disk_c_free_gb: f64,
    pub disk_c_total_gb: f64,
    pub installed_programs: usize,
}

pub fn overview() -> Overview {
    let mut o = Overview::default();
    o.computer_name = std::env::var("COMPUTERNAME").unwrap_or_default();
    #[cfg(windows)]
    win::fill(&mut o);
    o
}

#[cfg(windows)]
mod win {
    use super::Overview;
    use std::collections::HashSet;
    use winreg::enums::*;
    use winreg::RegKey;

    pub fn fill(o: &mut Overview) {
        windows_version(o);
        disk_c(o);
        o.uptime_hours =
            unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() } as f64
                / 3_600_000.0;
        o.installed_programs = installed_programs();
    }

    fn windows_version(o: &mut Overview) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let Ok(k) = hklm.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion") else {
            return;
        };
        let product: String = k.get_value("ProductName").unwrap_or_default();
        let display: String = k.get_value("DisplayVersion").unwrap_or_default();
        let build: String = k.get_value("CurrentBuild").unwrap_or_default();
        let ubr: u32 = k.get_value("UBR").unwrap_or(0);
        // Pe Windows 11, ProductName încă spune „Windows 10”: decide după build.
        let n: u32 = build.parse().unwrap_or(0);
        o.windows_name = if n >= 22000 {
            product.replace("Windows 10", "Windows 11")
        } else {
            product
        };
        o.windows_version = display;
        o.build = if ubr > 0 { format!("{build}.{ubr}") } else { build };
    }

    fn disk_c(o: &mut Overview) {
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let path: Vec<u16> = "C:\\\0".encode_utf16().collect();
        let (mut free, mut total, mut _tf) = (0u64, 0u64, 0u64);
        let ok = unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut free, &mut total, &mut _tf) };
        if ok != 0 {
            const GB: f64 = 1024.0 * 1024.0 * 1024.0;
            o.disk_c_free_gb = free as f64 / GB;
            o.disk_c_total_gb = total as f64 / GB;
        }
    }

    /// Numără programele din cele trei locuri „Uninstall” (64 biți, 32 biți, utilizator),
    /// fără componente de sistem și fără actualizări. Baza viitorului modul „Registru curat”.
    fn installed_programs() -> usize {
        let sources = [
            (HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
            (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
            (HKEY_CURRENT_USER, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ];
        let mut names = HashSet::new();
        for (root, path) in sources {
            let Ok(key) = RegKey::predef(root).open_subkey(path) else { continue };
            for sub in key.enum_keys().flatten() {
                let Ok(app) = key.open_subkey(&sub) else { continue };
                let name: String = app.get_value("DisplayName").unwrap_or_default();
                let system: u32 = app.get_value("SystemComponent").unwrap_or(0);
                let parent: Result<String, _> = app.get_value("ParentKeyName");
                if !name.trim().is_empty() && system == 0 && parent.is_err() {
                    names.insert(name.trim().to_lowercase());
                }
            }
        }
        names.len()
    }
}
