use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_path = Path::new(&dir).parent().unwrap().join("lib");

    println!("cargo::rerun-if-changed={}", lib_path.display());
    println!("cargo:rustc-link-search=native={}", lib_path.display());

    let build_output = Command::new("bash")
        .arg(lib_path.join("build_lib.sh"))
        .output()
        .expect("could not run build_lib.sh");
    println!("cargo:warning=\r    {}     ", String::from_utf8_lossy(&build_output.stdout));
}