use gstreamer as gst;
use gst::prelude::*;
use serde::Serialize;
use specta::Type;
use std::sync::Mutex;
use std::time::Instant;

pub const EQ_BAND_COUNT: usize = 10;

/// How far before the active track's natural end we trigger a transition, even
/// in gapless mode. Exists to (a) absorb our ~200ms polling granularity and
/// (b) give the *already pre-rolled* standby deck a moment to reach PLAYING,
/// which is expected to be near-instant since all the slow work (typefind,
/// demux, decoder negotiation) already happened while the current track was
/// still playing normally.
const MIN_LOOKAHEAD_SECS: f64 = 0.3;

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
    playbin: gst::Element,
    eq: gst::Element,
    pitch: gst::Element,
    path: Option<String>,
}

impl Deck {
    fn build() -> anyhow::Result<Self> {
        let playbin = gst::ElementFactory::make("playbin")
            .build()
            .or_else(|_| gst::ElementFactory::make("playbin3").build())?;

        let convert_in = gst::ElementFactory::make("audioconvert").build()?;
        let eq = gst::ElementFactory::make("equalizer-10bands").build()?;
        let pitch = gst::ElementFactory::make("pitch").build()?;
        let convert_out = gst::ElementFactory::make("audioconvert").build()?;

        let bin = gst::Bin::new();
        bin.add_many([&convert_in, &eq, &pitch, &convert_out])?;
        gst::Element::link_many([&convert_in, &eq, &pitch, &convert_out])?;

        let sink_pad = convert_in
            .static_pad("sink")
            .ok_or_else(|| anyhow::anyhow!("audioconvert has no sink pad"))?;
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

        Ok(Self {
            playbin,
            eq,
            pitch,
            path: None,
        })
    }

    fn apply_eq(&self, gains: &[f64; EQ_BAND_COUNT]) {
        for (i, gain) in gains.iter().enumerate() {
            self.eq.set_property(&format!("band{i}"), gain.clamp(-24.0, 12.0));
        }
    }

    fn apply_tempo(&self, tempo: f64) {
        self.pitch.set_property("tempo", tempo.clamp(0.25, 4.0) as f32);
    }
}

struct CrossfadeState {
    started_at: Instant,
    duration_secs: f64,
}

struct Inner {
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
    tempo: f64,
    crossfade: Option<CrossfadeState>,
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
}

pub struct AudioPlayer {
    inner: Mutex<Inner>,
}

fn path_to_uri(path: &str) -> anyhow::Result<String> {
    Ok(gst::glib::filename_to_uri(path, None)?.to_string())
}

fn drain_bus_for_eos(playbin: &gst::Element) -> bool {
    let mut eos = false;
    if let Some(bus) = playbin.bus() {
        while let Some(msg) = bus.pop() {
            if let gst::MessageView::Eos(_) = msg.view() {
                eos = true;
            }
        }
    }
    eos
}

impl AudioPlayer {
    pub fn new() -> anyhow::Result<Self> {
        gst::init()?;

        let inner = Inner {
            deck_a: Deck::build()?,
            deck_b: Deck::build()?,
            active_is_a: true,
            current_path: None,
            next_path: None,
            volume: 1.0,
            crossfade_secs: 0.0,
            eq_gains: [0.0; EQ_BAND_COUNT],
            tempo: 1.0,
            crossfade: None,
        };

        Ok(Self {
            inner: Mutex::new(inner),
        })
    }

    pub fn load_and_play(&self, path: &str) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.standby_mut().playbin.set_state(gst::State::Null).ok();
        guard.standby_mut().path = None;
        guard.crossfade = None;
        guard.next_path = None;

