use std::env;
use std::fs;
use std::path::Path;

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

    if env::var("PROFILE").as_deref() == Ok("release") {
        validate_release_build_mode();
        validate_frontend_dist();
    }

    tauri_build::build()
}

fn validate_release_build_mode() {
    if env::var("DEP_TAURI_DEV").as_deref() == Ok("false") {
        return;
    }

    panic!(
        "Cassady desktop release binaries must be built with `cargo tauri build --no-bundle` so Tauri enables custom-protocol asset embedding. Plain `cargo build --release -p cassady-desktop` builds a dev-url binary that opens as a blank window when the Vite server is not running."
    );
}

fn validate_frontend_dist() {
    let index = Path::new("../dist/index.html");
    let assets_dir = Path::new("../dist/assets");

    if !index.is_file() {
        panic!(
            "Cassady desktop release builds require built frontend assets. Run `npm install` and `npm run build` in cassady-desktop, or use `cargo tauri build --no-bundle`, before building the desktop binary."
        );
    }

    let html = fs::read_to_string(index).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", index.display());
    });
    if !html.contains("assets/index-") {
        panic!(
            "{} does not reference the Vite-built frontend assets. Run `npm run build` in cassady-desktop before building the release desktop binary.",
            index.display()
        );
    }

    if !assets_dir.is_dir() {
        panic!(
            "{} is missing. Run `npm run build` in cassady-desktop before building the release desktop binary.",
            assets_dir.display()
        );
    }

    let mut has_js = false;
    let mut has_css = false;
    for entry in fs::read_dir(assets_dir).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", assets_dir.display());
    }) {
        let path = entry
            .unwrap_or_else(|error| panic!("failed to read asset entry: {error}"))
            .path();
        has_js |= path.extension().and_then(|extension| extension.to_str()) == Some("js");
        has_css |= path.extension().and_then(|extension| extension.to_str()) == Some("css");
    }

    if !has_js || !has_css {
        panic!(
            "{} must contain Vite-built JavaScript and CSS assets. Run `npm run build` in cassady-desktop before building the release desktop binary.",
            assets_dir.display()
        );
    }
}
