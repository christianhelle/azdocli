#[cfg(windows)]
extern crate winres;

fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("images/icon.ico");
        res.compile()
            .expect("Failed to compile Windows icon resource");
    }
}
