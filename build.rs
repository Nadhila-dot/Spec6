use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let vite_manifest_dir = manifest_dir.join("src/frontend/dist/client/.vite");
    let vite_manifest_path = vite_manifest_dir.join("manifest.json");
    let keep_path = manifest_dir.join("src/frontend/dist/client/.keep");

    fs::create_dir_all(&vite_manifest_dir).expect("failed to create Vite manifest directory");

    if !vite_manifest_path.exists() {
        fs::write(&vite_manifest_path, "{}\n").expect("failed to write placeholder Vite manifest");
    }

    if !keep_path.exists() {
        fs::write(&keep_path, "\n").expect("failed to write dist keep file");
    }

    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=src/frontend/dist/client");
    println!("cargo:rerun-if-changed=src/frontend/public");
}
