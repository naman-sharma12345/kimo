// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/timeline,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Media references in interchange documents: `file://` URLs, absolute and relative paths.
//!
//! Lexical only (no file system access): a document read on one machine names media on another.

fn is_windows_abs(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

pub fn is_absolute(p: &str) -> bool {
    p.starts_with('/') || p.starts_with("\\\\") || is_windows_abs(p)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()))
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = h {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Decode percent escapes in a URI path, including a relative media locator.
/// Invalid escapes remain literal and invalid UTF-8 is replaced.
pub fn decode_uri_path(path: &str) -> String {
    percent_decode(path)
}

/// Percent-encode a URI path while preserving path separators.
pub fn encode_uri_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &c in s.as_bytes() {
        if c.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&c) {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// Convert a `file://` URL (`file:///a/b`, `file://localhost/a/b`, `file:///C:/x`) to a path.
/// Strings that are not file URLs are returned unchanged.
pub fn file_url_to_path(url: &str) -> String {
    let lower = url.get(..7).map(|s| s.to_ascii_lowercase());
    if lower.as_deref() != Some("file://") {
        return url.to_string();
    }
    let rest = &url[7..];
    let is_localhost = rest.get(..9).is_some_and(|s| s.eq_ignore_ascii_case("localhost"))
        && rest.get(9..).is_some_and(|after| after.is_empty() || after.starts_with('/'));
    let rest = if is_localhost { &rest[9..] } else { rest };
    let decoded = percent_decode(rest);
    // file:///C:/x -> C:/x
    if decoded.len() >= 3 && decoded.starts_with('/') && is_windows_abs(&decoded[1..]) {
        return decoded[1..].to_string();
    }
    if !decoded.starts_with('/') {
        // file://server/share -> UNC
        return format!("//{decoded}");
    }
    decoded
}

/// Convert an absolute path to a `file://` URL (`localhost` form for FCP7 XML).
pub fn path_to_file_url(path: &str, localhost: bool) -> String {
    let p = path.replace('\\', "/");
    // Windows verbatim paths (`\\?\C:\x`, `\\?\UNC\server\share`, as `canonicalize` returns them)
    // are not network shares: drop the prefix so `?` doesn't become the URL's host.
    let p = if let Some(unc) = p.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else if let Some(local) = p.strip_prefix("//?/") {
        local.to_string()
    } else {
        p
    };
    let host = if localhost { "localhost" } else { "" };
    if is_windows_abs(&p) {
        format!("file://{host}/{}", encode_uri_path(&p))
    } else if let Some(unc) = p.strip_prefix("//") {
        format!("file://{}", encode_uri_path(unc))
    } else {
        format!("file://{host}{}", encode_uri_path(&p))
    }
}

/// Join `rel`, a relative media reference, to `base`, the folder of the document that names it.
/// Lexical, like `craft-docpath`, and with the same roots: none (a relative folder), `/`, a drive
/// (`C:\`, or `C:` alone for a drive-relative folder), a share (`\\host\share`, `//host/share`) or a
/// `\\?\` prefix with its drive or `UNC\host\share`. Below the root, empty names (repeated
/// separators) and `.` are dropped, and a `..` takes off the last name: at the root of an absolute
/// folder it is dropped, and past the start of a relative one it is kept. The folder's own `.` and
/// `..` go through the same rule, so a reference starts from where the folder really is. The result
/// keeps the folder's separator: `\` when it has only those, else `/` (filmcraft#3998).
fn join(base: &str, rel: &str) -> String {
    let sep = if base.contains('\\') && !base.contains('/') { '\\' } else { '/' };
    let (root, absolute, rest) = split_root(base);
    let mut names: Vec<&str> = Vec::new();
    for name in rest.split(['/', '\\']).chain(rel.split(['/', '\\'])) {
        match name {
            "" | "." => {}
            ".." => match names.last() {
                Some(&last) if last != ".." => {
                    names.pop();
                }
                _ if absolute => {}
                _ => names.push(".."),
            },
            n => names.push(n),
        }
    }
    let body = names.join(&sep.to_string());
    if absolute {
        format!("{root}{sep}{body}")
    } else if root.is_empty() && body.is_empty() {
        ".".to_string()
    } else {
        format!("{root}{body}")
    }
}

/// The root `p` starts with, as written (without its last separator), whether it makes `p`
/// absolute, and the rest of `p`.
fn split_root(p: &str) -> (&str, bool, &str) {
    let is_sep = |c: char| c == '/' || c == '\\';
    let seps: Vec<usize> = p.char_indices().filter(|&(_, c)| is_sep(c)).map(|(i, _)| i).collect();
    if seps.starts_with(&[0, 1]) {
        // `\\host\share` and `\\?\C:` take two names after the leading separators,
        // `\\?\UNC\host\share` four. A name runs to the next separator, or to the end.
        let names: Vec<&str> = p.get(2..).unwrap_or_default().split(is_sep).collect();
        let count = match (names.first().copied(), names.get(1)) {
            (Some("?" | "."), Some(u)) if u.eq_ignore_ascii_case("UNC") => 4,
            _ => 2,
        };
        let end = seps.iter().copied().filter(|&i| i >= 2).nth(count - 1).unwrap_or(p.len());
        let (root, rest) = p.split_at(end);
        return (root, true, rest);
    }
    if seps.first() == Some(&0) {
        return ("", true, p);
    }
    let b = p.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let (root, rest) = p.split_at(2);
        return (root, seps.first() == Some(&2), rest);
    }
    ("", false, p)
}

/// Resolve a media URL from a document (FCPXML `src`, FCP7 `pathurl`, OTIO `target_url`): like
/// [`resolve_path`], with a relative URL percent-decoded as a `file://` URL is (`Clip%20A.mov` is
/// `Clip A.mov`, filmcraft#2195). Plain file names (EDL, AAF) go through [`resolve_path`] untouched.
pub fn resolve_url(reference: &str, base_dir: Option<&str>) -> String {
    let r = reference.trim();
    let is_file_url = r.get(..5).is_some_and(|s| s.eq_ignore_ascii_case("file:"));
    if is_file_url || is_absolute(r) { resolve_path(r, base_dir) } else { resolve_path(&percent_decode(r), base_dir) }
}

/// A relative path written as a URL (the inverse of [`resolve_url`]'s decoding).
pub fn relative_url(path: &str) -> String {
    encode_uri_path(path)
}

/// Resolve a media reference from a document: file URLs become paths, relative paths are joined to
/// `base_dir` (when given).
pub fn resolve_path(reference: &str, base_dir: Option<&str>) -> String {
    let p = file_url_to_path(reference.trim());
    if is_absolute(&p) {
        return p;
    }
    match base_dir {
        Some(b) if !b.is_empty() => join(b, &p),
        _ => p,
    }
}

/// `path` relative to directory `base`, if `path` lies under it (or shares a prefix).
pub fn relative_path(path: &str, base: &str) -> Option<String> {
    if !is_absolute(path) || !is_absolute(base) {
        return None;
    }
    let a: Vec<&str> = path.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let b: Vec<&str> = base.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return None;
    }
    let mut out: Vec<&str> = std::iter::repeat_n("..", b.len() - common).collect();
    out.extend(&a[common..]);
    Some(out.join("/"))
}

/// Final path component.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// File name without extension.
pub fn file_stem(path: &str) -> &str {
    let n = file_name(path);
    match n.rfind('.') {
        Some(i) if i > 0 => &n[..i],
        _ => n,
    }
}

pub fn extension(path: &str) -> String {
    let n = file_name(path);
    n.rfind('.').map(|i| n[i + 1..].to_ascii_lowercase()).unwrap_or_default()
}


