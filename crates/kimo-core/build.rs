// Auto-discovers extension modules: every src/ext/<name>.rs (and src/ext/<name>/mod.rs) becomes `ext::<name>`,
// and its `pub fn commands() -> Vec<Command>` is added to the registry. New tools never need to edit an existing file.
use std::{env, fs, path::PathBuf};
fn main() {
    build_ocio(); build_manifold(); embed_git_commit();
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/ext"); println!("cargo:rerun-if-changed=src/ext");
    let mut names: Vec<(String, PathBuf)> = vec![];
    if let Ok(rd) = fs::read_dir(&dir) { for e in rd.flatten() { let p = e.path(); let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if stem == "mod" || stem.starts_with('.') || stem.is_empty() { continue; }
        if p.is_dir() { if p.join("mod.rs").exists() { names.push((stem, p.join("mod.rs"))); } } else if p.extension().and_then(|s| s.to_str()) == Some("rs") { names.push((stem, p)); } } }
    names.sort(); let mut out = String::new();
    for (n, p) in &names { out += &format!("#[path = {:?}] pub mod {};\n", p.to_string_lossy(), n); }
    out += "pub fn commands() -> Vec<crate::commands::Command> { let mut v = Vec::new();\n"; for (n, _) in &names { out += &format!("    v.extend({}::commands());\n", n); } out += "    v }\n";
    out += &format!("pub const MODULES: &[&str] = &[{}];\n", names.iter().map(|(n, _)| format!("{:?}", n)).collect::<Vec<_>>().join(", "));
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("ext_gen.rs"), out).unwrap();
}

fn build_ocio() {
    println!("cargo:rerun-if-changed=src/color/ocio_native.cpp");
    for name in ["OCIO_INCLUDE_DIR", "OCIO_LIB_DIR"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    if std::env::var_os("CARGO_FEATURE_NATIVE_OCIO").is_none() { return; }
    let include = std::env::var("OCIO_INCLUDE_DIR").expect("native-ocio requires OCIO_INCLUDE_DIR (OCIO 2 headers)");
    let lib = std::env::var("OCIO_LIB_DIR").expect("native-ocio requires OCIO_LIB_DIR (shared OCIO 2 library)");
    cc::Build::new().cpp(true).std("c++17").cpp_link_stdlib(None).include(include)
        .file("src/color/ocio_native.cpp").compile("kimo_ocio_native");
    println!("cargo:rustc-link-search=native={lib}");
    println!("cargo:rustc-link-lib=dylib=OpenColorIO");
}

/// KIMO_GIT_COMMIT = short commit of the checkout being built ("-dirty" with uncommitted changes, "unknown" outside git). Reruns when the checkout's HEAD log changes.
fn embed_git_commit() {
    let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(env::var("CARGO_MANIFEST_DIR").unwrap()).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let commit = match git(&["rev-parse", "--short=12", "HEAD"]) {
        Some(c) if !c.is_empty() => { let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty()).unwrap_or(false); if dirty { format!("{c}-dirty") } else { c } }
        _ => "unknown".to_string(),
    };
    if let Some(gd) = git(&["rev-parse", "--absolute-git-dir"]) { println!("cargo:rerun-if-changed={gd}/logs/HEAD"); println!("cargo:rerun-if-changed={gd}/HEAD"); }
    println!("cargo:rustc-env=KIMO_GIT_COMMIT={commit}");
}

/// Builds vendored Manifold 3.5.3 (Apache-2.0, vendor/manifold-3.5.3) as a static lib with
/// cross-sections/TBB/bindings/tests OFF, serial (`--parallel 1`, generator-neutral so MSBuild accepts it; small box), then compiles the C ABI shim
/// (src/threed/manifold_shim.cpp) with the existing `cc` build-dep. No new dependencies.
fn build_manifold() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // No canonicalize(): on Windows it returns a `\\?\` / `\\?\UNC\` verbatim path that MSBuild/cl cannot open (C1083).
    // CARGO_MANIFEST_DIR is already absolute, so walk up two plain components instead of joining "../..".
    let src = manifest.parent().and_then(|p| p.parent()).expect("workspace root above crates/kimo-core").join("vendor").join("manifold-3.5.3");
    assert!(src.join("CMakeLists.txt").is_file(), "vendor/manifold-3.5.3 missing at {}", src.display());
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed=src/threed/manifold_shim.cpp");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("manifold-build");
    let cfg = std::process::Command::new("cmake")
        .args(["-S"]).arg(&src).args(["-B"]).arg(&out)
        .args(["-DCMAKE_BUILD_TYPE=Release","-DBUILD_SHARED_LIBS=OFF","-DMANIFOLD_CROSS_SECTION=OFF","-DMANIFOLD_PAR=OFF","-DMANIFOLD_PYBIND=OFF","-DMANIFOLD_JSBIND=OFF","-DMANIFOLD_CBIND=OFF","-DMANIFOLD_TEST=OFF","-DMANIFOLD_DOWNLOADS=OFF","-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
            // MSVC: pin the dynamic release CRT (/MD) to match the cc-built shim and Rust std; ignored by non-MSVC toolchains.
            "-DCMAKE_POLICY_DEFAULT_CMP0091=NEW","-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL"])
        .status().expect("cmake not found (required for vendored manifold)");
    assert!(cfg.success(), "manifold cmake configure failed");
    let build = std::process::Command::new("cmake").args(["--build"]).arg(&out).args(["--config","Release","--parallel","1"]).status().expect("manifold build failed to start");
    assert!(build.success(), "manifold build failed");
    cc::Build::new().cpp(true).std("c++17").cpp_link_stdlib(None).include(src.join("include")).include(out.join("include"))
        .file("src/threed/manifold_shim.cpp").compile("kimo_manifold_shim");
    // Single-config generators (Makefiles/Ninja) put the lib in out/src; Visual Studio's multi-config
    // generator puts it in out/src/<Config>, so search both and build Release explicitly (VS defaults to Debug,
    // whose /MDd CRT would not link against the /MD shim and Rust std).
    // MSVC: manifold's exportlib() sets ARCHIVE_OUTPUT_DIRECTORY to <build>/lib, so the lib lands in out/lib/Release.
    for dir in [out.join("src"), out.join("src").join("Release"), out.join("lib"), out.join("lib").join("Release")] { if dir.exists() { println!("cargo:rustc-link-search=native={}", dir.display()); } }
    println!("cargo:rustc-link-lib=static=manifold");
    link_cpp_runtime();
}

/// Links the C++ runtime exactly once, after every C++ static lib (the `cc` builds above opt out of
/// cc's own stdlib link). Reads the TARGET's os/env from Cargo, not cfg!(), because this script runs
/// on the host: libc++ on Apple, nothing on MSVC (the CRT carries it), libstdc++ everywhere else
/// (Linux gnu and musl included).
fn link_cpp_runtime() {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let lib = match (os.as_str(), target_env.as_str()) {
        (_, "msvc") => None,
        ("macos" | "ios", _) => Some("c++"),
        _ => Some("stdc++"),
    };
    if let Some(lib) = lib { println!("cargo:rustc-link-lib=dylib={lib}"); }
}