        let volume = guard.volume;
        let active_is_a = guard.active_is_a;
        let uri = path_to_uri(path)?;
        let deck = if active_is_a {
            &mut guard.deck_a
        } else {
            &mut guard.deck_b
        };
        deck.playbin.set_state(gst::State::Null)?;
        deck.playbin.set_property("uri", &uri);
        deck.playbin.set_property("volume", volume);
        deck.playbin.set_state(gst::State::Playing)?;
        deck.path = Some(path.to_string());
        guard.current_path = Some(path.to_string());
        Ok(())
    }

    pub fn play(&self) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap();
        guard.active().playbin.set_state(gst::State::Playing)?;
        Ok(())
    }

    pub fn pause(&self) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap();
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
            guard.active().playbin.set_property("volume", guard.volume);
        }
        Ok(())
    }

    pub fn volume(&self) -> f64 {
        self.inner.lock().unwrap().volume
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.deck_a.playbin.set_state(gst::State::Null).ok();
        guard.deck_b.playbin.set_state(gst::State::Null).ok();
        guard.deck_a.path = None;
        guard.deck_b.path = None;
        guard.current_path = None;
        guard.next_path = None;
        guard.crossfade = None;
        Ok(())
    }

    /// Tells the player what the frontend's queue would play next, and
    /// immediately pre-rolls it (uri set, Paused) on the standby deck so the
    /// slow part (typefind/demux/decoder setup) happens well before it's
    /// actually needed. Pass `None` when there is nothing next (end of queue,
    /// repeat off).
    pub fn set_next_track(&self, path: Option<String>) {
        let mut guard = self.inner.lock().unwrap();
        if guard.next_path == path {
            return;
        }
        guard.next_path = path.clone();

        // A transition is already under way; the standby deck is mid-swap and
        // about to become active - leave it alone.
        if guard.crossfade.is_some() {
            return;
        }

        let standby = guard.standby_mut();
        standby.playbin.set_state(gst::State::Null).ok();
        match &path {
            Some(next) => {
                if let Ok(uri) = path_to_uri(next) {
                    standby.playbin.set_property("uri", &uri);
                    standby.playbin.set_property("volume", 0.0_f64);
                    let _ = standby.playbin.set_state(gst::State::Paused);
                    standby.path = Some(next.clone());
                    eprintln!("[muzon audio] pre-rolling next track: {next}");
                }
            }
            None => {
                standby.path = None;
            }
        }
    }

    pub fn set_crossfade_seconds(&self, secs: f64) {
        self.inner.lock().unwrap().crossfade_secs = secs.max(0.0);
    }

    pub fn set_equalizer_bands(&self, gains: [f64; EQ_BAND_COUNT]) {
        let mut guard = self.inner.lock().unwrap();
        guard.eq_gains = gains;
        guard.deck_a.apply_eq(&gains);
        guard.deck_b.apply_eq(&gains);
    }

    pub fn set_tempo(&self, tempo: f64) {
        let mut guard = self.inner.lock().unwrap();
        guard.tempo = tempo;
        guard.deck_a.apply_tempo(tempo);
        guard.deck_b.apply_tempo(tempo);
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

    /// Drains EOS off the active deck's bus, progresses an in-flight crossfade
    /// (or triggers one/a gapless hard-cut if we're inside the switch window),
    /// and reports what happened this tick. Must be called from a single
    /// polling loop only - it consumes bus messages, so a second concurrent
    /// caller would race it for the same events.
    pub fn tick(&self) -> PlaybackTick {
        let mut guard = self.inner.lock().unwrap();

        let active_playbin = guard.active().playbin.clone();
        let eos = drain_bus_for_eos(&active_playbin);
        let mut auto_advanced_to = None;

        if let Some(started_at) = guard.crossfade.as_ref().map(|c| c.started_at) {
            // A gradual crossfade is in progress - progress or finish it.
            let duration_secs = guard.crossfade.as_ref().unwrap().duration_secs;
            let t = (started_at.elapsed().as_secs_f64() / duration_secs).min(1.0);
            let volume = guard.volume;
            let active_is_a = guard.active_is_a;
            let (from_playbin, to_playbin) = if active_is_a {
                (guard.deck_a.playbin.clone(), guard.deck_b.playbin.clone())
            } else {
                (guard.deck_b.playbin.clone(), guard.deck_a.playbin.clone())
            };
            from_playbin.set_property("volume", volume * (1.0 - t));
            to_playbin.set_property("volume", volume * t);

            if t >= 1.0 {
                from_playbin.set_state(gst::State::Null).ok();
                guard.active_is_a = !active_is_a;
                guard.crossfade = None;
                let new_path = guard.active().path.clone();
                guard.current_path = new_path.clone();
                auto_advanced_to = new_path.clone();
                eprintln!("[muzon audio] crossfade complete -> {new_path:?}");
            }
        } else if let Some(next) = guard.next_path.clone() {
            let pos = active_playbin
                .query_position::<gst::ClockTime>()
                .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
                .unwrap_or(0.0);
            let dur = active_playbin
                .query_duration::<gst::ClockTime>()
                .map(|c| c.nseconds() as f64 / 1_000_000_000.0)
                .unwrap_or(0.0);
            let lookahead = guard.crossfade_secs.max(MIN_LOOKAHEAD_SECS);

            if dur > 0.0 && (dur - pos) <= lookahead {
                let crossfade_secs = guard.crossfade_secs;
                let volume = guard.volume;
                let active_is_a = guard.active_is_a;
                let (active_for_swap, standby_playbin, standby_path) = if active_is_a {
                    (
                        guard.deck_a.playbin.clone(),
                        guard.deck_b.playbin.clone(),
                        guard.deck_b.path.clone(),
                    )
                } else {
                    (
                        guard.deck_b.playbin.clone(),
                        guard.deck_a.playbin.clone(),
                        guard.deck_a.path.clone(),
                    )
                };

                // The standby deck was already pre-rolled by `set_next_track`, so
                // this should reach PLAYING almost immediately.
                standby_playbin.set_state(gst::State::Playing).ok();

                if crossfade_secs <= 0.0 {
                    standby_playbin.set_property("volume", volume);
                    active_for_swap.set_state(gst::State::Null).ok();
                    guard.active_is_a = !active_is_a;
                    guard.crossfade = None;
                    guard.current_path = standby_path.clone();
                    auto_advanced_to = standby_path.clone();
                    eprintln!(
                        "[muzon audio] gapless switch -> {standby_path:?} (pos={pos:.2}/{dur:.2})"
                    );
                } else {
                    standby_playbin.set_property("volume", 0.0_f64);
                    guard.crossfade = Some(CrossfadeState {
                        started_at: Instant::now(),
                        duration_secs: crossfade_secs,
                    });
                    eprintln!(
                        "[muzon audio] starting crossfade to {next:?} over {crossfade_secs}s (pos={pos:.2}/{dur:.2})"
                    );
                }
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

        PlaybackTick {
            is_playing: current == gst::State::Playing,
            position_secs,
            duration_secs,
            path: guard.current_path.clone(),
            ended,
            auto_advanced_to,
        }
    }
}
