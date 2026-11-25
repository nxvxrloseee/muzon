use super::output::{sink_format, OutputStatus};
use super::replay_gain::ReplayGain;
use super::spectrum::{SpectrumSink, Visualizer};
use crate::domain::playback_settings::{validate_output_device, OutputMode, ReplayGainSettings};
use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;
use specta::Type;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const EQ_BAND_COUNT: usize = 10;

/// Bounds of the time-stretch, shared so the MPRIS `MinimumRate`/`MaximumRate`
/// a desktop shell reads can't drift away from what playback actually accepts.
pub const MIN_TEMPO: f64 = 0.25;
pub const MAX_TEMPO: f64 = 4.0;

/// Floor on how early a crossfade may start, so a very short fade can't fall
/// between two polls and be missed entirely - starting at most this much early
/// is inaudible, silently skipping the fade is not. Gapless transitions don't
/// go through here at all any more: `about-to-finish` hands the next URI to the
/// same playbin while the current one is still feeding the sink, so the join is
/// made inside the pipeline rather than by us watching a clock.
const MIN_CROSSFADE_LOOKAHEAD_SECS: f64 = 0.25;

/// playbin's `volume` property is linear amplitude, but hearing is closer to
/// logarithmic, so a slider mapped straight onto it spends four fifths of its
/// travel doing almost nothing audible. GStreamer's own "cubic" stream-volume
/// scale is the accepted fix for exactly this, and it is just `linear = cubic³`.
fn cubic_to_linear(cubic: f64) -> f64 {
    let c = cubic.clamp(0.0, 1.0);
    c * c * c
}

/// A GStreamer error message that reached one of the decks, on its way to a
/// toast in the UI.
#[derive(Debug, Clone, Serialize, Type)]
pub struct PlaybackError {
    /// The file that failed, when the deck that raised it had one loaded.
    pub path: Option<String>,
    pub message: String,
    /// True when the failure hit the deck that was actually playing - audio has
    /// stopped and the queue needs to move on. False when it was the pre-rolled
    /// standby deck, where only the pre-load was lost and playback continues.
    pub fatal: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct PlaybackTick {
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub path: Option<String>,
    /// One-shot pulse: true on exactly the tick that observed a real End-Of-Stream
    /// with nothing pre-rolled to follow it, so the frontend can decide whether to
    /// advance the queue.
    pub ended: bool,
    /// One-shot pulse: set to the new path exactly on the tick that performed a
    /// gapless or crossfade transition, so the frontend can sync its queue cursor
    /// without re-invoking play (which would interrupt the seamless transition).
    pub auto_advanced_to: Option<String>,
    /// One-shot pulse: an error drained off a deck's bus this tick. Everything
    /// that wasn't EOS used to be discarded here, so a missing codec or an
    /// unreadable file left playback silently stalled with nothing said and the
    /// queue never advancing.
    pub error: Option<PlaybackError>,
}

pub struct PlaybackStatus {
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub path: Option<String>,
}

/// One decode-and-output unit: a playbin plus its own equalizer/tempo filter
/// chain. Two of these let us pre-roll the next track on the standby deck while
/// the active one keeps playing normally, then switch over with either a hard
/// cut (gapless) or a gradual volume ramp (crossfade) - the same mechanism
/// either way, just a different fade duration.
struct Deck {
    filter: gst::Bin,
    normal_flags: gst::glib::Value,
    normal_sink: Option<gst::Element>,
    direct: bool,
    direct_sink: Option<gst::Element>,
    playbin: gst::Element,
    eq: gst::Element,
    pitch: gst::Element,
    replay_gain: ReplayGain,
    spectrum: gst::Element,
    path: Option<String>,
    /// URI handed to this deck's `about-to-finish` handler, which runs on a
    /// GStreamer streaming thread. Deliberately its own small lock rather than
    /// a reach back into `Inner`: blocking a streaming thread on the mutex the
    /// polling loop holds would stall audio output.
    next_uri: Arc<Mutex<Option<String>>>,
}

impl Deck {
    fn build(visualizer: &Arc<Visualizer>, is_deck_a: bool) -> anyhow::Result<Self> {
        let playbin = gst::ElementFactory::make("playbin")
            .build()
            .or_else(|_| gst::ElementFactory::make("playbin3").build())?;

        let convert_in = gst::ElementFactory::make("audioconvert").build()?;
        // Levelling comes first, so the EQ works on audio already at its
        // normalised loudness rather than boosting into a gain cut after it
        let replay_gain = ReplayGain::build()?;
        let eq = gst::ElementFactory::make("equalizer-10bands").build()?;
        let pitch = gst::ElementFactory::make("pitch").build()?;
        // Last, so the bars show what is heard: levelled, equalised, stretched
        let spectrum = visualizer.tap(&playbin, is_deck_a)?;
        let convert_out = gst::ElementFactory::make("audioconvert").build()?;

        let chain = [
            &convert_in,
            replay_gain.element(),
            &eq,
            &pitch,
            &spectrum,
            &convert_out,
        ];
        let bin = gst::Bin::new();
        bin.add_many(chain)?;
        gst::Element::link_many(chain)?;

        let sink_pad = convert_in
            .static_pad("sink")
            .ok_or_else(|| anyhow::anyhow!("audioconvert has no sink pad"))?;
        replay_gain.follow(&sink_pad);
        let ghost_sink = gst::GhostPad::with_target(&sink_pad)?;
        ghost_sink.set_active(true)?;
        bin.add_pad(&ghost_sink)?;

        let src_pad = convert_out
            .static_pad("src")
            .ok_or_else(|| anyhow::anyhow!("audioconvert has no src pad"))?;
        let ghost_src = gst::GhostPad::with_target(&src_pad)?;
        ghost_src.set_active(true)?;
        bin.add_pad(&ghost_src)?;

        playbin.set_property("audio-filter", &bin);

        // True gapless. playbin raises this while the current track is still
        // feeding the sink; setting `uri` from the handler makes it decode
        // straight on into the next one, with the samples concatenated inside
        // the pipeline. Nothing is cut and nothing restarts - which the old
        // two-deck hard cut could not manage, since it had to switch decks
        // slightly *before* the end and so clipped the tail off every track.
        let next_uri: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let armed = next_uri.clone();
        playbin.connect("about-to-finish", false, move |args| {
            let playbin = args[0].get::<gst::Element>().ok()?;
            // Taken rather than read: one arming is good for one transition,
            // and the frontend re-arms as soon as it sees the advance land.
            let uri = armed.lock().unwrap().take()?;
            eprintln!("[muzon audio] gapless hand-off -> {uri}");
            playbin.set_property("uri", &uri);
            None
        });

        let normal_flags = playbin.property_value("flags");
        let normal_sink = playbin.property::<Option<gst::Element>>("audio-sink");
        Ok(Self {
            filter: bin,
            normal_flags,
            normal_sink,
            direct: false,
            direct_sink: None,
            playbin,
            eq,
            pitch,
            replay_gain,
            spectrum,
            path: None,
            next_uri,
        })
    }

