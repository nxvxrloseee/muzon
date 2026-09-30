//! Spectrum for the visualizer: a `spectrum` element at the end of each
//! deck's filter chain, its messages caught on the streaming thread and handed
//! over at the moment their audio is actually heard.
//!
//! The element sees audio before the sink's buffer does, by however much the
//! sink holds - a few hundred milliseconds - so bars shown on arrival would
//! lead the music visibly. Each frame is held back until the pipeline's
//! running time reaches it.

use gst::prelude::*;
use gstreamer as gst;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bars the frontend draws.
pub const BARS: usize = 48;

/// Linear FFT bands the element computes; folded into `BARS` log-spaced
/// bars. Fine enough that even the narrow bass bars cover a band or two.
const FFT_BANDS: u32 = 1024;

/// About 30 frames a second.
const INTERVAL: Duration = Duration::from_millis(33);

/// Quieter than this is the bottom of a bar.
const FLOOR_DB: f32 = -80.0;

const MIN_HZ: f32 = 30.0;
const MAX_HZ: f32 = 16_000.0;

/// Longest a frame is held for; anything later is a clock mismatch, and
/// showing it late is better than queueing without bound.
const MAX_DELAY: Duration = Duration::from_secs(2);

/// Where frames go - the IPC channel to the frontend, in the app.
pub type SpectrumSink = Arc<dyn Fn(Vec<f32>) + Send + Sync>;

/// Folds linear-band magnitudes (dB, band `i` centred on
/// `(i + 0.5) * nyquist / n`) into `bars` log-spaced bars scaled to 0..1.
///
/// Each bar takes the loudest band inside it. The lowest bars are narrower
/// than one band, so a bar with no band centre inside takes the band under
/// its own centre instead of coming out empty.
pub fn fold(magnitudes_db: &[f32], rate: u32, bars: usize) -> Vec<f32> {
    let n = magnitudes_db.len();
    if n == 0 || rate == 0 {
        return vec![0.0; bars];
    }
    let band_hz = rate as f32 / 2.0 / n as f32;
    let ratio = (MAX_HZ / MIN_HZ).powf(1.0 / bars as f32);
    (0..bars)
        .map(|bar| {
            let low = MIN_HZ * ratio.powi(bar as i32);
            let high = low * ratio;
            let first = ((low / band_hz) - 0.5).ceil().max(0.0) as usize;
            let last = (((high / band_hz) - 0.5).floor() as usize).min(n - 1);
            let db = if first <= last {
                magnitudes_db[first..=last]
                    .iter()
                    .copied()
                    .fold(f32::NEG_INFINITY, f32::max)
            } else {
                let centre = ((low * high).sqrt() / band_hz) as usize;
                magnitudes_db[centre.min(n - 1)]
            };
            ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
        })
        .collect()
}

/// Shared by both decks.
pub struct Visualizer {
    sink: Mutex<Option<SpectrumSink>>,
    /// Whether deck A is the one being heard; only its frames are sent, or a
    /// crossfade would interleave two spectra into flicker.
    deck_a_audible: AtomicBool,
    delayed: Mutex<Sender<(Instant, Vec<f32>)>>,
}

impl Visualizer {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = channel::<(Instant, Vec<f32>)>();
        let visualizer = Arc::new(Self {
            sink: Mutex::new(None),
            deck_a_audible: AtomicBool::new(true),
            delayed: Mutex::new(tx),
        });
        let weak = Arc::downgrade(&visualizer);
        std::thread::spawn(move || {
            // Frames arrive in order and all carry about the same delay, so
            // waiting for each in turn keeps them in order and on time
            while let Ok((due, bars)) = rx.recv() {
                let now = Instant::now();
                if due > now {
                    std::thread::sleep(due - now);
                }
                let Some(visualizer) = weak.upgrade() else {
                    return;
                };
                let sink = visualizer.sink.lock().unwrap().clone();
                if let Some(sink) = sink {
                    sink(bars);
                }
            }
        });
        visualizer
    }

    pub fn set_sink(&self, sink: Option<SpectrumSink>) {
        *self.sink.lock().unwrap() = sink;
    }

    fn enabled(&self) -> bool {
        self.sink.lock().unwrap().is_some()
    }

    pub fn set_deck_a_audible(&self, a: bool) {
        self.deck_a_audible.store(a, Ordering::Relaxed);
    }

    /// Builds the element for one deck and starts listening to that deck's
    /// bus for its messages.
    pub fn tap(
        self: &Arc<Self>,
        playbin: &gst::Element,
        is_deck_a: bool,
    ) -> anyhow::Result<gst::Element> {
        let element = gst::ElementFactory::make("spectrum")
            .property("bands", FFT_BANDS)
            .property("interval", INTERVAL.as_nanos() as u64)
            .property("threshold", FLOOR_DB as i32)
            .property("post-messages", false)
            .property("message-magnitude", true)
            .build()?;

        let bus = playbin
            .bus()
            .ok_or_else(|| anyhow::anyhow!("playbin has no bus"))?;
        let visualizer = Arc::downgrade(self);
        let analysed = element.downgrade();
        let pipeline = playbin.downgrade();
        // A sync handler runs on the streaming thread that posted the message:
        // waiting for the polling loop to drain the bus would add up to a
        // whole tick of lag and bunch frames together.
        bus.set_sync_handler(move |_, msg| {
            let gst::MessageView::Element(element_msg) = msg.view() else {
                return gst::BusSyncReply::Pass;
            };
            let Some(s) = element_msg.structure().filter(|s| s.name() == "spectrum") else {
                return gst::BusSyncReply::Pass;
            };
            if let (Some(visualizer), Some(element), Some(pipeline)) =
                (visualizer.upgrade(), analysed.upgrade(), pipeline.upgrade())
            {
                let audible = visualizer.deck_a_audible.load(Ordering::Relaxed) == is_deck_a;
                if audible && visualizer.enabled() {
                    if let Some((due, bars)) = frame(s, &element, &pipeline) {
                        let _ = visualizer.delayed.lock().unwrap().send((due, bars));
                    }
                }
            }
            // Never queued for the polling loop's drain: at 30 a second they
            // would only be popped and thrown away there
            gst::BusSyncReply::Drop
        });
        Ok(element)
    }

    /// Turns the element's messages on or off; off costs nothing downstream.
    pub fn enable_on(&self, element: &gst::Element, on: bool) {
        element.set_property("post-messages", on);
    }
}

