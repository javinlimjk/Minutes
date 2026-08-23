fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        std::env::set_var("MACOSX_DEPLOYMENT_TARGET", "10.15");
        std::env::set_var("CXXFLAGS", "-mmacosx-version-min=10.15");
        std::env::set_var("CFLAGS", "-mmacosx-version-min=10.15");
        // Re-link whenever Info.plist changes so the embedded plist stays fresh.
        println!("cargo:rerun-if-changed=Info.plist");
        println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,Info.plist");
    } else if target_os == "ios" {
        println!("cargo:rerun-if-changed=src/stt_ios.m");
        cc::Build::new()
            .file("src/stt_ios.m")
            .flag("-fobjc-arc")
            .compile("stt_ios_native");
        println!("cargo:rustc-link-lib=framework=Speech");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
    tauri_build::build()
}


