fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/nvim.ico");
        res.compile().expect("Could not attach exe icon");
    }
}
