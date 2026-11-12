fn main() {
    let commands = &[
        "list_accounts",
        "save_manual_account",
        "remove_account",
        "connect_accounts",
        "refresh_accounts",
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
