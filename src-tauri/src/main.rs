// Fără fereastră de consolă în release pe Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    piocare_lib::run()
}
