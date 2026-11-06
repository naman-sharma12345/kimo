// The app manifest names every IPC command the webview may call; tauri-build turns each into an `allow-<command>`
// permission, and capabilities/main.json grants them to the main window only. The spectator window gets none.
const COMMANDS: &[&str] = &[
    "app_state", "engine_restart", "mcp_set", "mcp_probe", "agents_detect", "agent_add",
    "key_set", "key_clear", "key_test", "settings_set", "chat_send", "chat_undo", "chat_reset",
    "project_open", "open_spectator", "setup_status", "setup_start", "setup_pause", "setup_skip",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("tauri-build failed");
}
