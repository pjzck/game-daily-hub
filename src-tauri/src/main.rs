#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod community;
mod gacha;
mod mumu;
mod protected;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let directory = match std::env::var_os("GAME_DAILY_HUB_DATA_DIR") {
                Some(path) => std::path::PathBuf::from(path),
                None => app.path().app_local_data_dir()?,
            };
            app.manage(gacha::GachaStore::new(&directory));
            app.manage(accounts::AccountStore::open(directory)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            accounts::list_accounts,
            accounts::save_manual_account,
            accounts::remove_account,
            accounts::connect_accounts,
            accounts::refresh_accounts,
            mumu::list_mumu_instances,
            gacha::list_gacha,
            gacha::import_gacha,
            gacha::gacha_page,
        ])
        .run(tauri::generate_context!())
        .expect("Game Daily Hub failed to start");
}
