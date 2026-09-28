fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("branding/icon.ico");
        res.set_manifest_file("app.manifest");
        res.compile().unwrap();
    }
}