    /// Volume on the perceptual scale, i.e. the number the slider shows.
    fn apply_volume(&self, cubic: f64) {
        self.playbin.set_property(
            "volume",
            if self.direct {
                1.0
            } else {
                cubic_to_linear(cubic)
            },
        );
    }

    /// Volume as raw linear amplitude - what a crossfade has to work in, since
    /// equal-power mixing is a statement about amplitudes and not about
    /// whatever scale a slider happens to use.
    fn apply_linear_volume(&self, linear: f64) {
        self.playbin.set_property("volume", linear.clamp(0.0, 1.0));
    }

    fn disarm(&self) {
        *self.next_uri.lock().unwrap() = None;
    }

    fn apply_eq(&self, gains: &[f64; EQ_BAND_COUNT]) {
        for (i, gain) in gains.iter().enumerate() {
            self.eq
                .set_property(&format!("band{i}"), gain.clamp(-24.0, 12.0));
        }
    }

    fn apply_tempo(&self, tempo: f64) {
        self.pitch
            .set_property("tempo", tempo.clamp(MIN_TEMPO, MAX_TEMPO) as f32);
    }

    /// Drops everything queued on this deck's bus. Called whenever a deck is
    /// re-pointed at a different file, so a message left over from the previous
    /// one can't be drained a tick later and blamed on its replacement.
    fn flush_bus(&self) {
        if let Some(bus) = self.playbin.bus() {
            bus.set_flushing(true);
            bus.set_flushing(false);
        }
    }
}

struct CrossfadeState {
    started_at: Instant,
    duration_secs: f64,
}

#[derive(Clone)]
struct OutputRecovery {
    path: Option<String>,
    position: Option<gst::ClockTime>,
    playing: bool,
    loaded: bool,
    next_path: Option<String>,
}

struct Inner {
    requested_mode: OutputMode,
    requested_device: Option<String>,
    output_recovery: Option<OutputRecovery>,
    output_mode: OutputMode,
    output_device: Option<String>,
    output_error: Option<String>,
    output_blocked: bool,
    replay_gain_settings: ReplayGainSettings,
    spectrum_sink: Option<SpectrumSink>,
    deck_a: Deck,
    deck_b: Deck,
    active_is_a: bool,
    current_path: Option<String>,
    /// What the frontend's queue would play next; the standby deck is pre-rolled
    /// (Paused, uri set) against this as soon as it's known, not just when the
    /// transition window arrives.
    next_path: Option<String>,
    volume: f64,
    crossfade_secs: f64,
    eq_gains: [f64; EQ_BAND_COUNT],
    crossfade: Option<CrossfadeState>,
    visualizer: Arc<Visualizer>,
    /// Last position reported by the active deck. A gapless hand-off is only
    /// visible as this jumping backwards - see `tick`.
    last_position_secs: f64,
}

impl Inner {
    fn active(&self) -> &Deck {
        if self.active_is_a {
            &self.deck_a
        } else {
            &self.deck_b
        }
    }

    fn standby_mut(&mut self) -> &mut Deck {
        if self.active_is_a {
            &mut self.deck_b
        } else {
            &mut self.deck_a
        }
    }

    fn active_mut(&mut self) -> &mut Deck {
        if self.active_is_a {
            &mut self.deck_a
        } else {
            &mut self.deck_b
        }
    }

    /// Points the decks at `next_path` using whichever mechanism the current
    /// crossfade setting calls for.
    ///
    /// Gapless hands the URI to the active deck's `about-to-finish` handler,
    /// which is what makes the join seamless. A crossfade instead pre-rolls the
    /// standby deck, because a fade needs two streams audible at once and one
    /// playbin can only be one place at a time. The mode that isn't in force is
    /// always disarmed first, or flipping the slider mid-track would leave both
    /// a pre-rolled deck and an armed handler racing for the same transition.
    fn arm_next(&mut self) {
        if self.output_blocked || self.output_mode == OutputMode::BitPerfect {
            self.deck_a.disarm();
            self.deck_b.disarm();
            return;
        }
        let crossfading = self.crossfade_secs > 0.0;
        let next = self.next_path.clone();

        self.active().disarm();
        if !crossfading {
            let standby = self.standby_mut();
            standby.playbin.set_state(gst::State::Null).ok();
            standby.flush_bus();
            standby.path = None;
        }

        let Some(next) = next else {
            return;
        };
        let Ok(uri) = path_to_uri(&next) else {
            return;
        };

        if crossfading {
            let standby = self.standby_mut();
            standby.playbin.set_state(gst::State::Null).ok();
            standby.flush_bus();
            standby.playbin.set_property("uri", &uri);
            standby.playbin.set_property("volume", 0.0_f64);
            let _ = standby.playbin.set_state(gst::State::Paused);
            standby.path = Some(next.clone());
            eprintln!("[muzon audio] pre-rolling next track for crossfade: {next}");
        } else {
            *self.active().next_uri.lock().unwrap() = Some(uri);
            eprintln!("[muzon audio] armed gapless follow-on: {next}");
        }
    }

    /// Forgets whatever was queued up next, on both mechanisms. Used wherever
    /// playback is being redirected outright, so a stale follow-on can't sneak
    /// in behind the new track.
    fn disarm_next(&mut self) {
        self.next_path = None;
        self.deck_a.disarm();
        self.deck_b.disarm();
        // A stale high-water mark would make the next track's first tick look
        // like a hand-off, since position "went backwards".
        self.last_position_secs = 0.0;
    }
}

/// The file a playbin is playing *now*, which after a gapless hand-off is no
/// longer the one it was originally pointed at.
fn current_playbin_path(playbin: &gst::Element) -> Option<String> {
    let uri = playbin.property::<Option<String>>("current-uri")?;
    let (path, _) = gst::glib::filename_from_uri(&uri).ok()?;
    Some(path.to_string_lossy().to_string())
}

pub struct AudioPlayer {
    inner: Mutex<Inner>,
}

fn path_to_uri(path: &str) -> anyhow::Result<String> {
    Ok(gst::glib::filename_to_uri(path, None)?.to_string())
}

#[derive(Default)]
struct BusDrain {
    eos: bool,

