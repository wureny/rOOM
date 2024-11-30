use bindgen;
use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h");

    // 根据目标平台添加不同的配置
    if target_os == "macos" {
        // macOS 特定配置
        builder = builder
            .clang_arg("-I/usr/include")
            .clang_arg("-I/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include");
    } else {
        // Linux 交叉编译配置
        builder = builder
            .clang_arg("--target=x86_64-unknown-linux-musl")
            .clang_arg("-I/usr/local/include")
            .clang_arg("-I/usr/x86_64-linux-musl/include");
    }

    let bindings = builder
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
} 