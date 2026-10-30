#[cfg(windows)]
fn play(wav: &'static [u8]) {
    use windows_sys::Win32::Media::Audio::{PlaySoundA, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    unsafe {
        PlaySoundA(
            wav.as_ptr(),
            std::ptr::null_mut(),
            SND_ASYNC | SND_MEMORY | SND_NODEFAULT,
        );
    }
}

#[cfg(not(windows))]
fn play(_wav: &'static [u8]) {}

pub(crate) fn start() {
    play(include_bytes!("../assets/sounds/start.wav"));
}

pub(crate) fn stop() {
    play(include_bytes!("../assets/sounds/stop.wav"));
}
