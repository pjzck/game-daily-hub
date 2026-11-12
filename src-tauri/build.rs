fn main() {
    let commands = &[
        "list_accounts",
        "get_battle_records",
        "update_account_card",
        "save_manual_account",
        "remove_account",
        "connect_accounts",
        "refresh_accounts",
        "start_miyoushe_login",
        "poll_miyoushe_login",
        "finish_miyoushe_login",
        "cancel_miyoushe_login",
        "list_mumu_instances",
        "list_gacha",
        "import_gacha",
        "gacha_page",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("could not build Tauri application metadata");
}