    /// First error seen this drain; later ones are almost always cascades of the
    /// same failure, and only one of them is worth showing.
    error: Option<String>,
}

fn drain_bus(playbin: &gst::Element) -> BusDrain {
    let mut drained = BusDrain::default();
    if let Some(bus) = playbin.bus() {
        while let Some(msg) = bus.pop() {
            match msg.view() {
                gst::MessageView::Eos(_) => drained.eos = true,
                gst::MessageView::Error(err) => {
                    if drained.error.is_none() {
                        drained.error = Some(err.error().to_string());
                    }
                    eprintln!(
                        "[muzon audio] gstreamer error: {} ({:?})",
                        err.error(),
                        err.debug()
                    );
                }
                _ => {}
            }
        }
    }
    drained
}

impl AudioPlayer {
    pub fn new() -> anyhow::Result<Self> {
        gst::init()?;

        let visualizer = Visualizer::new();
        let inner = Inner {
            requested_mode: OutputMode::Normal,
            requested_device: None,
            output_recovery: None,
            output_mode: OutputMode::Normal,
            output_device: None,
            output_error: None,
            output_blocked: false,
            replay_gain_settings: ReplayGainSettings::default(),
            spectrum_sink: None,
            deck_a: Deck::build(&visualizer, true)?,
            deck_b: Deck::build(&visualizer, false)?,
            visualizer,
            active_is_a: true,
            current_path: None,
            next_path: None,
            volume: 1.0,
            crossfade_secs: 0.0,
            eq_gains: [0.0; EQ_BAND_COUNT],
            crossfade: None,
            last_position_secs: 0.0,
        };

        Ok(Self {
            inner: Mutex::new(inner),
        })
    }

