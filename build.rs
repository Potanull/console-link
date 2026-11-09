fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/icon.ico")
        .set("ProductName", "Console Link")
        .set("FileDescription", "Console Link")
        .set("LegalCopyright", "Copyright (c) 2026 Potanull")
        .compile()
        .expect("compile Windows application icon");
}
