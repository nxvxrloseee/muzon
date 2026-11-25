//! Opt-in engine probe. Only an already-loaded ALSA Loopback card is accepted.
//! Does not load modules, change profiles, or open a physical output device.
use muzon_core::data::audio::pipeline::AudioPlayer;
use muzon_core::domain::playback_settings::OutputMode;
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let file = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: aloop_probe FILE [hw:CARD=Loopback,DEV=0]"))?;
    let device = args
        .next()
        .unwrap_or_else(|| "hw:CARD=Loopback,DEV=0".into());
    anyhow::ensure!(
        device == "hw:CARD=Loopback,DEV=0" || device == "hw:CARD=Loopback,DEV=1",
        "Only the Loopback virtual card is allowed"
    );
    let card = std::fs::read_to_string("/proc/asound/Loopback/id")?;
    anyhow::ensure!(
        card.trim() == "Loopback",
        "No existing Loopback card; this probe never loads snd-aloop"
    );
    let player = AudioPlayer::new()?;
    // Non-neutral stored effects must not change a direct PCM payload.
    player.set_equalizer_bands([6.0; 10]);
    player.set_volume(0.1)?;
    player.set_crossfade_seconds(2.0);
    player.configure_output(OutputMode::BitPerfect, Some(device.clone()))?;
    player.load_and_play(&std::fs::canonicalize(file)?.to_string_lossy())?;
    let started = Instant::now();
    let mut previous = None;
    loop {
        let tick = player.tick();
        if let Some(error) = tick.error {
            anyhow::bail!("{}", error.message);
        }
        let status = player.output_status();
        if status.negotiated_format != previous {
            println!(
                "Negotiated sink (not hardware verification): {:?}",
                status.negotiated_format
            );
            let pcm = if device.ends_with("DEV=0") {
                "pcm0p"
            } else {
                "pcm1p"
            };
            // snd-aloop defaults to subdevice 0; if already occupied, inspect
            // the other sub*/hw_params manually and reject ambiguous evidence.
            println!(
                "ALSA hw_params: {}",
                std::fs::read_to_string(format!("/proc/asound/Loopback/{pcm}/sub0/hw_params"))?
            );
            previous = status.negotiated_format;
        }
        if tick.ended {
            break;
        }
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(120),
            "Probe timed out"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    player.stop()?;
    Ok(())
}