    /// Reconfigure while stopped, then restore URI, position and playing/paused
    /// state. On failure audio remains stopped and the error remains observable.
    /// Call from the controller worker; preroll is bounded, never on the UI thread.
    pub fn configure_output(&self, mode: OutputMode, device: Option<String>) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        if guard.output_mode == mode
            && guard.output_device == device
            && guard.output_error.is_none()
        {
            return Ok(());
        }
        guard.requested_mode = mode;
        guard.requested_device = device.clone();
        if guard.output_recovery.is_none() {
            let p = &guard.active().playbin;
            let (_, state, pending) = p.state(gst::ClockTime::ZERO);
            guard.output_recovery = Some(OutputRecovery {
                path: guard.current_path.clone(),
                position: p.query_position::<gst::ClockTime>(),
                playing: state == gst::State::Playing || pending == gst::State::Playing,
                loaded: state >= gst::State::Paused || pending >= gst::State::Paused,
                next_path: guard.next_path.clone(),
            });
        }
        let recovery = guard.output_recovery.clone().expect("captured recovery");
        let result = (|| -> anyhow::Result<()> {
            if let Some(id) = &device {
                validate_output_device(id).map_err(anyhow::Error::msg)?;
            }
            let sinks = if mode == OutputMode::BitPerfect {
                let id = device
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Выберите прямое ALSA-устройство"))?;
                let build = || -> anyhow::Result<(gst::Element, gst::Element)> {
                    let sink = gst::ElementFactory::make("alsasink")
                        .property("device", id)
                        .property("provide-clock", true)
                        .build()?;
                    // No resampling or sample insertion/drop for clock slaving.
                    sink.set_property_from_str("slave-method", "none");
                    let filter = gst::ElementFactory::make("capsfilter")
                        .property("caps", gst::Caps::builder("audio/x-raw").build())
                        .build()?;
                    let bin = gst::Bin::new();
                    bin.add_many([&filter, &sink])?;
                    filter.link(&sink)?;
                    let pad = gst::GhostPad::with_target(
                        &filter
                            .static_pad("sink")
                            .ok_or_else(|| anyhow::anyhow!("capsfilter missing sink pad"))?,
                    )?;
                    pad.set_active(true)?;
                    bin.add_pad(&pad)?;
                    Ok((bin.upcast(), sink))
                };
                Some([build()?, build()?])
            } else {
                None
            };
            let OutputRecovery {
                path,
                position,
                playing,
                loaded,
                next_path: next,
            } = recovery.clone();
            guard.deck_a.disarm();
            guard.deck_b.disarm();
            guard.crossfade = None;
            for deck in [&guard.deck_a, &guard.deck_b] {
                deck.playbin.set_state(gst::State::Null)?;
                deck.playbin.state(gst::ClockTime::from_seconds(5)).0?;
                deck.flush_bus();
            }
            guard.output_mode = mode;
            guard.output_device = device.clone();
            let volume = guard.volume;
            let rg = guard.replay_gain_settings;
            let Inner { deck_a, deck_b, .. } = &mut *guard;
            for (i, deck) in [deck_a, deck_b].into_iter().enumerate() {
                deck.direct = mode == OutputMode::BitPerfect;
                if let Some(sinks) = &sinks {
                    deck.playbin
                        .set_property("audio-filter", None::<gst::Element>);
                    // GstPlayFlags are plugin-defined, hence use registered nicks.
                    // native-audio suppresses playsink conversion/resampling;
                    // omitting soft-volume also removes the software volume path.
                    deck.playbin
                        .set_property_from_str("flags", "audio+native-audio");
                    deck.playbin.set_property("audio-sink", &sinks[i].0);
                    deck.direct_sink = Some(sinks[i].1.clone());
                    deck.replay_gain.set_settings(ReplayGainSettings::default());
                } else {
                    deck.direct_sink = None;
                    deck.playbin
                        .set_property_from_value("flags", &deck.normal_flags);
                    deck.playbin.set_property("audio-filter", &deck.filter);
                    deck.playbin.set_property("audio-sink", &deck.normal_sink);
                    deck.replay_gain.set_settings(rg);
                }
                deck.apply_volume(volume);
            }
            let on = mode == OutputMode::Normal && guard.spectrum_sink.is_some();
            guard.visualizer.set_sink(if on {
                guard.spectrum_sink.clone()
            } else {
                None
            });
            guard.visualizer.enable_on(&guard.deck_a.spectrum, on);
            guard.visualizer.enable_on(&guard.deck_b.spectrum, on);
            if loaded {
                if let Some(path) = path {
                    let p = &guard.active().playbin;
                    p.set_property("uri", path_to_uri(&path)?);
                    p.set_state(gst::State::Paused)?;
                    let (result, current, _) = p.state(gst::ClockTime::from_seconds(5));
                    result?;
                    anyhow::ensure!(current == gst::State::Paused, "Output preroll timed out");
                    if let Some(pos) = position.filter(|p| *p > gst::ClockTime::ZERO) {
                        p.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, pos)?;
                        p.state(gst::ClockTime::from_seconds(5)).0?;
                    }
                    if playing {
                        p.set_state(gst::State::Playing)?;
                    }
                }
            }
            guard.next_path = next;
            guard.last_position_secs = 0.0;
            guard.output_blocked = false;
            guard.arm_next();
            Ok(())
        })();
        guard.output_blocked = result.is_err();
        guard.output_error = result
            .as_ref()
            .err()
            .map(|e| format!("Output configuration failed: {e}"));
        if result.is_ok() {
            guard.output_recovery = None;
        }
        if result.is_err() {
            guard.crossfade = None;
            guard.deck_a.disarm();
            guard.deck_b.disarm();
            for deck in [&guard.deck_a, &guard.deck_b] {
                let _ = deck.playbin.set_state(gst::State::Null);
            }
        }
        result
    }

    pub fn output_status(&self) -> OutputStatus {
        let guard = self.inner.lock().unwrap();
        let p = &guard.active().playbin;
        let sink = guard
            .active()
            .direct_sink
            .clone()
            .or_else(|| p.property::<Option<gst::Element>>("audio-sink"));
        let (_, state, _) = p.state(gst::ClockTime::ZERO);
        let format = if state >= gst::State::Paused && guard.output_error.is_none() {
            sink.as_ref()
                .map(sink_format)
                .transpose()
                .map(Option::flatten)
        } else {
            Ok(None)
        };
        OutputStatus {
            mode: guard.output_mode,
            device: guard.output_device.clone(),
            requested_mode: guard.requested_mode,
            requested_device: guard.requested_device.clone(),
            blocked: guard.output_blocked,
            negotiated_format: format.as_ref().ok().cloned().flatten(),
            bit_perfect_verified: false,
            error: guard.output_error.clone().or_else(|| format.err()),
        }
    }

    pub fn load_and_play(&self, path: &str) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        anyhow::ensure!(
            !guard.output_blocked,
            "Output configuration failed; reconfigure before playback"
        );
        guard.output_error = None;
        let standby = guard.standby_mut();
        standby.playbin.set_state(gst::State::Null).ok();
        standby.flush_bus();
        standby.path = None;
        guard.crossfade = None;
        guard.disarm_next();

        let volume = guard.volume;
        let active_is_a = guard.active_is_a;
        let uri = path_to_uri(path)?;
        let deck = if active_is_a {
            &mut guard.deck_a
        } else {
            &mut guard.deck_b
        };
        deck.playbin.set_state(gst::State::Null)?;
        deck.flush_bus();
        deck.playbin.set_property("uri", &uri);
        deck.apply_volume(volume);
        deck.path = Some(path.to_string());
        guard.current_path = Some(path.to_string());
        if let Err(error) = guard.active().playbin.set_state(gst::State::Playing) {
            guard.output_error = Some(format!("Playback start failed: {error}"));
            let _ = guard.active().playbin.set_state(gst::State::Null);
            return Err(error.into());
        }
        Ok(())
    }

    /// Loads `path` on the active deck and holds it Paused at `position_secs`,
    /// never entering PLAYING - what restoring a session wants: the app comes
    /// back looking exactly as it was closed rather than starting to play on
    /// its own, and hitting play resumes instead of restarting.
    ///
    /// Blocks until the deck has pre-rolled, because a seek issued before that
    /// simply fails - call it off the UI thread.
    pub fn load_paused_at(&self, path: &str, position_secs: f64) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        anyhow::ensure!(
            !guard.output_blocked,
            "Output configuration failed; reconfigure before playback"
        );
        let standby = guard.standby_mut();
        standby.playbin.set_state(gst::State::Null).ok();
        standby.flush_bus();
        standby.path = None;
        guard.crossfade = None;
        guard.disarm_next();

        let volume = guard.volume;
        let active_is_a = guard.active_is_a;
        let uri = path_to_uri(path)?;
        let deck = if active_is_a {
            &mut guard.deck_a
        } else {
            &mut guard.deck_b
        };
        deck.playbin.set_state(gst::State::Null)?;
        deck.flush_bus();
        deck.playbin.set_property("uri", &uri);
        deck.apply_volume(volume);
        deck.playbin.set_state(gst::State::Paused)?;
        // Pre-roll has to finish before the pipeline can answer a seek. Five
        // seconds is far more than a local file needs and still bounded if the
        // file turns out to be unreadable.
        let _ = deck.playbin.state(gst::ClockTime::from_seconds(5));
        deck.path = Some(path.to_string());

        if position_secs > 0.0 {
            let pos = gst::ClockTime::from_nseconds((position_secs * 1_000_000_000.0) as u64);
            // ACCURATE rather than the KEY_UNIT snap used for interactive
            // scrubbing: resuming should land where the session was left, and
            // this happens once at startup where the extra cost doesn't matter.
            deck.playbin
                .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, pos)
                .ok();
            // A flushing seek starts a second asynchronous pre-roll. Do not
            // publish the restored session until it settles: querying position
            // in that gap can report zero and erase the restored playhead.
            let _ = deck.playbin.state(gst::ClockTime::from_seconds(5));
        }
        guard.current_path = Some(path.to_string());
        Ok(())
    }

    pub fn play(&self) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap();
        anyhow::ensure!(
            !guard.output_blocked,
            "Output configuration failed; reconfigure before playback"
        );
        guard.active().playbin.set_state(gst::State::Playing)?;
        Ok(())
    }

    pub fn pause(&self) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap();
        anyhow::ensure!(
            !guard.output_blocked,
            "Output configuration failed; reconfigure before pause"
        );
        guard.active().playbin.set_state(gst::State::Paused)?;
        Ok(())
    }

    pub fn seek(&self, position_secs: f64) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap();
        let pos = gst::ClockTime::from_nseconds((position_secs * 1_000_000_000.0) as u64);
        guard
            .active()
            .playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, pos)?;
        Ok(())
    }

    pub fn set_volume(&self, volume: f64) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.volume = volume.clamp(0.0, 1.0);
        // Outside of an active crossfade both decks should reflect the same master
        // volume; during one, `tick()` owns both decks' volume until it finishes.
        if guard.crossfade.is_none() {
            guard.active().apply_volume(guard.volume);
        }
        Ok(())
    }

    pub fn volume(&self) -> f64 {
        self.inner.lock().unwrap().volume
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.output_recovery = None;
        guard.deck_a.playbin.set_state(gst::State::Null).ok();
        guard.deck_b.playbin.set_state(gst::State::Null).ok();
        guard.deck_a.flush_bus();
        guard.deck_b.flush_bus();
        guard.deck_a.path = None;
        guard.deck_b.path = None;
        guard.current_path = None;
        guard.crossfade = None;
        guard.disarm_next();
        Ok(())
    }

    /// Tells the player what the frontend's queue would play next, so it can be
    /// made ready well before it's needed. Pass `None` when there is nothing
    /// next (end of queue, repeat off). See `Inner::arm_next` for what "ready"
    /// means in each mode.
    pub fn set_next_track(&self, path: Option<String>) {
        let mut guard = self.inner.lock().unwrap();
        if guard.next_path == path {
            return;
        }
        guard.next_path = path;

        // Mid-crossfade the standby deck is already fading in and about to
        // become the active one - re-pointing it now would cut the fade off.
        if guard.crossfade.is_some() {
            return;
        }
        guard.arm_next();
    }

    pub fn set_crossfade_seconds(&self, secs: f64) {
        let mut guard = self.inner.lock().unwrap();
        let secs = secs.max(0.0);
        let mode_changed = (guard.crossfade_secs > 0.0) != (secs > 0.0);
        guard.crossfade_secs = secs;
        // Crossfading and gapless arm completely different machinery, so
        // crossing zero mid-track has to re-arm: otherwise the next transition
        // still uses whichever mechanism the previous setting chose.
        if mode_changed && guard.crossfade.is_none() {
            guard.arm_next();
        }
    }

    /// Whether a fade is in flight - the polling loop watches this to tighten
    /// its interval, since a ramp stepped at the normal rate is audible.
    pub fn is_crossfading(&self) -> bool {
        self.inner.lock().unwrap().crossfade.is_some()
    }

    /// Playback speed of the deck that's playing, for MPRIS's `Rate`.
    pub fn current_tempo(&self) -> f64 {
        let guard = self.inner.lock().unwrap();
        if guard.output_mode == OutputMode::BitPerfect {
            1.0
        } else {
            guard.active().pitch.property::<f32>("tempo") as f64
        }
    }

    /// Points both decks at a fake sink, so tests can exercise real decoding
    /// and real transitions without needing an audio device to exist. Crate
    /// visible so the controller's tests drive the real engine too.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn use_fake_sinks(&self) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        let Inner { deck_a, deck_b, .. } = &mut *guard;
        for deck in [deck_a, deck_b] {
            let sink = gst::ElementFactory::make("fakesink")
                // Honour timestamps, so a file of a known length still takes
                // that long to play and transition timing stays meaningful.
                .property("sync", true)
                .build()?;
            deck.playbin.set_property("audio-sink", &sink);
            deck.normal_sink = Some(sink);
        }
        Ok(())
    }

    /// Starts (`Some`) or stops sending visualizer frames. Stopped, the
    /// spectrum elements post nothing.
    pub fn set_spectrum_sink(&self, sink: Option<SpectrumSink>) {
        let mut guard = self.inner.lock().unwrap();
        guard.spectrum_sink = sink.clone();
        let sink = if guard.output_mode == OutputMode::BitPerfect {
            None
        } else {
            sink
        };
        let on = sink.is_some();
        guard.visualizer.set_sink(sink);
        guard.visualizer.enable_on(&guard.deck_a.spectrum, on);
        guard.visualizer.enable_on(&guard.deck_b.spectrum, on);
    }

    pub fn set_replay_gain(&self, settings: ReplayGainSettings) {
        let mut guard = self.inner.lock().unwrap();
        guard.replay_gain_settings = settings;
        let settings = if guard.output_mode == OutputMode::BitPerfect {
            ReplayGainSettings::default()
        } else {
            settings
        };
        guard.deck_a.replay_gain.set_settings(settings);
        guard.deck_b.replay_gain.set_settings(settings);
    }

    pub fn set_equalizer_bands(&self, gains: [f64; EQ_BAND_COUNT]) {
        let mut guard = self.inner.lock().unwrap();
        guard.eq_gains = gains;
        guard.deck_a.apply_eq(&gains);
        guard.deck_b.apply_eq(&gains);
    }

    #[cfg(test)]
    pub(crate) fn equalizer_gains_for_test(&self) -> [[f64; EQ_BAND_COUNT]; 2] {
        let guard = self.inner.lock().unwrap();
        [&guard.deck_a, &guard.deck_b]
            .map(|deck| std::array::from_fn(|i| deck.eq.property::<f64>(&format!("band{i}"))))
    }

    /// Applies `tempo` to whichever deck (active or pre-rolled standby) currently
    /// has `path` loaded - a no-op if neither does. Tempo is per-track now, so
    /// unlike EQ/volume this can no longer be applied to both decks unconditionally:
    /// the active and standby decks may legitimately hold different tempos when
    /// two tracks with different saved speeds are adjacent in the queue.
    pub fn apply_tempo_for_path(&self, path: &str, tempo: f64) {
        let guard = self.inner.lock().unwrap();
        if guard.deck_a.path.as_deref() == Some(path) {
            guard.deck_a.apply_tempo(tempo);
        }
        if guard.deck_b.path.as_deref() == Some(path) {
            guard.deck_b.apply_tempo(tempo);
        }
    }

    pub fn status(&self) -> PlaybackStatus {
        let guard = self.inner.lock().unwrap();
        let p = &guard.active().playbin;
        let position_secs = p
            .query_position::<gst::ClockTime>()
            .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
            .unwrap_or(0.0);
        let duration_secs = p
            .query_duration::<gst::ClockTime>()
            .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
            .unwrap_or(0.0);
        let (_, current, _) = p.state(gst::ClockTime::ZERO);

        PlaybackStatus {
            is_playing: current == gst::State::Playing,
            position_secs,
            duration_secs,
            path: guard.current_path.clone(),
        }
    }

    /// Drains both decks' buses, progresses an in-flight crossfade (or triggers
    /// one/a gapless hard-cut if we're inside the switch window), and reports
    /// what happened this tick. Must be called from a single polling loop only -
    /// it consumes bus messages, so a second concurrent caller would race it for
    /// the same events.
    pub fn tick(&self) -> PlaybackTick {
        let mut guard = self.inner.lock().unwrap();

        let active_playbin = guard.active().playbin.clone();
        let standby_playbin = guard.standby_mut().playbin.clone();
        let active_bus = drain_bus(&active_playbin);
        // The standby deck's bus was never drained at all before, so a failed
        // pre-roll went unnoticed and its messages piled up for the lifetime of
        // the process.
        let standby_bus = drain_bus(&standby_playbin);

        let eos = active_bus.eos;
        let mut auto_advanced_to = None;
        let mut error = None;

        if let Some(message) = standby_bus.error {
            // Only the pre-load is lost. Drop it so the transition window can't
            // switch onto a deck that will never reach PLAYING; the frontend
            // re-arms whatever comes next on its own.
            let path = guard.standby_mut().path.take();
            standby_playbin.set_state(gst::State::Null).ok();
            guard.next_path = None;
            eprintln!("[muzon audio] pre-roll failed for {path:?}: {message}");
            error = Some(PlaybackError {
                path,
                message,
                fatal: false,
            });
        }

        if let Some(message) = active_bus.error {
            // The deck that was actually playing died: there's nothing left to
            // fade into or out of, so tear the transition down and let the
            // frontend decide whether to skip on. A dead active deck outranks a
            // failed pre-load in the single slot a tick has for reporting.
            guard.output_error = Some(message.clone());
            let path = guard.current_path.clone();
            active_playbin.set_state(gst::State::Null).ok();
            guard.crossfade = None;
            eprintln!("[muzon audio] playback failed for {path:?}: {message}");
            error = Some(PlaybackError {
                path,
                message,
                fatal: true,
            });
        } else if let Some(started_at) = guard.crossfade.as_ref().map(|c| c.started_at) {
            // A gradual crossfade is in progress - progress or finish it.
            let duration_secs = guard.crossfade.as_ref().unwrap().duration_secs;
            let t = (started_at.elapsed().as_secs_f64() / duration_secs).min(1.0);
            let master = cubic_to_linear(guard.volume);
            let active_is_a = guard.active_is_a;
            let (from_deck, to_deck) = if active_is_a {
                (&guard.deck_a, &guard.deck_b)
            } else {
                (&guard.deck_b, &guard.deck_a)
            };
            // Equal power, not a straight line: two linear ramps sum to roughly
            // -3dB at the midpoint, heard as the mix sagging through the
            // transition. sqrt keeps the two sides summing to one in *power*.
            from_deck.apply_linear_volume(master * (1.0 - t).sqrt());
            to_deck.apply_linear_volume(master * t.sqrt());
            // The bars follow whichever side is louder
            guard
                .visualizer
                .set_deck_a_audible(active_is_a == (t < 0.5));

            if t >= 1.0 {
                let from_playbin = from_deck.playbin.clone();
                from_playbin.set_state(gst::State::Null).ok();
                guard.active_is_a = !active_is_a;
                guard.crossfade = None;
                let new_path = guard.active().path.clone();
                guard.current_path = new_path.clone();
                auto_advanced_to = new_path.clone();
                eprintln!("[muzon audio] crossfade complete -> {new_path:?}");
            }
        } else if !guard.output_blocked
            && guard.output_mode == OutputMode::Normal
            && guard.crossfade_secs > 0.0
            && guard.next_path.is_some()
        {
            // Only crossfades are driven from here. A gapless transition needs
            // no window at all: playbin has already been handed the next URI
            // and joins the two streams itself.
            let pos = active_playbin
                .query_position::<gst::ClockTime>()
                .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
                .unwrap_or(0.0);
            let dur = active_playbin
                .query_duration::<gst::ClockTime>()
                .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
                .unwrap_or(0.0);
            let crossfade_secs = guard.crossfade_secs;
            let lookahead = crossfade_secs.max(MIN_CROSSFADE_LOOKAHEAD_SECS);

            if dur > 0.0 && (dur - pos) <= lookahead {
                let standby_playbin = guard.standby_mut().playbin.clone();
                // The standby deck was pre-rolled when the next track was
                // armed, so this reaches PLAYING almost immediately.
                standby_playbin.set_state(gst::State::Playing).ok();
                standby_playbin.set_property("volume", 0.0_f64);
                guard.crossfade = Some(CrossfadeState {
                    started_at: Instant::now(),
                    duration_secs: crossfade_secs,
                });
                eprintln!(
                    "[muzon audio] starting crossfade over {crossfade_secs}s (pos={pos:.2}/{dur:.2})"
                );
            }
        }

        let ended = eos && guard.crossfade.is_none() && auto_advanced_to.is_none();

        let status_playbin = guard.active().playbin.clone();
        let position_secs = status_playbin
            .query_position::<gst::ClockTime>()
            .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
            .unwrap_or(0.0);
        let duration_secs = status_playbin
            .query_duration::<gst::ClockTime>()
            .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
            .unwrap_or(0.0);
        let (_, current, _) = status_playbin.state(gst::ClockTime::ZERO);

        // Committing a gapless hand-off, which is subtler than it looks.
        //
        // `about-to-finish` fires as soon as the *source* is done pushing, and
        // playbin flips `current-uri` (and posts stream-start) right then -
        // while the sink still has the whole tail of the old track buffered.
        // Reporting the advance there moves the UI on seconds before anything
        // is heard. What does coincide with the audible join is the position
        // resetting, so that is what commits it. A user seek can't be mistaken
        // for one: `current-uri` still names the track we think is playing.
        if auto_advanced_to.is_none() && guard.current_path.is_some() {
            if let Some(path) = current_playbin_path(&status_playbin) {
                if guard.current_path.as_deref() != Some(path.as_str())
                    && position_secs < guard.last_position_secs
                {
                    guard.current_path = Some(path.clone());
                    guard.active_mut().path = Some(path.clone());
                    guard.next_path = None;
                    eprintln!("[muzon audio] gapless hand-off audible -> {path}");
                    auto_advanced_to = Some(path);
                }
            }
        }
        guard.last_position_secs = position_secs;

        PlaybackTick {
            is_playing: current == gst::State::Playing,
            position_secs,
            duration_secs,
            path: guard.current_path.clone(),
            ended,
            auto_advanced_to,
            error,
        }
    }
}

