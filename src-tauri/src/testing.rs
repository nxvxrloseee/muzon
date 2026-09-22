//! Fixtures shared between test modules. Test-only; not compiled into the app.

use std::path::{Path, PathBuf};

/// Writes a real, decodable audio file of a known length: 16-bit mono PCM in a
/// WAV container, which both GStreamer and lofty read without extra plugins.
pub fn write_tone(dir: &Path, name: &str, seconds: f64) -> PathBuf {
    const SAMPLE_RATE: u32 = 8000;
    let samples = (SAMPLE_RATE as f64 * seconds) as u32;
    let data_len = samples * 2;

    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data_len).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes()); // PCM header size
    wav.extend(1u16.to_le_bytes()); // uncompressed
    wav.extend(1u16.to_le_bytes()); // mono
    wav.extend(SAMPLE_RATE.to_le_bytes());
    wav.extend((SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    wav.extend(2u16.to_le_bytes()); // block align
    wav.extend(16u16.to_le_bytes()); // bits per sample
    wav.extend(b"data");
    wav.extend(data_len.to_le_bytes());
    for i in 0..samples {
        let t = i as f64 / SAMPLE_RATE as f64;
        let sample = (t * 440.0 * std::f64::consts::TAU).sin() * 8000.0;
        wav.extend((sample as i16).to_le_bytes());
    }

    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, wav).unwrap();
    path
}

/// A directory of its own under the system temp dir, so parallel tests can't
/// collide on filenames.
pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muzon-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
