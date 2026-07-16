fn main() {
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=assets/backgrounds/galactic.png");

    slint_build::compile("ui/app-window.slint")
        .expect("failed to compile the Termi Slint interface");
}
