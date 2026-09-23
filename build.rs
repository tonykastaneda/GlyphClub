fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("branding/windows/GlyphClub.ico");
        if let Err(e) = res.compile() {
            eprintln!("warning: failed to embed Windows .exe icon: {e}");
        }
    }
}
