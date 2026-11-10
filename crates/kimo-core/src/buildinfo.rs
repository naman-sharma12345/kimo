//! RV-2: one build identity used by the MCP server (initialize + kimo_info), saved project files and export reports.
use serde_json::{json, Value};

/// Semver of this build (workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short git commit the binary was built from, "-dirty" when the work tree had uncommitted changes, "unknown" outside a git checkout.
pub const COMMIT: &str = env!("KIMO_GIT_COMMIT");
/// Version of the saved project file layout. Bump when old binaries could misread a new file; load_project refuses files with a newer schema.
pub const SCHEMA_VERSION: u32 = 2;

fn tool_version(bin: &str) -> Value {
    match std::process::Command::new(bin).arg("-version").output() {
        Ok(o) if o.status.success() => { let t = String::from_utf8_lossy(&o.stdout); let first = t.lines().next().unwrap_or("").to_string(); json!({"available": true, "version": first.split_whitespace().nth(2).unwrap_or("").to_string(), "banner": first}) }
        Ok(_) => json!({"available": false, "error": "exited with an error"}),
        Err(e) => json!({"available": false, "error": e.to_string()}),
    }
}

/// Short identity stamped into project files and export reports.
pub fn identity() -> Value { json!({"version": VERSION, "commit": COMMIT, "schema_version": SCHEMA_VERSION}) }

/// Full report for kimo_info: identity plus enabled features and backend versions.
pub fn info() -> Value {
    static BACKENDS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let backends = BACKENDS.get_or_init(|| json!({"ffmpeg": tool_version("ffmpeg"), "ffprobe": tool_version("ffprobe")})).clone();
    json!({"name": "kimo", "version": VERSION, "commit": COMMIT, "schema_version": SCHEMA_VERSION,
        "features": {"native_ocio": cfg!(feature = "native-ocio")}, "backends": backends,
        "tools": crate::commands::registry_ref().len(), "extension_modules": crate::ext::MODULES.len(),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" }})
}

/// Add the identity to an object result (export reports); non-objects pass through unchanged.
pub fn stamp(mut v: Value) -> Value { if let Some(o) = v.as_object_mut() { o.insert("kimo".into(), identity()); } v }

/// The "kimo" block written into saved projects: schema_version, created_with (kept from the loaded file) and last_saved_with. None when KIMO_NO_IDENTITY=1
/// (byte-for-byte compatible with files written before build identity existed).
pub fn saved_block(created_with: &mut Option<Value>) -> Option<Value> {
    if std::env::var("KIMO_NO_IDENTITY").map_or(false, |v| v == "1") { return None; }
    let me = json!({"version": VERSION, "commit": COMMIT});
    let created = created_with.get_or_insert_with(|| me.clone()).clone();
    Some(json!({"schema_version": SCHEMA_VERSION, "created_with": created, "last_saved_with": me}))
}
