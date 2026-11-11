fn main() {
    // libwebrtc's static Objective-C categories must survive dead stripping in
    // the final macOS executable, including Rust unit-test executables. Linker
    // arguments from dependency build scripts do not propagate to consumers.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-ObjC");
    }
}
