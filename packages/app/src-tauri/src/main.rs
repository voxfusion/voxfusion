#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    if voxfusion_app_lib::linux::handle_cli() {
        return;
    }
    voxfusion_app_lib::run()
}
