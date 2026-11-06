// Gives the Windows .exe the "S" icon and file details.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Kincaid Label Generator")
            .set("FileDescription", "Kincaid Label Generator")
            .set("CompanyName", "Adam Kincaid")
            .set("LegalCopyright", "Developed by Adam Kincaid");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not add the Windows icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
