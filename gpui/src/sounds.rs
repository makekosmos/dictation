#[cfg(windows)]
fn play(wav: &'static [u8], enabled: bool, volume: f32) {
    use std::sync::Mutex;
    use windows_sys::Win32::Media::Audio::{PlaySoundA, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    static WAV_BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    if !enabled || volume <= 0.0 {
        return;
    }

    let mut buffer = WAV_BUFFER.lock().expect("sound buffer poisoned");
    unsafe {
        PlaySoundA(std::ptr::null(), std::ptr::null_mut(), 0);
    }
    buffer.clear();
    buffer.extend_from_slice(wav);
    // Bundled assets are PCM 16-bit WAVs with the standard 44-byte header.
    if let Some(samples) = buffer.get_mut(44..) {
        for sample in samples.chunks_exact_mut(2) {
            let value = i16::from_le_bytes([sample[0], sample[1]]);
            let scaled = (value as f32 * volume.clamp(0.0, 1.0)).round() as i16;
            sample.copy_from_slice(&scaled.to_le_bytes());
        }
    }
    unsafe {
        PlaySoundA(
            buffer.as_ptr(),
            std::ptr::null_mut(),
            SND_ASYNC | SND_MEMORY | SND_NODEFAULT,
        );
    }
}

#[cfg(not(windows))]
fn play(_wav: &'static [u8], _enabled: bool, _volume: f32) {}

pub(crate) fn start(enabled: bool, volume: f32) {
    play(
        include_bytes!("../assets/sounds/start.wav"),
        enabled,
        volume,
    );
}

pub(crate) fn stop(enabled: bool, volume: f32) {
    play(include_bytes!("../assets/sounds/stop.wav"), enabled, volume);
}
