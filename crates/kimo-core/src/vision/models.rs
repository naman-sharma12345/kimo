//! The vision model list (`models.toml`) and the first-use download: resumable over HTTP Range into `<file>.part`,
//! SHA-512 and size checked before the file is moved into place, and a file on disk that does not match its pin is
//! refused (removed, never loaded). Modelled on `crates/kimo-gui/src/download.rs` (ureq + sha2).
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct ModelSpec {
    pub id: String,
    pub file: String,
    pub url: String,
    pub size: u64,
    pub sha512: String,
    pub license: String,
    pub source: String,
}

pub const MODELS_TOML: &str = include_str!("models.toml");

/// Parse the small TOML subset `models.toml` uses: `[[model]]` tables of `key = "string"` / `key = integer`.
pub fn parse_models(src: &str) -> Result<Vec<ModelSpec>, String> {
    let mut out: Vec<std::collections::HashMap<String, String>> = vec![];
    for (ln, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if line == "[[model]]" { out.push(Default::default()); continue; }
        let (k, v) = line.split_once('=').ok_or_else(|| format!("models.toml line {}: expected key = value", ln + 1))?;
        let v = v.trim();
        let v = if let Some(s) = v.strip_prefix('"') { s.strip_suffix('"').ok_or_else(|| format!("models.toml line {}: unterminated string", ln + 1))?.to_string() } else { v.to_string() };
        out.last_mut().ok_or_else(|| format!("models.toml line {}: key outside [[model]]", ln + 1))?.insert(k.trim().to_string(), v);
    }
    out.into_iter().map(|m| {
        let g = |k: &str| m.get(k).cloned().ok_or_else(|| format!("models.toml: a model lacks `{k}`"));
        let spec = ModelSpec { id: g("id")?, file: g("file")?, url: g("url")?, size: g("size")?.parse().map_err(|_| "models.toml: size must be an integer".to_string())?,
            sha512: g("sha512")?.to_ascii_lowercase(), license: g("license")?, source: g("source")? };
        if spec.sha512.len() != 128 || !spec.sha512.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(format!("models.toml: {} sha512 must be 128 hex digits", spec.id)); }
        Ok(spec)
    }).collect()
}

pub fn registry() -> &'static [ModelSpec] {
    static R: std::sync::OnceLock<Vec<ModelSpec>> = std::sync::OnceLock::new();
    R.get_or_init(|| parse_models(MODELS_TOML).expect("src/vision/models.toml is valid"))
}

pub fn spec(id: &str) -> Option<&'static ModelSpec> { registry().iter().find(|m| m.id == id) }

/// `~/.cache/kimo_models/vision`: HOME, or USERPROFILE when HOME is unset. KIMO_VISION_DIR overrides it.
pub fn cache_dir() -> PathBuf { cache_dir_from(std::env::var_os("KIMO_VISION_DIR"), std::env::var_os("HOME"), std::env::var_os("USERPROFILE")) }

pub fn cache_dir_from(over: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>, userprofile: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(d) = over.filter(|d| !d.is_empty()) { return PathBuf::from(d); }
    let home = home.filter(|h| !h.is_empty()).or(userprofile.filter(|h| !h.is_empty())).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join(".cache").join("kimo_models").join("vision")
}

/// KIMO_OFFLINE=1 (or KIMO_VISION_OFFLINE=1): never touch the network; only files already in the cache are used.
pub fn offline() -> bool {
    ["KIMO_OFFLINE", "KIMO_VISION_OFFLINE"].iter().any(|k| std::env::var(k).map(|v| !v.is_empty() && v != "0").unwrap_or(false))
}

#[derive(Debug, PartialEq)]
pub enum ModelError {
    /// Not on disk and the network is off or unreachable.
    Offline(String),
    /// A file (on disk, or freshly downloaded) did not match its SHA-512 / size pin; it was removed.
    Tampered { file: String, expected: String, got: String },
    Http(u16, String),
    Io(String),
    /// Built without the `vision` cargo feature.
    Disabled,
}
impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::Offline(m) => write!(f, "model not downloaded and no connection ({m})"),
            ModelError::Tampered { file, .. } => write!(f, "{file} did not match its SHA-512 pin, so it was refused and removed"),
            ModelError::Http(c, u) => write!(f, "the server answered {c} for {u}"),
            ModelError::Io(m) => write!(f, "could not write the model file: {m}"),
            ModelError::Disabled => write!(f, "this build has no vision models (cargo feature `vision` off)"),
        }
    }
}

#[cfg(feature = "vision")]
pub use net::{ensure, ensure_in, sha512_file};

#[cfg(not(feature = "vision"))]
pub fn ensure(_s: &ModelSpec) -> Result<PathBuf, ModelError> { Err(ModelError::Disabled) }
#[cfg(not(feature = "vision"))]
pub fn ensure_in(_d: &Path, _s: &ModelSpec, _offline: bool) -> Result<PathBuf, ModelError> { Err(ModelError::Disabled) }

