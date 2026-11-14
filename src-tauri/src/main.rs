#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod battle_records;
mod community;
mod gacha;
mod genshin_wishes;
mod miyoushe_login;
mod mumu;
mod protected;
mod starrail_warps;

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
            app.manage(miyoushe_login::QrLogin::default());
            app.manage(accounts::AccountStore::open(directory)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            accounts::list_accounts,
            accounts::get_battle_records,
            accounts::update_account_card,
            accounts::save_manual_account,
            accounts::remove_account,
            accounts::connect_accounts,
            accounts::refresh_accounts,
            accounts::refresh_account,
            miyoushe_login::start_miyoushe_login,
            miyoushe_login::poll_miyoushe_login,
            miyoushe_login::finish_miyoushe_login,
            miyoushe_login::cancel_miyoushe_login,
            mumu::list_mumu_instances,
            gacha::list_gacha,
            gacha::import_gacha,
            gacha::sync_genshin_wishes,
            gacha::sync_starrail_warps,
            gacha::gacha_page,
        ])
        .run(tauri::generate_context!())
        .expect("Game Daily Hub failed to start");
}
