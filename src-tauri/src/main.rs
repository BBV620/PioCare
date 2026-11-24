// Fără fereastră de consolă în release pe Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 4 && args[1] == "--elevated" {
        std::process::exit(piocare_lib::elevated_main(&args[2], &args[3]));
    }
    piocare_lib::run()
}
