// The app manifest names every IPC command the webview may call; tauri-build turns each into an `allow-<command>`
// permission, and capabilities/main.json grants them to the main window only. The spectator window gets none.
const COMMANDS: &[&str] = &[
    "app_state", "engine_restart", "engine_call", "engine_read", "changes_list", "change_undo", "project_open",
    "project_save", "project_new", "pick", "media_thumb", "open_url", "reveal", "cli_status", "cli_signin", "cli_install",
    "cli_register", "cli_models", "agent_select", "agent_send", "agent_stop", "agent_reset", "mcp_set", "mcp_probe", "key_set",
    "key_clear", "key_test", "settings_set", "onboarded_set", "open_spectator", "setup_status", "setup_start",
    "setup_pause", "setup_skip", "viewer_frame", "viewer_draft",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("tauri-build failed");
}