/// The bars in a spectrum message, and when the audio they describe will be
/// heard.
fn frame(
    s: &gst::StructureRef,
    element: &gst::Element,
    pipeline: &gst::Element,
) -> Option<(Instant, Vec<f32>)> {
    let magnitudes: Vec<f32> = s
        .get::<gst::List>("magnitude")
        .ok()?
        .iter()
        .filter_map(|v| v.get::<f32>().ok())
        .collect();
    let rate = element
        .static_pad("sink")?
        .current_caps()
        .and_then(|caps| gst_audio_rate(&caps))?;
    let bars = fold(&magnitudes, rate, BARS);

    // The middle of the analysed stretch, in running time, against where the
    // pipeline's clock is now
    let start = s.get::<u64>("running-time").ok()?;
    let duration = s.get::<u64>("duration").unwrap_or(0);
    let heard_at = start + duration / 2;
    let now = pipeline
        .clock()
        .map(|clock| clock.time())
        .zip(pipeline.base_time())
        .map(|(time, base)| time.nseconds().saturating_sub(base.nseconds()))?;
    let delay = Duration::from_nanos(heard_at.saturating_sub(now)).min(MAX_DELAY);
    Some((Instant::now() + delay, bars))
}

fn gst_audio_rate(caps: &gst::CapsRef) -> Option<u32> {
    caps.structure(0)?.get::<i32>("rate").ok().map(|r| r as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spectrum with one loud band at `hz` over a floor of silence.
    fn tone_at(hz: f32, rate: u32, n: usize) -> Vec<f32> {
        let band_hz = rate as f32 / 2.0 / n as f32;
        let loud = (hz / band_hz) as usize;
        (0..n)
            .map(|i| if i == loud { 0.0 } else { FLOOR_DB })
            .collect()
    }

    fn loudest_bar(bars: &[f32]) -> usize {
        bars.iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0
    }

    #[test]
    fn silence_is_all_zero_and_full_scale_is_one() {
        assert!(fold(&[FLOOR_DB; 1024], 44_100, BARS)
            .iter()
            .all(|b| *b == 0.0));
        assert!(fold(&[0.0; 1024], 44_100, BARS).iter().all(|b| *b == 1.0));
    }

    #[test]
    fn higher_tones_land_in_higher_bars() {
        let low = loudest_bar(&fold(&tone_at(100.0, 44_100, 1024), 44_100, BARS));
        let mid = loudest_bar(&fold(&tone_at(1_000.0, 44_100, 1024), 44_100, BARS));
        let high = loudest_bar(&fold(&tone_at(10_000.0, 44_100, 1024), 44_100, BARS));
        assert!(low < mid && mid < high, "{low} {mid} {high}");
    }

    #[test]
    fn the_scale_is_logarithmic() {
        // Each decade of frequency takes about the same share of the bars
        let at = |hz| loudest_bar(&fold(&tone_at(hz, 44_100, 1024), 44_100, BARS)) as i32;
        let first_decade = at(1_000.0) - at(100.0);
        let second_decade = at(10_000.0) - at(1_000.0);
        assert!(
            (first_decade - second_decade).abs() <= 2,
            "{first_decade} vs {second_decade}"
        );
    }

    #[test]
    fn no_bar_is_left_empty_below_a_band_width() {
        // A flat -40 dB spectrum: every bar, bass included, has to show it
        let bars = fold(&[-40.0; 1024], 44_100, BARS);
        assert!(bars.iter().all(|b| (*b - 0.5).abs() < 1e-6));
    }
}