#[cfg(feature = "vision")]
mod net {
    use super::*;
    use sha2::{Digest, Sha512};
    use std::io::{Read, Write};
    use std::time::Duration;

    pub fn sha512_file(p: &Path) -> std::io::Result<String> {
        let mut f = std::fs::File::open(p)?; let mut h = Sha512::new(); let mut buf = vec![0u8; 1 << 16];
        loop { let n = f.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); }
        Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Files already verified in this process (path -> size), so a model is hashed once, not on every frame.
    fn verified() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, u64>> {
        static V: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, u64>>> = std::sync::OnceLock::new();
        V.get_or_init(Default::default)
    }

    /// The model's path in the default cache dir, downloading it on first use.
    pub fn ensure(s: &ModelSpec) -> Result<PathBuf, ModelError> { ensure_in(&cache_dir(), s, offline()) }

    /// The model's path under `dir`. A file already there is checked against its pin: a match is used, a mismatch is
    /// refused and removed (then fetched again when online). A download is verified the same way before it is moved in.
    pub fn ensure_in(dir: &Path, s: &ModelSpec, offline: bool) -> Result<PathBuf, ModelError> {
        // one download at a time (two detectors asking for the same model must not write the same .part)
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dest = dir.join(&s.file);
        let mut refused = None;
        if let Ok(md) = dest.metadata() {
            if verified().lock().unwrap_or_else(|e| e.into_inner()).get(&dest) == Some(&md.len()) && md.len() == s.size { return Ok(dest); }
            let got = if md.len() == s.size { sha512_file(&dest).map_err(|e| ModelError::Io(e.to_string()))? } else { format!("size {}", md.len()) };
            if got == s.sha512 { verified().lock().unwrap_or_else(|e| e.into_inner()).insert(dest.clone(), s.size); return Ok(dest); }
            let _ = std::fs::remove_file(&dest);
            verified().lock().unwrap_or_else(|e| e.into_inner()).remove(&dest);
            refused = Some(ModelError::Tampered { file: s.file.clone(), expected: s.sha512.clone(), got });
        }
        if offline { return Err(refused.unwrap_or_else(|| ModelError::Offline("offline mode".into()))); }
        std::fs::create_dir_all(dir).map_err(|e| ModelError::Io(e.to_string()))?;
        let part = dir.join(format!("{}.part", s.file));
        let mut last = ModelError::Offline("no attempt".into());
        for _attempt in 0..3 {
            match fetch_from(&s.url, &part, s.size) {
                Ok(()) => {
                    let got = sha512_file(&part).map_err(|e| ModelError::Io(e.to_string()))?;
                    if got != s.sha512 { let _ = std::fs::remove_file(&part); return Err(ModelError::Tampered { file: s.file.clone(), expected: s.sha512.clone(), got }); }
                    std::fs::rename(&part, &dest).map_err(|e| ModelError::Io(e.to_string()))?;
                    verified().lock().unwrap_or_else(|e| e.into_inner()).insert(dest.clone(), s.size);
                    return Ok(dest);
                }
                Err(e @ ModelError::Http(..)) => return Err(e),
                Err(e) => { last = e; std::thread::sleep(Duration::from_millis(250)); } // dropped: resume from the .part
            }
        }
        Err(last)
    }

    fn fetch_from(url: &str, part: &Path, size: u64) -> Result<(), ModelError> {
        let mut have = part.metadata().map(|m| m.len()).unwrap_or(0);
        if have == size { return Ok(()); }
        if have > size { let _ = std::fs::remove_file(part); have = 0; }
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(30)).build();
        let mut req = agent.get(url);
        if have > 0 { req = req.set("Range", &format!("bytes={have}-")); }
        let resp = match req.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(416, _)) => { let _ = std::fs::remove_file(part); return Err(ModelError::Offline("range refused, restarting".into())); }
            Err(ureq::Error::Status(c, _)) => return Err(ModelError::Http(c, url.to_string())),
            Err(ureq::Error::Transport(t)) => return Err(ModelError::Offline(t.to_string())),
        };
        let resumed = resp.status() == 206;
        let mut f = std::fs::OpenOptions::new().create(true).append(resumed).write(true).truncate(!resumed).open(part).map_err(|e| ModelError::Io(e.to_string()))?;
        let mut done = if resumed { have } else { 0 };
        let mut r = resp.into_reader(); let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = match r.read(&mut buf) { Ok(0) => break, Ok(n) => n, Err(e) => { let _ = f.flush(); return Err(ModelError::Offline(e.to_string())); } };
            if done + n as u64 > size { let _ = f.flush(); drop(f); let _ = std::fs::remove_file(part); return Err(ModelError::Tampered { file: part.display().to_string(), expected: format!("{size} bytes"), got: "more bytes than pinned".into() }); }
            f.write_all(&buf[..n]).map_err(|e| ModelError::Io(e.to_string()))?; done += n as u64;
        }
        f.flush().map_err(|e| ModelError::Io(e.to_string()))?;
        if done < size { return Err(ModelError::Offline(format!("connection dropped at {done} of {size} bytes"))); }
        Ok(())
    }
}
