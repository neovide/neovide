use windows::Win32::System::Console::ATTACH_PARENT_PROCESS;
use windows::Win32::System::Console::AttachConsole;

pub fn attach_to_console() {
    // Attach to parent console tip found here: https://github.com/rust-lang/rust/issues/67159#issuecomment-987882771
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS).ok();
    }
}
