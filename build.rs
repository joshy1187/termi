use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let background = manifest.join("assets/backgrounds/galactic.png");
    let fallback = manifest.join("assets/backgrounds/galactic-fallback.png");

    if !background.exists() {
        fs::copy(&fallback, &background).expect("failed to install fallback galactic background");
        println!(
            "cargo:warning=assets/backgrounds/galactic.png was missing; using the bundled fallback"
        );
    }

    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=assets/backgrounds/galactic.png");
    println!("cargo:rerun-if-changed=assets/backgrounds/galactic-fallback.png");

    slint_build::compile("ui/app-window.slint")
        .expect("failed to compile the Termi Slint interface");
}
