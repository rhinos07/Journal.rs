//! Unter Windows Icon und Versionsinfo in die Exe einbetten (Explorer, Taskleiste).
//! Das Icon wird mit `cargo run -p journal-app --example icon` erzeugt.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico")
        .set("ProductName", "Tagebuch")
        .set("FileDescription", "Trainingstagebuch")
        .set("OriginalFilename", "trainingstagebuch.exe");
    if let Err(e) = res.compile() {
        // Ohne Windows SDK (rc.exe) lieber ohne Icon bauen als gar nicht.
        println!("cargo:warning=Icon konnte nicht eingebettet werden: {e}");
    }
}
