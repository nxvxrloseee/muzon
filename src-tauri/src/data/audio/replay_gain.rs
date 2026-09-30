//! ReplayGain for one deck: a `volume` element in the filter chain, set from
//! the stream's own tags as they flow past.
//!
//! Not `rgvolume`: it has no way to be switched off short of rebuilding the
//! chain, and it always applies whatever the tags say. Reading the same tag
//! events ourselves keeps its one real virtue - the gain changes on the
//! streaming thread, exactly where one track's tags give way to the next's,
//! so a gapless hand-off is levelled from its first sample - and lets the
//! mode change at any time.

use crate::domain::playback_settings::{gain_factor, ReplayGainSettings, StreamGains};
use gst::prelude::*;
use gstreamer as gst;
use std::sync::{Arc, Mutex};

struct State {
    settings: ReplayGainSettings,
    gains: StreamGains,
}

pub struct ReplayGain {
    volume: gst::Element,
    state: Arc<Mutex<State>>,
}

impl ReplayGain {
    pub fn build() -> anyhow::Result<Self> {
        Ok(Self {
            volume: gst::ElementFactory::make("volume").build()?,
            state: Arc::new(Mutex::new(State {
                settings: ReplayGainSettings::default(),
                gains: StreamGains::default(),
            })),
        })
    }

    pub fn element(&self) -> &gst::Element {
        &self.volume
    }

    /// Follows the events arriving on `pad`, which must be upstream of (or
    /// on) the volume element.
    pub fn follow(&self, pad: &gst::Pad) {
        let state = self.state.clone();
        let volume = self.volume.clone();
        pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
            let Some(gst::PadProbeData::Event(event)) = &info.data else {
                return gst::PadProbeReturn::Ok;
            };
            let mut state = state.lock().unwrap();
            match event.view() {
                // A new stream - the next track of a gapless run included -
                // starts from nothing; its tags follow right behind.
                gst::EventView::StreamStart(_) => state.gains = StreamGains::default(),
                // Tags can arrive split across several events, so each only
                // overrides the fields it actually carries
                gst::EventView::Tag(tag) => {
                    let list = tag.tag();
                    let g = &mut state.gains;
                    let read = |v: Option<f64>, old: Option<f64>| v.or(old);
                    g.track_gain = read(
                        list.get::<gst::tags::TrackGain>().map(|v| v.get()),
                        g.track_gain,
                    );
                    g.track_peak = read(
                        list.get::<gst::tags::TrackPeak>().map(|v| v.get()),
                        g.track_peak,
                    );
                    g.album_gain = read(
                        list.get::<gst::tags::AlbumGain>().map(|v| v.get()),
                        g.album_gain,
                    );
                    g.album_peak = read(
                        list.get::<gst::tags::AlbumPeak>().map(|v| v.get()),
                        g.album_peak,
                    );
                }
                _ => return gst::PadProbeReturn::Ok,
            }
            apply(&volume, &state);
            gst::PadProbeReturn::Ok
        });
    }

    pub fn set_settings(&self, settings: ReplayGainSettings) {
        let mut state = self.state.lock().unwrap();
        state.settings = settings;
        apply(&self.volume, &state);
    }

    #[cfg(test)]
    pub fn current_factor(&self) -> f64 {
        self.volume.property::<f64>("volume")
    }
}

fn apply(volume: &gst::Element, state: &State) {
    // `volume` tops out at 10x (+20 dB), far beyond any sane tag
    let factor = gain_factor(state.settings, &state.gains).clamp(0.0, 10.0);
    volume.set_property("volume", factor);
}
