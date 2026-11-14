//! Tools this computer already has. kimo never downloads a part the user already installed: FFmpeg / FFprobe from
//! PATH or the usual install places (winget, Chocolatey, Scoop, C:\ffmpeg, Program Files, Homebrew, /usr/bin),
//! whisper.cpp's `whisper-cli`, and a Whisper model already on disk (KIMO_WHISPER_MODEL, or ~/.cache/kimo_models,
//! where the engine's own fetch script puts it). Found tools are handed to the engine by exact path, so they work even
//! when they are not on PATH.
use std::path::{Path, PathBuf};

/// The model files the engine accepts, best first (same list as kimo-core's auto_captions).
pub const MODELS: [&str; 4] = ["ggml-large-v3-turbo-q5_0.bin", "ggml-small-q5_1.bin", "ggml-small.bin", "ggml-base-q5_1.bin"];

fn exe(name: &str) -> String { if cfg!(windows) { format!("{name}.exe") } else { name.to_string() } }

fn home() -> Option<PathBuf> { crate::agents::home_dir() }

/// Folders where FFmpeg usually lands when it is installed but not on PATH.
fn ffmpeg_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(windows) {
        if let Some(l) = env("LOCALAPPDATA") {
            let l = l.join("Microsoft").join("WinGet");
            v.push(l.join("Links"));
            // winget's portable FFmpeg: Packages\<id>\ffmpeg-<build>\bin
            if let Ok(rd) = std::fs::read_dir(l.join("Packages")) {
                for pkg in rd.flatten().filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase().contains("ffmpeg")) {
                    v.push(pkg.path().join("bin"));
                    if let Ok(inner) = std::fs::read_dir(pkg.path()) { for d in inner.flatten() { v.push(d.path().join("bin")); } }
                }
            }
        }
        if let Some(p) = env("ProgramData") { v.push(p.join("chocolatey").join("bin")); }
        if let Some(h) = home() { v.push(h.join("scoop").join("shims")); v.push(h.join("scoop").join("apps").join("ffmpeg").join("current").join("bin")); }
        for pf in ["ProgramFiles", "ProgramFiles(x86)"] { if let Some(p) = env(pf) { v.push(p.join("ffmpeg").join("bin")); v.push(p.join("FFmpeg").join("bin")); } }
        v.push(PathBuf::from(r"C:\ffmpeg\bin"));
        v.push(PathBuf::from(r"C:\Program Files\ffmpeg\bin"));
    } else {
        v.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin", "/opt/local/bin"].map(PathBuf::from));
        if let Some(h) = home() { v.push(h.join(".local").join("bin")); }
    }
    v
}

fn first_file(dirs: &[PathBuf], file: &str) -> Option<PathBuf> { dirs.iter().map(|d| d.join(file)).find(|p| p.is_file()) }

/// The engine's own models folder: $KIMO_MODELS, else ~/.cache/kimo_models.
pub fn models_dir() -> Option<PathBuf> {
    std::env::var_os("KIMO_MODELS").map(PathBuf::from).or_else(|| home().map(|h| h.join(".cache").join("kimo_models")))
}

/// Pick the best model file in a folder.
pub fn model_in(dir: &Path) -> Option<PathBuf> { MODELS.iter().map(|m| dir.join(m)).find(|p| p.is_file()) }

/// A part already on this computer (outside kimo's own download folder), by part id.
pub fn find(id: &str) -> Option<PathBuf> {
    match id {
        "ffmpeg" | "ffprobe" => crate::agents::which(id).or_else(|| first_file(&ffmpeg_dirs(), &exe(id))),
        "whisper" => std::env::var_os("KIMO_WHISPER_BIN").map(PathBuf::from).filter(|p| p.is_file())
            .or_else(|| crate::agents::which("whisper-cli"))
            .or_else(|| crate::agents::which("whisper-cpp"))
            .or_else(|| models_dir().map(|d| d.join("bin").join(exe("whisper-cli"))).filter(|p| p.is_file()))
            .or_else(|| if cfg!(windows) { None } else { first_file(&ffmpeg_dirs(), "whisper-cli") }),
        "model" => std::env::var_os("KIMO_WHISPER_MODEL").map(PathBuf::from).filter(|p| p.is_file())
            .or_else(|| models_dir().and_then(|d| model_in(&d))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_the_best_model_and_the_first_existing_tool() {
        let d = std::env::temp_dir().join(format!("kimo-sys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("a")).unwrap();
        std::fs::create_dir_all(d.join("b")).unwrap();
        assert!(model_in(&d).is_none());
        std::fs::write(d.join("ggml-base-q5_1.bin"), b"x").unwrap();
        std::fs::write(d.join("ggml-small-q5_1.bin"), b"x").unwrap();
        assert_eq!(model_in(&d).unwrap().file_name().unwrap(), "ggml-small-q5_1.bin", "the better model wins");
        std::fs::write(d.join("b").join(exe("ffmpeg")), b"x").unwrap();
        assert_eq!(first_file(&[d.join("a"), d.join("b")], &exe("ffmpeg")).unwrap(), d.join("b").join(exe("ffmpeg")));
        assert!(find("nothing").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
