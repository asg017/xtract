use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=js/entry.js");
    println!("cargo:rerun-if-changed=js/node_modules/zod");

    let status = Command::new("npx")
        .args([
            "esbuild",
            "js/entry.js",
            "--bundle",
            "--format=iife",
            "--platform=neutral",
            "--outfile=js/bundle.js",
        ])
        .status()
        .expect("failed to run esbuild");

    assert!(status.success(), "esbuild failed");
}