// A playbin bus keeps its sync handler alive. The spectrum handler captures
// that playbin, so releasing the Rust wrapper alone would retain the entire
// deck. Break that cycle and release codec/device resources on every exit.
impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let inner = self
            .inner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.visualizer.set_sink(None);
        for deck in [&inner.deck_a, &inner.deck_b] {
            let _ = deck.playbin.set_state(gst::State::Null);
            if let Some(bus) = deck.playbin.bus() {
                bus.unset_sync_handler();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_configuration_disables_processing_without_opening_hardware() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        if gst::ElementFactory::find("alsasink").is_none() {
            return;
        }
        player.set_volume(0.2).unwrap();
        player.set_crossfade_seconds(4.0);
        player
            .configure_output(
                OutputMode::BitPerfect,
                Some("hw:CARD=MuzonTestNonexistent,DEV=0".into()),
            )
            .unwrap();
        player.set_next_track(Some("/not/loaded.wav".into()));
        player.set_volume(0.01).unwrap();
        let guard = player.inner.lock().unwrap();
        for deck in [&guard.deck_a, &guard.deck_b] {
            assert_eq!(deck.playbin.state(gst::ClockTime::ZERO).1, gst::State::Null);
            assert!(deck
                .playbin
                .property::<Option<gst::Element>>("audio-filter")
                .is_none());
            assert_eq!(deck.playbin.property::<f64>("volume"), 1.0);
            assert_eq!(
                deck.playbin
                    .property_value("flags")
                    .get::<Vec<&gst::glib::FlagsValue>>()
                    .unwrap()
                    .iter()
                    .fold(0, |bits, flag| bits | flag.value()),
                0x22
            );
            assert!(deck.next_uri.lock().unwrap().is_none());
            let sink = deck.direct_sink.as_ref().unwrap();
            assert_eq!(sink.factory().unwrap().name(), "alsasink");
            assert_eq!(
                sink.property_value("slave-method")
                    .get::<&gst::glib::EnumValue>()
                    .unwrap()
                    .value(),
                2
            );
        }
        drop(guard);
        assert!(!player.output_status().bit_perfect_verified);
        assert!(player.output_status().negotiated_format.is_none());
        player.configure_output(OutputMode::Normal, None).unwrap();
        assert!(
            (player
                .inner
                .lock()
                .unwrap()
                .active()
                .playbin
                .property::<f64>("volume")
                - 0.000001)
                .abs()
                < 1e-10
        );
    }

    #[test]
    fn rejected_output_is_visible_and_blocks_silent_playback_fallback() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        assert!(player
            .configure_output(OutputMode::BitPerfect, Some("default".into()))
            .is_err());
        assert!(player.output_status().error.is_some());
        assert!(player.play().is_err());
        assert!(player.load_and_play("/missing.wav").is_err());
        player.configure_output(OutputMode::Normal, None).unwrap();
        assert!(player.output_status().error.is_none());
    }

    #[test]
    fn failed_output_keeps_resume_position_and_blocks_pause_and_preroll() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        player.use_fake_sinks().unwrap();
        let tone = write_tone("muzon-failed-output-recovery.wav", 3.0);
        player.load_paused_at(tone.to_str().unwrap(), 0.7).unwrap();
        assert!(player
            .configure_output(OutputMode::BitPerfect, Some("default".into()))
            .is_err());
        assert!(player.pause().is_err());
        player.set_crossfade_seconds(2.);
        player.set_next_track(Some(tone.to_string_lossy().into_owned()));
        {
            let guard = player.inner.lock().unwrap();
            for deck in [&guard.deck_a, &guard.deck_b] {
                assert_eq!(deck.playbin.state(gst::ClockTime::ZERO).1, gst::State::Null);
                assert!(deck.next_uri.lock().unwrap().is_none());
            }
        }
        player.configure_output(OutputMode::Normal, None).unwrap();
        let status = player.status();
        assert!(!status.is_playing);
        assert_eq!(status.path.as_deref(), tone.to_str());
        assert!(status.position_secs >= 0.65, "{}", status.position_secs);
        player.stop().unwrap();
    }

    #[test]
    fn normal_output_reconfiguration_restores_paused_position() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        player.use_fake_sinks().unwrap();
        let tone = write_tone("muzon-output-restore.wav", 3.0);
        player.load_paused_at(tone.to_str().unwrap(), 0.7).unwrap();
        // Device preference changes in Normal do not route to hardware.
        player
            .configure_output(OutputMode::Normal, Some("hw:CARD=Unused,DEV=0".into()))
            .unwrap();
        let status = player.status();
        assert!(!status.is_playing);
        assert_eq!(status.path.as_deref(), tone.to_str());
        assert!(status.position_secs >= 0.65, "{}", status.position_secs);
        player.stop().unwrap();
    }

    #[test]
    fn missing_file_reports_failure_and_a_later_track_can_play() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        player.use_fake_sinks().unwrap();
        let missing =
            std::env::temp_dir().join(format!("muzon-disappeared-{}.wav", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        let immediate = player.load_and_play(missing.to_str().unwrap());
        if immediate.is_ok() {
            assert!(tick_until_error(&player, 80).is_some());
        }
        let tone = write_tone("muzon-after-disappeared.wav", 1.0);
        player.load_and_play(tone.to_str().unwrap()).unwrap();
        tick_until(&player, 160, |tick| {
            tick.is_playing && tick.position_secs > 0.1 && tick.error.is_none()
        })
        .expect("recovery after missing disk/file");
        player.stop().unwrap();
    }

    /// Locks GStreamer and builds a player, or returns None when the machine
    /// has no usable GStreamer - these exercise real pipeline behaviour and
    /// there is nothing meaningful to assert about it in its absence.
    ///
    /// The lock is shared with the controller's tests (see
    /// [`crate::testing::gst_lock`]): they run in the same process, and two
    /// pipelines building at once race in GStreamer's registry.
    ///
    /// The returned guard has to stay alive for the body of the test, not just
    /// for construction: playback keeps touching the registry afterwards.
    fn locked_player() -> Option<(std::sync::MutexGuard<'static, ()>, AudioPlayer)> {
        let guard = crate::testing::gst_lock();
        Some((guard, AudioPlayer::new().ok()?))
    }

    /// A file that is definitely not decodable, which is how a missing codec, a
    /// truncated download or a corrupted file all present themselves.
    fn write_undecodable(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, b"this is not audio").unwrap();
        path
    }

    /// Polls the way the app's playback thread does, and returns the first tick
    /// that carried an error.
    fn tick_until_error(player: &AudioPlayer, attempts: u32) -> Option<PlaybackError> {
        tick_until(player, attempts, |tick| tick.error.is_some())
            .map(|(tick, _)| tick.error.unwrap())
    }

    /// Polls until `wanted` is happy, reporting the tick and how long it took -
    /// the timing is the point for transitions, not just that one happened.
    fn tick_until(
        player: &AudioPlayer,
        attempts: u32,
        wanted: impl Fn(&PlaybackTick) -> bool,
    ) -> Option<(PlaybackTick, std::time::Duration)> {
        let started = Instant::now();
        for _ in 0..attempts {
            let tick = player.tick();
            if wanted(&tick) {
                return Some((tick, started.elapsed()));
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        None
    }

    fn write_tone(name: &str, seconds: f64) -> std::path::PathBuf {
        crate::testing::write_tone(&std::env::temp_dir(), name, seconds)
    }

    #[test]
    fn a_gapless_follow_on_loses_none_of_the_first_track() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        if player.use_fake_sinks().is_err() {
            return;
        }
        let first = write_tone("muzon-test-gapless-first.wav", 0.6);
        let second = write_tone("muzon-test-gapless-second.wav", 0.6);

        player.set_crossfade_seconds(0.0);
        let started = Instant::now();
        player.load_and_play(first.to_str().unwrap()).unwrap();
        player.set_next_track(Some(second.to_string_lossy().to_string()));

        let (advance, _) = tick_until(&player, 400, |tick| tick.auto_advanced_to.is_some())
            .expect("an armed follow-on has to be handed over automatically");
        assert_eq!(advance.auto_advanced_to.as_deref(), second.to_str());

        tick_until(&player, 400, |tick| tick.ended)
            .expect("with nothing armed after it, the second track has to end the queue");

        // The point of handing the URI to playbin rather than cutting between
        // two decks: nothing is dropped at the join. Two 0.6s files contain
        // 1.2s of audio and must take about that long. The old two-deck switch
        // fired a fixed 0.3s before the end of each track, so the same pair
        // would have finished in around 0.9s.
        let total = started.elapsed().as_secs_f64();
        assert!(
            total >= 1.05,
            "the pair took {total:.3}s, short of the 1.2s of audio in them - \
             samples were dropped at the join"
        );
    }

    /// A FLAC whose vorbis comments carry a ReplayGain track gain, made by
    /// GStreamer itself: wavparse ignores ID3 chunks, so a tagged WAV would
    /// never deliver the tag this is meant to test.
    fn write_tagged_tone(name: &str, seconds: f64, track_gain_db: f64) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        // audiotestsrc's buffers are 1024 samples at 44.1 kHz. The gain is
        // written with decimals: "-6" parses as an integer, which the double
        // tag silently refuses.
        let buffers = (seconds * 44100.0 / 1024.0).ceil() as u32;
        let pipeline = gst::parse::launch(&format!(
            "audiotestsrc num-buffers={buffers} ! taginject tags=\"replaygain-track-gain={track_gain_db:.2}\" \
             ! audioconvert ! flacenc ! filesink location=\"{}\"",
            path.display()
        ))
        .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(10),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        pipeline.set_state(gst::State::Null).unwrap();
        path
    }

    fn active_replay_gain_factor(player: &AudioPlayer) -> f64 {
        player
            .inner
            .lock()
            .unwrap()
            .active()
            .replay_gain
            .current_factor()
    }

    #[test]
    fn replay_gain_follows_each_track_of_a_gapless_run() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        if player.use_fake_sinks().is_err() {
            return;
        }
        // The gain is set where the filter is, ahead of the sink's buffer, so
        // it moves on to the next track while the end of this one is still to
        // be heard - each sample has already been levelled on its way past.
        // A short file would be through the filter before the first check;
        // three seconds keeps it there long enough to look at.
        let tagged = write_tagged_tone("muzon-test-rg-tagged.flac", 3.0, -6.0);
        let untagged = write_tone("muzon-test-rg-untagged.wav", 0.6);

        player.set_crossfade_seconds(0.0);
        player.set_replay_gain(ReplayGainSettings {
            mode: crate::domain::playback_settings::ReplayGainMode::Track,
            preamp_db: 0.0,
        });
        player.load_and_play(tagged.to_str().unwrap()).unwrap();
        player.set_next_track(Some(untagged.to_string_lossy().to_string()));

        tick_until(&player, 200, |tick| tick.position_secs > 0.1)
            .expect("the tagged track has to start playing");
        let during_tagged = active_replay_gain_factor(&player);
        assert!(
            (during_tagged - 0.501).abs() < 0.01,
            "a -6 dB track gain should halve the amplitude, got {during_tagged}"
        );

        tick_until(&player, 600, |tick| tick.auto_advanced_to.is_some())
            .expect("the untagged track has to follow on");
        let during_untagged = active_replay_gain_factor(&player);
        assert_eq!(
            during_untagged, 1.0,
            "the previous track's gain must not carry over to one without tags"
        );

        player.set_replay_gain(ReplayGainSettings::default());
        assert_eq!(active_replay_gain_factor(&player), 1.0);
    }

    #[test]
    fn the_visualizer_sees_a_tone_where_it_is_and_stops_when_told() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        if player.use_fake_sinks().is_err() {
            return;
        }
        // write_tone is a 440 Hz sine
        let tone = write_tone("muzon-test-spectrum.wav", 1.5);
        let frames: Arc<Mutex<Vec<Vec<f32>>>> = Arc::default();
        let collected = frames.clone();
        player.set_spectrum_sink(Some(Arc::new(move |bars| {
            collected.lock().unwrap().push(bars)
        })));

        player.load_and_play(tone.to_str().unwrap()).unwrap();
        tick_until(&player, 200, |tick| tick.position_secs > 0.8).expect("the tone has to play");

        let received = frames.lock().unwrap().clone();
        assert!(
            received.len() >= 10,
            "only {} frames in 0.8s",
            received.len()
        );
        let last = received.last().unwrap();
        assert_eq!(last.len(), super::super::spectrum::BARS);
        let loudest = last
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        // 30 Hz..16 kHz over 48 log bars: 440 Hz sits around bar 20
        assert!(
            (18..=23).contains(&loudest),
            "loudest bar {loudest} for 440 Hz"
        );

        player.set_spectrum_sink(None);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let settled = frames.lock().unwrap().len();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(
            frames.lock().unwrap().len(),
            settled,
            "frames kept coming after stopping"
        );
        player.stop().unwrap();
    }

    #[test]
    fn a_crossfade_takes_the_two_deck_path_instead() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        if player.use_fake_sinks().is_err() {
            return;
        }
        let first = write_tone("muzon-test-crossfade-first.wav", 0.8);
        let second = write_tone("muzon-test-crossfade-second.wav", 0.8);

        player.set_crossfade_seconds(0.3);
        player.load_and_play(first.to_str().unwrap()).unwrap();
        player.set_next_track(Some(second.to_string_lossy().to_string()));

        let mut faded = false;
        for _ in 0..200 {
            player.tick();
            if player.is_crossfading() {
                faded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(
            faded,
            "a non-zero crossfade still has to run through both decks"
        );

        let (tick, _) = tick_until(&player, 200, |tick| tick.auto_advanced_to.is_some())
            .expect("the fade has to complete onto the next track");
        assert_eq!(tick.auto_advanced_to.as_deref(), second.to_str());
    }

    #[test]
    fn dropping_the_player_releases_playbin_and_its_bus_handler() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        let weak = player.inner.lock().unwrap().active().playbin.downgrade();
        drop(player);
        assert!(
            weak.upgrade().is_none(),
            "the spectrum bus handler retained its playbin"
        );
    }

    #[test]
    fn cubic_volume_is_gentler_at_the_bottom_of_the_slider_than_linear() {
        assert_eq!(cubic_to_linear(0.0), 0.0);
        assert_eq!(cubic_to_linear(1.0), 1.0);
        // Half travel is an eighth of the amplitude, which is roughly where it
        // is heard as half as loud - the whole reason for the scale.
        assert_eq!(cubic_to_linear(0.5), 0.125);
        assert_eq!(cubic_to_linear(2.0), 1.0, "out-of-range input is clamped");
    }

    #[test]
    fn an_undecodable_active_track_reports_a_fatal_error() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        let path = write_undecodable("muzon-test-broken-active.mp3");
        player.load_and_play(path.to_str().unwrap()).unwrap();

        let error = tick_until_error(&player, 40)
            .expect("a file that cannot be decoded has to be reported, not swallowed");
        assert!(
            error.fatal,
            "the deck that was playing died, so the queue has to be told to move on"
        );
        assert_eq!(error.path.as_deref(), path.to_str());

        // Drained, so it is a one-shot pulse rather than something the frontend
        // sees again on every subsequent tick.
        assert!(player.tick().error.is_none());
    }

    #[test]
    fn an_undecodable_crossfade_pre_roll_reports_a_non_fatal_error() {
        let Some((_gst, player)) = locked_player() else {
            return;
        };
        // Only a crossfade pre-rolls a second deck at all: gapless arms a URI
        // on the deck already playing, so there is no separate pre-roll left
        // that could fail on its own.
        player.set_crossfade_seconds(2.0);
        let path = write_undecodable("muzon-test-broken-next.wav");
        player.set_next_track(Some(path.to_string_lossy().to_string()));

        let error = tick_until_error(&player, 40)
            .expect("a pre-roll that fails must surface too - its bus was never even drained");
        assert!(
            !error.fatal,
            "only the pre-load was lost; whatever is playing must keep playing"
        );
        assert_eq!(error.path.as_deref(), path.to_str());
        assert!(player.tick().error.is_none());
    }
}
