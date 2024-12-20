use std::env;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let debug = env::var("DEBUG").unwrap();
    let target = env::var("TARGET").unwrap();
    let lib_path = Path::new(&dir).parent().unwrap().join("lib");

    let mut lib_builder_args = Vec::new();
    lib_builder_args.push(lib_path.join("build_lib.py").display().to_string());
    if debug == "false" {
        lib_builder_args.push(String::from_str("--release").unwrap());
    }
    lib_builder_args.push(format!("--target={}", target));
    
    let mut lib_ext = "a";
    if target == "x86_64-pc-windows-gnu" {
        lib_ext = "lib";
    }
    
    println!("cargo::rerun-if-changed={}", lib_path.display());
    println!("cargo::rustc-link-search=native={}/", lib_path.join("build").display());
    println!("cargo::rustc-link-lib=static:+verbatim=libap2p.{}", lib_ext);
    println!("cargo::rustc-link-lib=static:+verbatim=libsqlite3.{}", lib_ext);
    
    let _ = Command::new("python3")
        .args(lib_builder_args)
        .output()
        .expect("could not run build_lib.py");
    println!("cargo:warning=\r    \x1b[32mRan\x1b[0m {}/build_lib.py         ", lib_path.display());
}