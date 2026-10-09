use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    // Path to target/{debug,release}
    let target_dir = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let src_dll = Path::new(&manifest_dir).join("models").join("onnxruntime.dll");

    if src_dll.exists() {
        let dest_dll = target_dir.join("onnxruntime.dll");
        let _ = fs::copy(&src_dll, &dest_dll);
    }
}
