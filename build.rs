//! Embeds the application icon and version information into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/vera-view.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/vera-view.ico")
        .set("ProductName", "Vera View")
        .set("FileDescription", "Vera View - Visio drawing viewer")
        .set("OriginalFilename", "vera-view.exe")
        .set("LegalCopyright", "")
        .set("LegalTrademarks", "Microsoft and Visio are trademarks of the Microsoft group of companies.");
    if let Err(e) = res.compile() {
        // A missing resource compiler shouldn't block development builds.
        println!("cargo:warning=Could not embed the Windows icon: {e}");
    }
}
