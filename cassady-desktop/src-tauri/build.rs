fn main() {
    // Tauri embeds the built frontend into release binaries. `cargo tauri build`
    // runs the frontend build before Cargo, so make Cargo rerun this build script
    // whenever the generated frontend changes; otherwise a standalone desktop
    // binary can accidentally contain stale or missing assets and open to a
    // blank WebView.
    println!("cargo:rerun-if-changed=../dist/index.html");
    println!("cargo:rerun-if-changed=../dist/assets");
    println!("cargo:rerun-if-changed=../index.html");
    println!("cargo:rerun-if-changed=../src");
    println!("cargo:rerun-if-changed=../vite.config.ts");
    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../package-lock.json");
    tauri_build::build()
}
