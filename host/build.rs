use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host/ should sit under the project root")
        .to_path_buf();
    let derived = project_root.join(".tacit").join("derived");

    println!("cargo:rerun-if-changed={}", derived.display());
    println!(
        "cargo:rerun-if-changed={}",
        project_root.join("src").join("main.tac").display()
    );

    let (lib_dir, lib_basename, bindings_path) = find_host_artifacts(&derived)
        .expect("no Tacit host artifacts under .tacit/derived — run `tacit interface . --emit-library`");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static={}", lib_basename);

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let staged = out_dir.join("tacit_host.rs");
    let raw = fs::read_to_string(&bindings_path).expect("read generated bindings");
    let stripped: String = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("#!["))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&staged, stripped).expect("write staged bindings");

    println!("cargo:rustc-env=TACIT_BINDINGS_PATH={}", staged.display());
}

fn find_host_artifacts(derived: &Path) -> Option<(PathBuf, String, PathBuf)> {
    for pkg_entry in fs::read_dir(derived).ok()?.flatten() {
        let host_dir = pkg_entry.path().join("host");
        if !host_dir.is_dir() {
            continue;
        }
        let mut archive: Option<(PathBuf, String)> = None;
        let mut bindings: Option<PathBuf> = None;
        for entry in fs::read_dir(&host_dir).ok()?.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.starts_with("libtacit_") && name.ends_with(".a") {
                let basename = name
                    .trim_start_matches("lib")
                    .trim_end_matches(".a")
                    .to_string();
                archive = Some((host_dir.clone(), basename));
            } else if name == "tacit_host.rs" {
                bindings = Some(path);
            }
        }
        if let (Some((dir, base)), Some(b)) = (archive, bindings) {
            return Some((dir, base, b));
        }
    }
    None
}
