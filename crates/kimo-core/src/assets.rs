//! Shared durable asset storage. Caller supplies a fixed engine-owned kind folder.
pub fn store_dir(
    project: &crate::model::Project,
    kind: &str,
) -> Result<std::path::PathBuf, String> {
    store_dir_in(project, None, kind)
}
/// Like `store_dir`, with an explicit root (`Session::asset_dir`) that wins over the KIMO_ASSET_DIR environment variable.
pub fn store_dir_in(
    project: &crate::model::Project,
    asset_dir: Option<&std::path::Path>,
    kind: &str,
) -> Result<std::path::PathBuf, String> {
    if kind.is_empty()
        || !kind
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid asset storage kind".into());
    }
    let root = if let Some(base) = &project.base_dir {
        std::path::Path::new(base).join("assets")
    } else if let Some(base) = asset_dir {
        base.to_path_buf()
    } else if let Some(base) = std::env::var_os("KIMO_ASSET_DIR") {
        std::path::PathBuf::from(base)
    } else if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
        std::path::PathBuf::from(base).join("kimo/assets")
    } else if let Some(home) = std::env::var_os("HOME") {
        std::path::PathBuf::from(home).join(".local/share/kimo/assets")
    } else {
        return Err(
            "asset storage requires project base_dir, KIMO_ASSET_DIR, XDG_DATA_HOME or HOME".into(),
        );
    };
    let path = root.join(kind);
    std::fs::create_dir_all(&path).map_err(|e| format!("asset storage: {e}"))?;
    Ok(path)
}
