use std::os::raw::c_void;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::NSData;
use objc2_foundation::NSString;

use crate::utils::expand_tilde;

static DEFAULT_NEOVIDE_ICON_BYTES: &[u8] =
    include_bytes!("../../../../extra/osx/Neovide.app/Contents/Resources/Neovide.icns");

fn load_icon_from_custom_path(icon_path: &str) -> Option<Retained<NSImage>> {
    let path = NSString::from_str(icon_path);
    NSImage::initWithContentsOfFile(NSImage::alloc(), &path)
}

fn load_icon_from_default_bytes() -> Option<Retained<NSImage>> {
    unsafe {
        let data = NSData::dataWithBytes_length(
            DEFAULT_NEOVIDE_ICON_BYTES.as_ptr() as *mut c_void,
            DEFAULT_NEOVIDE_ICON_BYTES.len(),
        );
        NSImage::initWithData(NSImage::alloc(), data.as_ref())
    }
}

pub fn load_neovide_icon(custom_icon_path: Option<&String>) -> Option<Retained<NSImage>> {
    custom_icon_path
        .and_then(|path| {
            let expanded = expand_tilde(path);
            load_icon_from_custom_path(&expanded)
        })
        .or_else(load_icon_from_default_bytes)
}
