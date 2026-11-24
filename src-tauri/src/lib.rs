//! PioCare — motorul. În versiunea 0.1 totul este DOAR CITIRE:
//! nicio comandă de aici nu modifică registrul, serviciile sau firewall-ul.

mod rules;
mod system;

#[tauri::command]
fn system_overview() -> system::Overview {
    system::overview()
}

#[tauri::command]
fn rules_info() -> rules::RulesInfo {
    rules::info()
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![system_overview, rules_info])
        .run(tauri::generate_context!())
        .expect("PioCare nu a putut porni");
}
